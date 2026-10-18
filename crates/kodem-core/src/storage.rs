//! One host-owned SQLite connection with immutable snapshot blobs.
use crate::{model::*, platform};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    collections::VecDeque,
    fs,
    path::{Path, PathBuf},
};

/// How many transcript writes a store remembers for watchers.
const RETAINED_CHANGES: usize = 1024;
/// Most changed rows that one answer to a watcher carries, as one transcript page.
const CHANGED_ROWS: usize = 100;

// Transcript writes of this open store, newest last. A row that is written again and again
// while it streams keeps one entry.
#[derive(Default)]
struct Changes {
    rev: u64,
    /// Newest revision that is no longer retained.
    floor: u64,
    rows: VecDeque<(u64, String, String)>,
}

pub struct Store {
    db: Connection,
    root: PathBuf,
    /// Identity of this open store. Revisions of another one are not comparable.
    epoch: String,
    changes: RefCell<Changes>,
}

/// A change to the submissions ledger that belongs to one lifecycle step of a task.
pub enum LedgerChange {
    /// Marks a queued message as in flight and records it in the transcript.
    Claim(OutgoingMessage),
    /// Records the provider turn that acknowledged a message.
    Confirm { id: String, turn: String },
    /// Keeps a refused or uncertain message recoverable.
    Fail {
        id: String,
        unknown: bool,
        error: String,
    },
}

