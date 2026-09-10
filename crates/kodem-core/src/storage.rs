//! One host-owned SQLite connection with immutable snapshot blobs.
use crate::model::*;
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

pub struct Store {
    db: Connection,
    root: PathBuf,
}
impl Store {
    /// Opens the local store, migrates its schema, and restricts access to the user.
    pub fn open(root: &Path) -> Result<Self> {
        fs::create_dir_all(root.join("snapshots"))?;
        fs::set_permissions(root, fs::Permissions::from_mode(0o700))?;
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
            fs::set_permissions(backup, fs::Permissions::from_mode(0o600))?;
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
        db.execute_batch("COMMIT;")?;
        fs::set_permissions(
            root.join("workspace.sqlite"),
            fs::Permissions::from_mode(0o600),
        )?;
        Ok(Self {
            db,
            root: root.to_owned(),
        })
    }
    /// Marks interrupted host-owned tasks without replaying external submissions.
    pub fn recover(&self) -> Result<()> {
        for mut task in self.tasks()? {
            let pending = self.has_uncertain_submission(&task.id)?;
            if task.state.active() || pending {
                task.state = RunState::DeliveryUnknown;
                task.error=Some("The host stopped before completion was confirmed. History is retained. No prompt was resent. Review the provider history before starting another turn.".into());
                task.active_turn = None;
                self.save_task(&task)?;
            }
        }
        self.db.execute(
            "UPDATE submissions SET state='unknown' WHERE state='pending'",
            [],
        )?;
        Ok(())
    }
    /// Returns registered repositories, saving a color for any project that predates colors.
    pub fn projects(&self) -> Result<Vec<Project>> {
        let mut projects: Vec<Project> =
            self.read_json("SELECT json FROM projects ORDER BY path")?;
        for project in &mut projects {
            if project.color.is_none() {
                project.color = Some(project_color());
                self.db.execute(
                    "UPDATE projects SET json=?1 WHERE id=?2",
                    params![serde_json::to_string(project)?, project.id],
                )?;
            }
        }
        Ok(projects)
    }
    /// Adds a project once, using its canonical path as the identity boundary.
    pub fn add_project(&self, path: PathBuf) -> Result<Project> {
        if let Some(p) = self.projects()?.into_iter().find(|p| p.path == path) {
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
            color: Some(project_color()),
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
        transaction.execute("DELETE FROM submissions WHERE task_id=?1", [id])?;
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
        Ok(s.map(|s| serde_json::from_str(&s))
            .transpose()?
            .unwrap_or_default())
    }
    /// Persists bounded pane sizes and typography choices.
    pub fn save_settings(&self, s: &Settings) -> Result<()> {
        let mut settings = s.clone();
        // Agent defaults are host-owned; an older layout snapshot must not overwrite them.
        settings.agent_defaults = self.settings()?.agent_defaults;
        self.write_settings(&settings)
    }
    /// Atomically saves an accepted task preference change and the defaults for new tasks.
    pub fn save_task_preferences(&self, task: &Task, defaults: AgentPreferences) -> Result<()> {
        let mut settings = self.settings()?;
        settings.agent_defaults = defaults;
        let transaction = self.db.unchecked_transaction()?;
        self.save_task(task)?;
        self.write_settings(&settings)?;
        transaction.commit()?;
        Ok(())
    }
    fn write_settings(&self, s: &Settings) -> Result<()> {
        ensure!(
            s.sidebar_width.is_finite() && s.review_width.is_finite() && s.font_size.is_finite(),
            "Invalid layout value"
        );
        let mut s = s.clone();
        s.sidebar_width = s.sidebar_width.clamp(220., 420.);
        s.review_width = s.review_width.clamp(320., 900.);
        s.font_size = s.font_size.clamp(12., 22.);
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
        ensure!(
            serde_json::to_vec(m)?.len() <= 4 * 1024 * 1024,
            "Message and attached context exceed 4 MiB"
        );
        self.db.execute("INSERT INTO messages(id,task_id,json) VALUES(?1,?2,?3) ON CONFLICT(task_id,id) DO UPDATE SET json=excluded.json",params![m.id,m.task_id,serde_json::to_string(m)?])?;
        Ok(())
    }
    /// Reads a bounded transcript page in chronological order.
    pub fn messages(&self, task: &str, before: Option<u64>) -> Result<(Vec<Message>, Option<u64>)> {
        let mut stmt = self.db.prepare(
            "SELECT seq,json FROM messages WHERE task_id=?1 AND seq<?2 ORDER BY seq DESC LIMIT 101",
        )?;
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
        let feedback: Vec<_> = message.feedback.iter().map(|f| &f.id).collect();
        self.db.execute("INSERT INTO submissions(id,task_id,prompt,state,feedback,payload) VALUES(?1,?2,?3,'queued',?4,?5)", params![message.id,task.id,prompt,serde_json::to_string(&feedback)?,payload])?;
        self.save_task(task)?;
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
    /// Claims one accepted message and records its transcript before external delivery.
    pub fn claim_outgoing(&self, task: &Task, message: &OutgoingMessage) -> Result<()> {
        let tx = self.db.unchecked_transaction()?;
        ensure!(self.db.execute("UPDATE submissions SET state='pending' WHERE id=?1 AND task_id=?2 AND state='queued'", params![message.id,task.id])? == 1, "Message is no longer queued");
        self.save_task(task)?;
        self.message(&Message {
            seq: 0,
            id: format!("submission-{}", message.id),
            task_id: task.id.clone(),
            kind: MessageKind::User,
            text: message.text.clone(),
            attachments: message.attachments.clone(),
            feedback: message.feedback.clone(),
            complete: true,
            created: now(),
        })?;
        tx.commit()?;
        Ok(())
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
    /// Stores captured content once, addressed by its SHA-256 identity.
    pub fn save_snapshot(&self, snapshot: &mut DiffSnapshot) -> Result<()> {
        snapshot.id.clear();
        let bytes = serde_json::to_vec(snapshot)?;
        snapshot.id = format!("{:x}", Sha256::digest(&bytes));
        let path = self
            .root
            .join("snapshots")
            .join(format!("{}.json", snapshot.id));
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

// Keep randomly assigned colors readable against both sidebar themes.
fn project_color() -> u32 {
    let bytes = *uuid::Uuid::new_v4().as_bytes();
    let palette = [
        0xd99a73, 0x9ca9dc, 0x89bca7, 0xcba0cd, 0xc8b875, 0x7fb8ca, 0xd88f9e, 0xa7bc80,
    ];
    palette[bytes[0] as usize % palette.len()]
}
