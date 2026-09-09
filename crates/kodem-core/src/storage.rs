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
            version <= 2,
            "This database was created by a newer Kodem version"
        );
        if version > 0 && version < 2 {
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
            let pending = self
                .latest_submission(&task.id)?
                .is_some_and(|s| s.state == "pending" || s.state == "unknown");
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
    /// Returns all registered repositories.
    pub fn projects(&self) -> Result<Vec<Project>> {
        self.read_json("SELECT json FROM projects ORDER BY path")
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
            let mut m: Message = serde_json::from_str(&row.get::<_, String>(1)?)?;
            m.seq = u64::try_from(row.get::<_, i64>(0)?)?;
            if !messages.is_empty() && bytes + m.text.len() > 4 * 1024 * 1024 {
                byte_limit = true;
                break;
            }
            bytes += m.text.len();
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
        let row=self.db.query_row("SELECT id,prompt,state,receipt,feedback FROM submissions WHERE task_id=?1 ORDER BY rowid DESC LIMIT 1",[task_id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,Option<String>>(3)?,r.get::<_,String>(4)?))).optional()?;
        row.map(|(id, prompt, state, receipt, feedback)| {
            Ok(SubmissionRecord {
                id,
                prompt,
                state,
                receipt,
                feedback: serde_json::from_str(&feedback)?,
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