/// Everything that one lifecycle step of a task stores.
pub struct Commit<'a> {
    pub task: &'a Task,
    pub ledger: &'a [LedgerChange],
    /// Runs that the step created or changed.
    pub runs: &'a [Run],
    /// Transcript rows that the step adds, written before the ledger changes.
    pub messages: &'a [Message],
    /// Publishes the final answer that was held until the whole agent tree settled.
    pub release_completion: bool,
}
/// Lists the provider conversations that the tasks of the Kodem store in `root` hold. The store
/// is read without changes, so another running build of Kodem keeps it. A missing store holds
/// none.
pub fn provider_threads(root: &Path) -> Result<Vec<(Provider, String)>> {
    let path = root.join("workspace.sqlite");
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let db = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    db.busy_timeout(std::time::Duration::from_secs(2))?;
    let mut statement = db.prepare(
        "SELECT json_extract(json, '$.provider'), json_extract(json, '$.provider_thread') \
         FROM tasks WHERE json_extract(json, '$.provider_thread') IS NOT NULL",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, Option<String>>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut threads = Vec::new();
    for row in rows {
        let (provider, thread) = row?;
        // Records from before the provider choice belong to Codex.
        let provider = provider
            .and_then(|provider| serde_json::from_value(serde_json::Value::String(provider)).ok())
            .unwrap_or_default();
        threads.push((provider, thread));
    }
    Ok(threads)
}

impl Store {
    /// Opens the local store, migrates its schema, and restricts access to the user.
    pub fn open(root: &Path) -> Result<Self> {
        fs::create_dir_all(root.join("snapshots"))?;
        platform::restrict(root, 0o700)?;
        let db = Connection::open(root.join("workspace.sqlite"))?;
        db.busy_timeout(std::time::Duration::from_secs(5))?;
        let version: u32 = db.pragma_query_value(None, "user_version", |row| row.get(0))?;
        ensure!(
            version <= 3,
            "This database was created by a newer Kodem version"
        );
        if version > 0 && version < 3 {
            let backup = root.join(format!("workspace.schema-{version}.{}.sqlite", new_id()));
            db.execute(
                "VACUUM main INTO ?1",
                [backup
                    .to_str()
                    .context("Database backup path is not UTF-8")?],
            )?;
            platform::restrict(&backup, 0o600)?;
        }
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; BEGIN IMMEDIATE; CREATE TABLE IF NOT EXISTS projects(id TEXT PRIMARY KEY, path TEXT UNIQUE NOT NULL, json TEXT NOT NULL); CREATE TABLE IF NOT EXISTS tasks(id TEXT PRIMARY KEY, json TEXT NOT NULL); CREATE TABLE IF NOT EXISTS messages(seq INTEGER PRIMARY KEY AUTOINCREMENT,id TEXT NOT NULL,task_id TEXT NOT NULL,json TEXT NOT NULL, UNIQUE(task_id,id)); CREATE INDEX IF NOT EXISTS messages_task_seq ON messages(task_id,seq); CREATE TABLE IF NOT EXISTS settings(id INTEGER PRIMARY KEY CHECK(id=1),json TEXT NOT NULL); CREATE TABLE IF NOT EXISTS submissions(id TEXT PRIMARY KEY,task_id TEXT NOT NULL,prompt TEXT NOT NULL,state TEXT NOT NULL,feedback TEXT NOT NULL); ")?;
        if version < 2 {
            db.execute_batch(
                "ALTER TABLE submissions ADD COLUMN receipt TEXT; PRAGMA user_version=2;",
            )?;
        }
        if version < 3 {
            db.execute_batch(
                "ALTER TABLE submissions ADD COLUMN payload TEXT; PRAGMA user_version=3;",
            )?;
        }
        db.execute_batch("CREATE TABLE IF NOT EXISTS agent_message_order(transcript TEXT NOT NULL, id TEXT NOT NULL, position INTEGER NOT NULL, PRIMARY KEY(transcript,id)); CREATE TABLE IF NOT EXISTS runs(id TEXT PRIMARY KEY, task_id TEXT NOT NULL, ordinal INTEGER NOT NULL, json TEXT NOT NULL, UNIQUE(task_id,ordinal)); COMMIT;")?;
        retire_removed_providers(&db)?;
        repair_settings_collections(&db)?;
        assign_project_avatars(&db)?;
        platform::restrict(&root.join("workspace.sqlite"), 0o600)?;
        Ok(Self {
            db,
            root: root.to_owned(),
            epoch: new_id(),
            changes: RefCell::default(),
        })
    }
    /// Returns the identity of this open store and the revision of its newest transcript write.
    pub fn revision(&self) -> (&str, u64) {
        (&self.epoch, self.changes.borrow().rev)
    }
    /// Returns the rows of one transcript that were written after a revision of this store.
    ///
    /// `None` means that the rows are not known exactly or do not fit one answer: the revision
    /// is older than the retained writes, or more rows changed than one page holds. The caller
    /// then reads the newest page.
    pub fn changed_messages(&self, transcript: &str, after: u64) -> Result<Option<Vec<Message>>> {
        let ids: Vec<String> = {
            let changes = self.changes.borrow();
            if after < changes.floor || after > changes.rev {
                return Ok(None);
            }
            let mut ids: Vec<String> = Vec::new();
            for (rev, owner, id) in &changes.rows {
                if *rev > after && owner == transcript && !ids.contains(id) {
                    ids.push(id.clone());
                }
            }
            ids
        };
        if ids.len() > CHANGED_ROWS {
            return Ok(None);
        }
        let mut statement = self
            .db
            .prepare("SELECT seq,json FROM messages WHERE task_id=?1 AND id=?2")?;
        let mut messages = Vec::new();
        let mut bytes = 0usize;
        for id in ids {
            let row = statement
                .query_row(params![transcript, id], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
                })
                .optional()?;
            // A row that was removed after its write is no news.
            let Some((seq, json)) = row else {
                continue;
            };
            bytes += json.len();
            if bytes > 4 * 1024 * 1024 {
                return Ok(None);
            }
            let mut message: Message = serde_json::from_str(&json)?;
            message.seq = u64::try_from(seq)?;
            messages.push(message);
        }
        messages.sort_by_key(|message| message.seq);
        Ok(Some(messages))
    }
    /// Marks interrupted host-owned tasks without replaying external submissions.
    pub fn recover(&self) -> Result<()> {
        for mut task in self.tasks()? {
            let pending = self.has_uncertain_submission(&task.id)?;
            if task.state.active() || pending {
                task.state = RunState::DeliveryUnknown;
                task.error=Some("The host stopped before completion was confirmed. History is retained. No prompt was resent. Review the provider history before starting another turn.".into());
                task.active_turn = None;
                task.pending_completion = None;
                for agent in &mut task.subagents {
                    if agent.state.active() {
                        agent.state = RunState::DeliveryUnknown;
                        agent.active_turn = None;
                    }
                }
                self.save_task(&task)?;
                // The run stays open: reconciliation can still prove its outcome.
                if let Some(mut run) = self.latest_run(&task.id)?
                    && run.ended.is_none()
                {
                    run.state = RunState::DeliveryUnknown;
                    self.save_run(&run)?;
                }
            } else if matches!(task.state, RunState::Idle | RunState::Completed)
                && self
                    .outgoing(&task.id)?
                    .iter()
                    .any(|message| message.state == DeliveryState::Queued)
            {
                task.state = RunState::Interrupted;
                task.error = Some(
                    "The host stopped before queued input was sent. Resume the queue to continue."
                        .into(),
                );
                self.save_task(&task)?;
            }
        }
        self.db.execute(
            "UPDATE submissions SET state='unknown' WHERE state='pending'",
            [],
        )?;
        Ok(())
    }
    /// Returns projects currently registered in the workspace.
    pub fn projects(&self) -> Result<Vec<Project>> {
        let projects: Vec<Project> = self.read_json("SELECT json FROM projects ORDER BY path")?;
        Ok(projects
            .into_iter()
            .filter(|project| !project.removed)
            .collect())
    }
    /// Saves project presentation and commands without changing its repository identity.
    pub fn customize_project(
        &self,
        id: &str,
        name: &str,
        color: Option<u32>,
        icon: ProjectIcon,
        avatar: Option<SkaterAvatar>,
        commands: Vec<ProjectCommand>,
    ) -> Result<()> {
        ensure!(
            !name.trim().is_empty() && name.len() < 512 && !name.chars().any(char::is_control),
            "Enter a project name under 512 bytes without control characters"
        );
        ensure!(
            color.is_none_or(|color| color <= 0xffffff),
            "Invalid project color"
        );
        ensure!(commands.len() <= 64, "A project holds up to 64 commands");
        for command in &commands {
            ensure!(
                command.name.len() < 512 && !command.name.chars().any(char::is_control),
                "Enter command names under 512 bytes without control characters"
            );
            ensure!(
                !command.steps.is_empty(),
                "Add a step to {}, or remove it",
                command.label()
            );
            ensure!(
                command.steps.len() <= ProjectCommand::MAX_STEPS,
                "A command holds up to {} steps",
                ProjectCommand::MAX_STEPS
            );
            // A step is typed into a shell as one line, so it must be text without line breaks.
            ensure!(
                command.steps.iter().all(|step| step
                    .to_str()
                    .is_some_and(|text| !text.is_empty() && !text.contains(char::is_control))),
                "The steps of {} need paths without control characters",
                command.label()
            );
        }
        let mut project = self
            .projects()?
            .into_iter()
            .find(|p| p.id == id)
            .context("Project not found")?;
        project.name = name.trim().into();
        project.color = color;
        project.icon = icon;
        // A project keeps its avatar when the request names none.
        if avatar.is_some() {
            project.avatar = avatar;
        }
        project.commands = commands;
        self.save_project(&project)
    }
    /// Removes a project from navigation while retaining its files and restorable history.
    pub fn remove_project(&self, id: &str) -> Result<()> {
        let mut project = self
            .projects()?
            .into_iter()
            .find(|p| p.id == id)
            .context("Project not found")?;
        let tasks = self.tasks()?;
        for task in tasks.iter().filter(|t| t.project_id == id) {
            ensure!(
                !task.state.active()
                    && task.active_turn.is_none()
                    && task.state != RunState::DeliveryUnknown
                    && !self.has_uncertain_submission(&task.id)?
                    && !self
                        .outgoing(&task.id)?
                        .iter()
                        .any(|m| m.state == DeliveryState::Queued),
                "Finish or reconcile this project's tasks and cancel queued messages before removing it"
            );
        }
        project.removed = true;
        let mut settings = self.settings()?;
        if tasks
            .iter()
            .any(|t| t.project_id == id && settings.selected_task.as_ref() == Some(&t.id))
        {
            settings.selected_task = None;
        }
        let transaction = self.db.unchecked_transaction()?;
        self.save_project(&project)?;
        self.write_settings(&settings)?;
        transaction.commit()?;
        Ok(())
    }
    fn save_project(&self, project: &Project) -> Result<()> {
        self.db.execute(
            "UPDATE projects SET json=?1 WHERE id=?2",
            params![serde_json::to_string(project)?, project.id],
        )?;
        Ok(())
    }
    /// Adds a project once, using its canonical path as the identity boundary.
    pub fn add_project(&self, path: PathBuf) -> Result<Project> {
        let projects: Vec<Project> = self.read_json("SELECT json FROM projects")?;
        if let Some(mut p) = projects.into_iter().find(|p| p.path == path) {
            p.removed = false;
            self.save_project(&p)?;
            return Ok(p);
        }
        let p = Project {
            id: new_id(),
            name: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into(),
            path,
            pinned: false,
            color: None,
            icon: ProjectIcon::default(),
            avatar: Some(SkaterAvatar::random()),
            removed: false,
            defaults: None,
            commands: Vec::new(),
        };
        self.db.execute(
            "INSERT INTO projects VALUES(?1,?2,?3)",
            params![p.id, p.path.to_string_lossy(), serde_json::to_string(&p)?],
        )?;
        Ok(p)
    }
    /// Returns persisted task summaries.
    pub fn tasks(&self) -> Result<Vec<Task>> {
        self.read_json("SELECT json FROM tasks")
    }
    /// Reports whether the agent of any task is starting or running a turn. It reads only the
    /// state of each record, so the host can ask often.
    pub fn working(&self) -> Result<bool> {
        Ok(self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks \
             WHERE json_extract(json, '$.state') IN ('Starting', 'Running'))",
            [],
            |r| r.get(0),
        )?)
    }
    /// Reads one task or reports that it no longer exists.
    pub fn task(&self, id: &str) -> Result<Task> {
        let json: String = self
            .db
            .query_row("SELECT json FROM tasks WHERE id=?1", [id], |r| r.get(0))
            .context("Task not found")?;
        Ok(serde_json::from_str(&json)?)
    }
    /// Deletes an idle task and its local conversation records in one transaction.
    pub fn delete_task(&mut self, id: &str) -> Result<()> {
        let task = self.task(id)?;
        ensure!(
            !task.state.active()
                && task.active_turn.is_none()
                && task.state != RunState::DeliveryUnknown
                && !self.has_uncertain_submission(id)?,
            "Finish or reconcile this task before deleting it"
        );
        ensure!(
            !self
                .outgoing(id)?
                .iter()
                .any(|message| message.state == DeliveryState::Queued),
            "Cancel queued messages before deleting this task"
        );
        let mut settings = self.settings()?;
        if settings.selected_task.as_deref() == Some(id) {
            settings.selected_task = None;
        }
        let transaction = self.db.transaction()?;
        transaction.execute("DELETE FROM messages WHERE task_id=?1", [id])?;
        transaction.execute(
            "DELETE FROM messages WHERE task_id=?1",
            [format!("{id}/completion")],
        )?;
        for agent in &task.subagents {
            transaction.execute(
                "DELETE FROM messages WHERE task_id=?1",
                [agent.transcript_id(id)],
            )?;
            transaction.execute(
                "DELETE FROM agent_message_order WHERE transcript=?1",
                [agent.transcript_id(id)],
            )?;
        }
        transaction.execute("DELETE FROM submissions WHERE task_id=?1", [id])?;
        transaction.execute("DELETE FROM runs WHERE task_id=?1", [id])?;
        transaction.execute("DELETE FROM tasks WHERE id=?1", [id])?;
        transaction.execute(
            "INSERT INTO settings VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET json=excluded.json",
            [serde_json::to_string(&settings)?],
        )?;
        transaction.commit()?;
        Ok(())
    }
    /// Saves a task independently of other tasks and their drafts.
    pub fn save_task(&self, task: &Task) -> Result<()> {
        self.db.execute(
            "INSERT INTO tasks VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET json=excluded.json",
            params![task.id, serde_json::to_string(task)?],
        )?;
        Ok(())
    }
    /// Returns persisted window preferences or their defaults.
    pub fn settings(&self) -> Result<Settings> {
        let s: Option<String> = self
            .db
            .query_row("SELECT json FROM settings WHERE id=1", [], |r| r.get(0))
            .optional()?;
        Ok(s.map(|s| Settings::from_json(&s))
            .transpose()?
            .unwrap_or_default())
    }
    /// Persists bounded pane sizes and typography choices.
    pub fn save_settings(&self, s: &Settings) -> Result<()> {
        let mut settings = s.clone();
        // Agent defaults are host-owned; an older layout snapshot must not overwrite them.
        let saved = self.settings()?;
        settings.agent_defaults = saved.agent_defaults;
        settings.provider_preferences = saved.provider_preferences;
        settings.model_choices = saved.model_choices;
        settings.access_choices = saved.access_choices;
        settings.accounts = saved.accounts;
        self.write_settings(&settings)
    }
    /// Replaces the account registry without touching window preferences.
    pub fn save_accounts(&self, accounts: Vec<Account>) -> Result<()> {
        let mut settings = self.settings()?;
        settings.accounts = accounts;
        self.write_settings(&settings)
    }
    /// Forgets an account and returns saved choices for new tasks to the default account.
    ///
    /// Existing tasks keep their reference, so a removed identity is never replaced silently.
    pub fn remove_account(&self, provider: Provider, id: &str) -> Result<()> {
        let selects = |preferences: &AgentPreferences| {
            preferences.provider == provider && preferences.account.as_deref() == Some(id)
        };
        let mut settings = self.settings()?;
        settings
            .accounts
            .retain(|account| !(account.provider == provider && account.id == id));
        if selects(&settings.agent_defaults) {
            settings.agent_defaults.account = None;
        }
        for preferences in settings.provider_preferences.values_mut() {
            if selects(preferences) {
                preferences.account = None;
            }
        }
        let projects: Vec<Project> = self.read_json("SELECT json FROM projects")?;
        let transaction = self.db.unchecked_transaction()?;
        self.write_settings(&settings)?;
        for mut project in projects {
            if project.defaults.as_ref().is_some_and(selects) {
                if let Some(defaults) = &mut project.defaults {
                    defaults.account = None;
                }
                self.save_project(&project)?;
            }
        }
        transaction.commit()?;
        Ok(())
    }
    /// Clears a model identity that a provider no longer lists from tasks and saved choices.
    ///
    /// An absent model selects the provider's default, so nothing keeps naming a removed entry.
    pub fn retire_model(&self, provider: Provider, model: &str) -> Result<()> {
        let names = |preferences: &AgentPreferences| {
            preferences.provider == provider && preferences.model.as_deref() == Some(model)
        };
        let transaction = self.db.unchecked_transaction()?;
        for mut task in self.tasks()? {
            if task.provider == provider && task.model.as_deref() == Some(model) {
                task.model = None;
                self.save_task(&task)?;
            }
        }
        let projects: Vec<Project> = self.read_json("SELECT json FROM projects")?;
        for mut project in projects {
            if let Some(defaults) = project.defaults.as_mut().filter(|defaults| names(defaults)) {
                defaults.model = None;
                self.save_project(&project)?;
            }
        }
        let mut settings = self.settings()?;
        let mut changed = false;
        let prefix = format!("{}/", provider.icon());
        let saved = std::iter::once(&mut settings.agent_defaults)
            .chain(settings.provider_preferences.values_mut())
            .filter(|preferences| names(preferences))
            .map(|preferences| &mut preferences.model)
            .chain(
                settings
                    .model_choices
                    .iter_mut()
                    .filter(|(key, choice)| {
                        key.starts_with(&prefix) && choice.model.as_deref() == Some(model)
                    })
                    .map(|(_, choice)| &mut choice.model),
            );
        for choice in saved {
            *choice = None;
            changed = true;
        }
        changed |= settings
            .access_choices
            .remove(&AutoCompact::model_key(provider, model))
            .is_some();
        if changed {
            self.write_settings(&settings)?;
        }
        transaction.commit()?;
        Ok(())
    }
    /// Sets or clears the options that new tasks in one project start with.
    pub fn set_project_defaults(&self, id: &str, defaults: Option<AgentPreferences>) -> Result<()> {
        let mut project = self
            .projects()?
            .into_iter()
            .find(|p| p.id == id)
            .context("Project not found")?;
        project.defaults = defaults;
        self.save_project(&project)
    }
    /// Saves the last provider and that provider's model options before a task is created.
    pub fn save_agent_preferences(&self, defaults: AgentPreferences) -> Result<()> {
        let mut settings = self.settings()?;
        settings.keep_agent_preferences(defaults);
        self.write_settings(&settings)
    }
    /// Saves options as [`Store::save_agent_preferences`] does, and records their approval rule
    /// as the choice for their model.
    pub fn save_access_choice(&self, defaults: AgentPreferences) -> Result<()> {
        let mut settings = self.settings()?;
        settings.remember_access(&defaults);
        settings.keep_agent_preferences(defaults);
        self.write_settings(&settings)
    }
    /// Records the approval rule of `preferences` as the last choice for its model.
    pub fn remember_access(&self, preferences: &AgentPreferences) -> Result<()> {
        let mut settings = self.settings()?;
        settings.remember_access(preferences);
        self.write_settings(&settings)
    }
    /// Records the model a new task starts with as the last choice of its account and project.
    pub fn remember_model(&self, preferences: &AgentPreferences, project: &str) -> Result<()> {
        let mut settings = self.settings()?;
        settings.remember_model(preferences, Some(project));
        self.write_settings(&settings)
    }
    /// Atomically saves an accepted task preference change and the defaults for new tasks.
    pub fn save_task_preferences(&self, task: &Task, defaults: AgentPreferences) -> Result<()> {
        let mut settings = self.settings()?;
        settings
            .provider_preferences
            .entry(settings.agent_defaults.provider)
            .or_insert_with(|| settings.agent_defaults.clone());
        settings
            .provider_preferences
            .insert(defaults.provider, defaults.clone());
        settings.remember_model(&defaults, Some(&task.project_id));
        settings.agent_defaults = defaults;
        let transaction = self.db.unchecked_transaction()?;
        self.save_task(task)?;
        self.write_settings(&settings)?;
        transaction.commit()?;
        Ok(())
    }
    fn write_settings(&self, s: &Settings) -> Result<()> {
        ensure!(
            s.sidebar_width.is_finite()
                && s.review_width.is_finite()
                && s.font_size.is_finite()
                && s.code_font_size.is_finite()
                && s.ui_scale.is_finite(),
            "Invalid layout value"
        );
        let mut s = s.clone();
        s.sidebar_width = s.sidebar_width.clamp(220., 420.);
        s.review_width = s.review_width.clamp(320., 900.);
        let (smallest, largest) = Settings::FONT_SIZE_RANGE;
        s.font_size = s.font_size.clamp(smallest, largest);
        let (smallest, largest) = Settings::CODE_FONT_SIZE_RANGE;
        s.code_font_size = s.code_font_size.clamp(smallest, largest);
        let (smallest, largest) = Settings::UI_SCALE_RANGE;
        s.ui_scale = s.ui_scale.clamp(smallest, largest);
        self.db.execute(
            "INSERT INTO settings VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET json=excluded.json",
            [serde_json::to_string(&s)?],
        )?;
        Ok(())
    }
    /// Writes or replaces a provider item while preserving its local sequence.
    pub fn message(&self, m: &Message) -> Result<()> {
        ensure!(
            m.text.len() <= 2 * 1024 * 1024,
            "Message exceeds the 2 MiB storage limit"
        );
        // A streamed row is written again as it grows, so it is serialized once per write.
        let json = serde_json::to_string(m)?;
        ensure!(
            json.len() <= 4 * 1024 * 1024,
            "Message and attached context exceed 4 MiB"
        );
        self.db.execute("INSERT INTO messages(id,task_id,json) VALUES(?1,?2,?3) ON CONFLICT(task_id,id) DO UPDATE SET json=excluded.json",params![m.id,m.task_id,json])?;
        let mut changes = self.changes.borrow_mut();
        changes.rev += 1;
        let rev = changes.rev;
        match changes.rows.back_mut() {
            Some((last, transcript, id)) if *transcript == m.task_id && *id == m.id => *last = rev,
            _ => {
                changes
                    .rows
                    .push_back((rev, m.task_id.clone(), m.id.clone()));
                if changes.rows.len() > RETAINED_CHANGES
                    && let Some((dropped, ..)) = changes.rows.pop_front()
                {
                    changes.floor = dropped;
                }
            }
        }
        Ok(())
    }
    /// Checks a transcript item before filling gaps from provider history.
    pub fn has_message(&self, task_id: &str, id: &str) -> Result<bool> {
        Ok(self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM messages WHERE task_id=?1 AND id=?2)",
            params![task_id, id],
            |row| row.get(0),
        )?)
    }
    /// Removes a staged item when the provider identifies it as commentary.
    pub fn remove_message(&self, task_id: &str, id: &str) -> Result<()> {
        self.db.execute(
            "DELETE FROM messages WHERE task_id=?1 AND id=?2",
            params![task_id, id],
        )?;
        Ok(())
    }
    /// Publishes the main final answer only after the entire agent tree settles.
    pub fn release_completion_messages(&self, task_id: &str) -> Result<()> {
        let source = format!("{task_id}/completion");
        let mut statement = self
            .db
            .prepare("SELECT json FROM messages WHERE task_id=?1 ORDER BY seq")?;
        let rows = statement.query_map([&source], |row| row.get::<_, String>(0))?;
        for row in rows {
            let mut message: Message = serde_json::from_str(&row?)?;
            message.task_id = task_id.into();
            self.message(&message)?;
        }
        self.db
            .execute("DELETE FROM messages WHERE task_id=?1", [&source])?;
        Ok(())
    }
    /// Records authoritative child item order independently of notification arrival time.
    pub fn order_agent_message(&self, transcript: &str, id: &str, position: u64) -> Result<()> {
        self.db.execute("INSERT INTO agent_message_order(transcript,id,position) VALUES(?1,?2,?3) ON CONFLICT(transcript,id) DO NOTHING", params![transcript, id, i64::try_from(position)?])?;
        Ok(())
    }
    /// Reads a bounded child page in provider order, keeping new live items at the tail.
    pub fn agent_messages(
        &self,
        task: &str,
        before: Option<u64>,
    ) -> Result<(Vec<Message>, Option<u64>)> {
        self.transcript_page(task, before, true)
    }
    /// Reads a bounded transcript page in chronological order.
    pub fn messages(&self, task: &str, before: Option<u64>) -> Result<(Vec<Message>, Option<u64>)> {
        self.transcript_page(task, before, false)
    }
    fn transcript_page(
        &self,
        task: &str,
        before: Option<u64>,
        agent: bool,
    ) -> Result<(Vec<Message>, Option<u64>)> {
        let mut stmt = self.db.prepare(if agent {
            "SELECT COALESCE(o.position,1099511627776+m.seq) AS ordinal,m.json FROM messages m LEFT JOIN agent_message_order o ON o.transcript=m.task_id AND o.id=m.id WHERE m.task_id=?1 AND ordinal<?2 ORDER BY ordinal DESC LIMIT 101"
        } else {
            "SELECT seq,json FROM messages WHERE task_id=?1 AND seq<?2 ORDER BY seq DESC LIMIT 101"
        })?;
        let mut rows = stmt.query(params![
            task,
            before
                .and_then(|n| i64::try_from(n).ok())
                .unwrap_or(i64::MAX)
        ])?;
        let mut messages = Vec::new();
        let mut bytes = 0usize;
        let mut byte_limit = false;
        while let Some(row) = rows.next()? {
            let json = row.get::<_, String>(1)?;
            let mut m: Message = serde_json::from_str(&json)?;
            m.seq = u64::try_from(row.get::<_, i64>(0)?)?;
            if !messages.is_empty() && bytes + json.len() > 4 * 1024 * 1024 {
                byte_limit = true;
                break;
            }
            bytes += json.len();
            messages.push(m);
        }
        let more = messages.len() > 100 || byte_limit;
        messages.truncate(100);
        messages.reverse();
        let before = if more {
            messages.first().map(|m| m.seq)
        } else {
            None
        };
        Ok((messages, before))
    }
    /// Commits outgoing intent before transport and deduplicates retries by submission ID.
    pub fn begin_submission(
        &self,
        id: &str,
        task: &str,
        prompt: &str,
        feedback: &[String],
    ) -> Result<bool> {
        Ok(self.db.execute(
            "INSERT OR IGNORE INTO submissions(id,task_id,prompt,state,feedback) VALUES(?1,?2,?3,'pending',?4)",
            params![id, task, prompt, serde_json::to_string(feedback)?],
        )? > 0)
    }
    /// Saves delivery acknowledgement independently from the provider turn outcome.
    pub fn delivery(&self, id: &str, state: &str) -> Result<()> {
        self.db.execute(
            "UPDATE submissions SET state=?2 WHERE id=?1",
            params![id, state],
        )?;
        Ok(())
    }
    /// Records the acknowledged provider turn for later identity-based reconciliation.
    pub fn confirm_submission(&self, id: &str, turn: &str) -> Result<()> {
        self.db.execute(
            "UPDATE submissions SET state='acknowledged',receipt=?2 WHERE id=?1",
            params![id, turn],
        )?;
        Ok(())
    }
    /// Returns the newest outgoing intent without replaying it.
    pub fn latest_submission(&self, task_id: &str) -> Result<Option<SubmissionRecord>> {
        let row=self.db.query_row("SELECT id,prompt,state,receipt,feedback,payload FROM submissions WHERE task_id=?1 AND state NOT IN ('queued','cancelled') ORDER BY CASE WHEN state IN ('pending','unknown') THEN 0 ELSE 1 END,rowid DESC LIMIT 1",[task_id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,Option<String>>(3)?,r.get::<_,String>(4)?,r.get::<_,Option<String>>(5)?))).optional()?;
        row.map(|(id, prompt, state, receipt, feedback, payload)| {
            Ok(SubmissionRecord {
                id,
                prompt,
                state,
                receipt,
                feedback: serde_json::from_str(&feedback)?,
                outgoing: payload.map(|s| serde_json::from_str(&s)).transpose()?,
            })
        })
        .transpose()
    }
    /// Resolves all locally pending intents after a transport owner exits.
    pub fn settle_pending(&self, task_id: &str, unknown: bool) -> Result<()> {
        self.db.execute(
            "UPDATE submissions SET state=?2 WHERE task_id=?1 AND state='pending'",
            params![task_id, if unknown { "unknown" } else { "failed" }],
        )?;
        Ok(())
    }
    /// Finds an accepted identity before task-state checks and rejects cross-task reuse.
    pub fn has_submission(&self, task_id: &str, id: &str) -> Result<bool> {
        let owner: Option<String> = self
            .db
            .query_row("SELECT task_id FROM submissions WHERE id=?1", [id], |r| {
                r.get(0)
            })
            .optional()?;
        ensure!(
            owner.as_deref().is_none_or(|owner| owner == task_id),
            "Submission identity belongs to another task"
        );
        Ok(owner.is_some())
    }
    /// Deduplicates an exact input identity and rejects reuse for different content.
    pub fn matches_submission(
        &self,
        task_id: &str,
        id: &str,
        text: &str,
        feedback_ids: &[String],
        attachment_ids: &[String],
        mode: SendMode,
    ) -> Result<bool> {
        if !self.has_submission(task_id, id)? {
            return Ok(false);
        }
        let payload: Option<String> =
            self.db
                .query_row("SELECT payload FROM submissions WHERE id=?1", [id], |r| {
                    r.get(0)
                })?;
        let message: OutgoingMessage = serde_json::from_str(
            &payload.context("Submission identity belongs to an older input")?,
        )?;
        ensure!(
            message.text == text
                && message.mode == mode
                && message
                    .feedback
                    .iter()
                    .map(|f| &f.id)
                    .eq(feedback_ids.iter())
                && message
                    .attachments
                    .iter()
                    .map(|a| &a.id)
                    .eq(attachment_ids.iter()),
            "Submission identity was reused for different input"
        );
        Ok(true)
    }
    /// Reports uncertain deliveries even when later queued messages exist.
    pub fn has_uncertain_submission(&self, task_id: &str) -> Result<bool> {
        Ok(self.db.query_row("SELECT EXISTS(SELECT 1 FROM submissions WHERE task_id=?1 AND state IN ('pending','unknown'))", [task_id], |r| r.get(0))?)
    }
    /// Atomically records frozen input and consumes only its matching composer content.
    /// Adds input starting an idle task to chat before provider startup.
    pub fn accept_outgoing(
        &self,
        task: &Task,
        message: &OutgoingMessage,
        prompt: &str,
    ) -> Result<()> {
        let payload = serde_json::to_string(message)?;
        let retained: i64 = self.db.query_row("SELECT COALESCE(SUM(length(CAST(payload AS BLOB))),0) FROM submissions WHERE task_id=?1 AND state IN ('queued','pending','failed','unknown')", [&task.id], |r| r.get(0))?;
        ensure!(
            retained + payload.len() as i64 <= 8 * 1024 * 1024,
            "Outbox content exceeds 8 MiB; send or remove existing messages first"
        );
        let tx = self.db.unchecked_transaction()?;
        let starts_turn = !task.state.active()
            && !self.db.query_row(
                "SELECT EXISTS(SELECT 1 FROM submissions WHERE task_id=?1 AND state IN ('queued','pending','unknown'))",
                [&task.id],
                |row| row.get::<_, bool>(0),
            )?;
        let feedback: Vec<_> = message.feedback.iter().map(|f| &f.id).collect();
        self.db.execute("INSERT INTO submissions(id,task_id,prompt,state,feedback,payload) VALUES(?1,?2,?3,'queued',?4,?5)", params![message.id,task.id,prompt,serde_json::to_string(&feedback)?,payload])?;
        self.save_task(task)?;
        if starts_turn {
            self.record_outgoing(&task.id, message)?;
        }
        tx.commit()?;
        Ok(())
    }
    /// Returns queued, in-flight and recoverable failed input in FIFO acceptance order.
    pub fn outgoing(&self, task_id: &str) -> Result<Vec<OutgoingMessage>> {
        let mut query = self.db.prepare("SELECT payload,state FROM submissions WHERE task_id=?1 AND payload IS NOT NULL AND state IN ('queued','pending','failed','unknown') ORDER BY rowid")?;
        let mut rows = query.query([task_id])?;
        let mut result = Vec::new();
        while let Some(row) = rows.next()? {
            let mut message: OutgoingMessage = serde_json::from_str(&row.get::<_, String>(0)?)?;
            message.state = match row.get::<_, String>(1)?.as_str() {
                "queued" => DeliveryState::Queued,
                "pending" => DeliveryState::Pending,
                "failed" => DeliveryState::Failed,
                _ => DeliveryState::Unknown,
            };
            result.push(message);
        }
        Ok(result)
    }
    /// Binds an undispatched message to the active turn without changing its saved content.
    pub fn steer_outgoing(&self, task_id: &str, id: &str) -> Result<()> {
        let task = self.task(task_id)?;
        ensure!(task.state.active(), "There is no active turn to steer");
        ensure!(
            !self.has_uncertain_submission(task_id)?,
            "Reconcile uncertain delivery before steering"
        );
        let turn = task
            .active_turn
            .context("The turn is still starting. Wait until it begins before steering.")?;
        let mut message = self
            .outgoing(task_id)?
            .into_iter()
            .find(|message| message.id == id)
            .context("Outgoing message not found")?;
        ensure!(
            message.state == DeliveryState::Queued,
            "Message is no longer queued"
        );
        ensure!(
            message
                .expected_turn
                .as_ref()
                .is_none_or(|expected| expected == &turn),
            "This message belongs to a different turn"
        );
        message.mode = SendMode::Steer;
        message.expected_turn = Some(turn);
        ensure!(
            self.db.execute(
                "UPDATE submissions SET payload=?3 WHERE id=?1 AND task_id=?2 AND state='queued'",
                params![id, task_id, serde_json::to_string(&message)?],
            )? == 1,
            "Message is no longer queued"
        );
        Ok(())
    }
    /// Stores one lifecycle step of a task: its ledger changes, its transcript and the task.
    ///
    /// Nothing is stored when any part fails. A claimed message is recorded in the transcript
    /// before its external delivery.
    pub fn commit(&self, commit: &Commit) -> Result<()> {
        let task = commit.task;
        let tx = self.db.unchecked_transaction()?;
        for message in commit.messages {
            self.message(message)?;
        }
        for change in commit.ledger {
            match change {
                LedgerChange::Claim(message) => {
                    ensure!(self.db.execute("UPDATE submissions SET state='pending' WHERE id=?1 AND task_id=?2 AND state='queued'", params![message.id,task.id])? == 1, "Message is no longer queued");
                    self.record_outgoing(&task.id, message)?;
                }
                LedgerChange::Confirm { id, turn } => self.confirm_submission(id, turn)?,
                LedgerChange::Fail { id, unknown, error } => {
                    self.fail_outgoing(&task.id, id, *unknown, error)?;
                }
            }
        }
        if commit.release_completion {
            self.release_completion_messages(&task.id)?;
        }
        // After the ledger, so that the first row of a new run is its recorded user message.
        for run in commit.runs {
            self.save_run(run)?;
        }
        self.save_task(task)?;
        tx.commit()?;
        Ok(())
    }
    /// Returns the newest run of a task, open or ended.
    pub fn latest_run(&self, task_id: &str) -> Result<Option<Run>> {
        let json: Option<String> = self
            .db
            .query_row(
                "SELECT json FROM runs WHERE task_id=?1 ORDER BY ordinal DESC LIMIT 1",
                [task_id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(json.map(|json| serde_json::from_str(&json)).transpose()?)
    }
    /// Returns the runs of a task, oldest first.
    pub fn runs(&self, task_id: &str) -> Result<Vec<Run>> {
        let mut statement = self
            .db
            .prepare("SELECT json FROM runs WHERE task_id=?1 ORDER BY ordinal")?;
        let rows = statement.query_map([task_id], |row| row.get::<_, String>(0))?;
        rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect()
    }
    /// Stores a run. A new run gets the `seq` of its first transcript row here: the row of the
    /// submission that started it, or the next row that the store will write.
    pub fn save_run(&self, run: &Run) -> Result<()> {
        let mut run = run.clone();
        if run.first_seq == 0 {
            let submitted: Option<i64> = match run.submissions.first() {
                Some(submission) => self
                    .db
                    .query_row(
                        "SELECT seq FROM messages WHERE task_id=?1 AND id=?2",
                        params![run.task_id, format!("submission-{submission}")],
                        |row| row.get(0),
                    )
                    .optional()?,
                None => None,
            };
            let first = match submitted {
                Some(seq) => seq,
                None => self.db.query_row(
                    "SELECT COALESCE((SELECT seq FROM sqlite_sequence WHERE name='messages'),0)+1",
                    [],
                    |row| row.get(0),
                )?,
            };
            run.first_seq = u64::try_from(first)?;
        }
        self.db.execute(
            "INSERT INTO runs VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET json=excluded.json",
            params![
                run.id,
                run.task_id,
                run.ordinal,
                serde_json::to_string(&run)?
            ],
        )?;
        Ok(())
    }
    fn record_outgoing(&self, task_id: &str, message: &OutgoingMessage) -> Result<()> {
        self.message(&Message {
            seq: 0,
            transfer: None,
            id: format!("submission-{}", message.id),
            task_id: task_id.into(),
            kind: MessageKind::User,
            text: message.text.clone(),
            attachments: message.attachments.clone(),
            feedback: message.feedback.clone(),
            file_edits: Vec::new(),
            tool: None,
            complete: true,
            created: message.created,
        })
    }
    /// Keeps a failed or uncertain input recoverable without altering the active provider turn.
    pub fn fail_outgoing(&self, task_id: &str, id: &str, unknown: bool, error: &str) -> Result<()> {
        let mut message = self
            .outgoing(task_id)?
            .into_iter()
            .find(|m| m.id == id)
            .context("Outgoing message not found")?;
        message.error = Some(error.chars().take(16 * 1024).collect());
        message.state = if unknown {
            DeliveryState::Unknown
        } else {
            DeliveryState::Failed
        };
        self.db.execute(
            "UPDATE submissions SET state=?2,payload=?3 WHERE id=?1",
            params![
                id,
                if unknown { "unknown" } else { "failed" },
                serde_json::to_string(&message)?
            ],
        )?;
        Ok(())
    }
    /// Cancels only an undispatched message or dismisses a definitively rejected one.
    pub fn cancel_outgoing(&self, task_id: &str, id: &str) -> Result<()> {
        ensure!(self.db.execute("UPDATE submissions SET state='cancelled' WHERE id=?1 AND task_id=?2 AND state IN ('queued','failed')",params![id,task_id])? == 1, "Only queued or rejected messages can be dismissed");
        Ok(())
    }
    /// Restores retained input to an empty composer and cancels its undispatched delivery atomically.
    pub fn restore_outgoing(&self, task_id: &str, id: &str) -> Result<()> {
        let mut task = self.task(task_id)?;
        ensure!(
            task.draft.trim().is_empty() && task.attachments.is_empty() && task.feedback.is_empty(),
            "Clear the composer before restoring a message"
        );
        let message = self
            .outgoing(task_id)?
            .into_iter()
            .find(|m| m.id == id)
            .context("Outgoing message not found")?;
        ensure!(
            matches!(message.state, DeliveryState::Queued | DeliveryState::Failed),
            "Only queued or rejected messages can be restored"
        );
        task.draft = message.text;
        task.attachments = message.attachments;
        task.feedback = message.feedback;
        let tx = self.db.unchecked_transaction()?;
        self.cancel_outgoing(task_id, id)?;
        self.save_task(&task)?;
        tx.commit()?;
        Ok(())
    }
    /// Returns the data directory that holds the database and the snapshot files.
    pub fn root(&self) -> &Path {
        &self.root
    }
    /// Stores captured content once, addressed by its SHA-256 identity.
    pub fn save_snapshot(&self, snapshot: &mut DiffSnapshot) -> Result<()> {
        Self::write_snapshot(&self.root, snapshot)
    }
    /// Stores captured content in a data directory without the database connection.
    ///
    /// A snapshot is an immutable file named by its content, so a caller can write it before
    /// taking the lock that guards the store.
    pub fn write_snapshot(root: &Path, snapshot: &mut DiffSnapshot) -> Result<()> {
        snapshot.id.clear();
        let bytes = serde_json::to_vec(snapshot)?;
        snapshot.id = format!("{:x}", Sha256::digest(&bytes));
        let path = root.join("snapshots").join(format!("{}.json", snapshot.id));
        if !path.exists() {
            let mut f = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?;
            use std::io::Write;
            f.write_all(&serde_json::to_vec(snapshot)?)?;
            f.sync_all()?;
        }
        Ok(())
    }
    /// Reuses the previous immutable snapshot when a refresh observes identical repository content.
    pub fn save_refreshed_snapshot(
        &self,
        snapshot: &mut DiffSnapshot,
        previous: Option<&str>,
    ) -> Result<()> {
        if let Some(previous) = previous.and_then(|id| self.snapshot(id).ok())
            && previous.same_content(snapshot)
        {
            *snapshot = previous;
            return Ok(());
        }
        self.save_snapshot(snapshot)
    }
    /// Loads the immutable content of a historical review snapshot.
    pub fn snapshot(&self, id: &str) -> Result<DiffSnapshot> {
        ensure!(
            id.len() == 64 && id.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid snapshot identity"
        );
        Ok(serde_json::from_slice(&fs::read(
            self.root.join("snapshots").join(format!("{id}.json")),
        )?)?)
    }
    fn read_json<T: serde::de::DeserializeOwned>(&self, sql: &str) -> Result<Vec<T>> {
        let mut stmt = self.db.prepare(sql)?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
    }
}

// Gives each project saved before avatars existed a random one, so its skater stays the same
// from then on.
fn assign_project_avatars(db: &Connection) -> Result<()> {
    let mut missing = Vec::new();
    {
        let mut statement = db.prepare("SELECT json FROM projects")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        for row in rows {
            let project: Project = serde_json::from_str(&row?)?;
            if project.avatar.is_none() {
                missing.push(project);
            }
        }
    }
    for mut project in missing {
        project.avatar = Some(SkaterAvatar::random());
        db.execute(
            "UPDATE projects SET json=?1 WHERE id=?2",
            params![serde_json::to_string(&project)?, project.id],
        )?;
    }
    Ok(())
}

// Providers that earlier versions stored and this version no longer has.
const REMOVED_PROVIDERS: [&str; 1] = ["Devin"];

// Moves tasks of removed providers aside and drops saved choices that name them, so the
// remaining records still parse. The moved tasks and their messages stay in the database.
fn retire_removed_providers(db: &Connection) -> Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS retired_tasks(id TEXT PRIMARY KEY, json TEXT NOT NULL);",
    )?;
    for provider in REMOVED_PROVIDERS {
        let pattern = format!("%\"provider\":\"{provider}\"%");
        let mut retired = Vec::new();
        {
            let mut statement = db.prepare("SELECT id, json FROM tasks WHERE json LIKE ?1")?;
            let rows = statement.query_map([&pattern], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            for row in rows {
                let (id, json) = row?;
                let task: serde_json::Value = serde_json::from_str(&json)?;
                if task["provider"] == provider {
                    retired.push((id, json));
                }
            }
        }
        for (id, json) in retired {
            db.execute(
                "INSERT OR REPLACE INTO retired_tasks VALUES(?1,?2)",
                params![id, json],
            )?;
            db.execute("DELETE FROM tasks WHERE id=?1", [&id])?;
        }
        let mut projects = Vec::new();
        {
            let mut statement = db.prepare("SELECT id, json FROM projects WHERE json LIKE ?1")?;
            let rows = statement.query_map([&pattern], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            for row in rows {
                projects.push(row?);
            }
        }
        for (id, json) in projects {
            let mut project: serde_json::Value = serde_json::from_str(&json)?;
            if project["defaults"]["provider"] == provider {
                project["defaults"] = serde_json::Value::Null;
                db.execute(
                    "UPDATE projects SET json=?1 WHERE id=?2",
                    params![serde_json::to_string(&project)?, id],
                )?;
            }
        }
        let saved: Option<String> = db
            .query_row("SELECT json FROM settings WHERE id=1", [], |row| row.get(0))
            .optional()?;
        let Some(saved) = saved else {
            continue;
        };
        if !saved.contains(provider) {
            continue;
        }
        let mut settings: serde_json::Value = serde_json::from_str(&saved)?;
        // `get_mut` leaves out a key that the saved settings lack. Indexing for a mutable
        // value would add the key as `null`, which a list or a map does not parse from.
        if let Some(preferences) = settings
            .get_mut("provider_preferences")
            .and_then(serde_json::Value::as_object_mut)
        {
            preferences.remove(provider);
        }
        // Every provider has a registered default account, so the removed one has an entry
        // in settings that hold accounts.
        if let Some(accounts) = settings
            .get_mut("accounts")
            .and_then(serde_json::Value::as_array_mut)
        {
            accounts.retain(|account| account["provider"] != provider);
        }
        if settings["agent_defaults"]["provider"] == provider
            && let Some(fields) = settings.as_object_mut()
        {
            fields.remove("agent_defaults");
        }
        db.execute(
            "UPDATE settings SET json=?1 WHERE id=1",
            [serde_json::to_string(&settings)?],
        )?;
    }
    Ok(())
}

// Fields of the saved settings that hold a list or a map.
const SETTINGS_COLLECTIONS: [&str; 2] = ["accounts", "provider_preferences"];

// Removes a `null` from a list or map field of the saved settings, so that the field takes its
// default again. An earlier form of `retire_removed_providers` wrote such a `null` into
// settings that lacked the field, and settings with it do not parse.
fn repair_settings_collections(db: &Connection) -> Result<()> {
    let saved: Option<String> = db
        .query_row("SELECT json FROM settings WHERE id=1", [], |row| row.get(0))
        .optional()?;
    let Some(saved) = saved else {
        return Ok(());
    };
    let mut settings: serde_json::Value = serde_json::from_str(&saved)?;
    let Some(fields) = settings.as_object_mut() else {
        return Ok(());
    };
    let count = fields.len();
    for name in SETTINGS_COLLECTIONS {
        if fields.get(name).is_some_and(serde_json::Value::is_null) {
            fields.remove(name);
        }
    }
    if fields.len() != count {
        db.execute(
            "UPDATE settings SET json=?1 WHERE id=1",
            [serde_json::to_string(&settings)?],
        )?;
    }
    Ok(())
}
