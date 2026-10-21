//! The Antigravity CLI (`agy`): where it is installed, its release, and the conversations it
//! stores on this computer.
//!
//! The CLI keeps its data under `~/.gemini/antigravity-cli`: one folder per conversation under
//! `brain`, whose `transcript_full.jsonl` holds every step, and an index of all conversations in
//! `conversation_summaries.db`. Both formats are internal to the CLI, so a reader skips what it
//! cannot read.
use crate::{model::*, platform};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OpenFlags};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

/// Most conversations that one listing of the index returns.
const LISTING_LIMIT: usize = 5000;

/// Finds the CLI: `KODEM_ANTIGRAVITY_PATH`, then the folder of its own installer, then the
/// folders where package managers put programs, then PATH.
pub fn executable() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("KODEM_ANTIGRAVITY_PATH") {
        return Ok(PathBuf::from(path));
    }
    let mut directories = Vec::new();
    // The Windows installer puts the CLI here; the other installers use `~/.local/bin`.
    #[cfg(windows)]
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        directories.push(PathBuf::from(local).join("agy").join("bin"));
    }
    directories.extend(platform::tool_directories());
    platform::find_program("agy", &directories)
        .context("The Antigravity CLI is not installed. Install it from Settings → Providers.")
}

/// Runs the CLI to read its release, such as `1.4.3`, without starting a session.
pub fn version() -> Result<String> {
    let mut command = Command::new(executable()?);
    platform::hide_console(&mut command);
    let output = command
        .arg("--version")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .context("The Antigravity CLI did not start")?;
    ensure!(
        output.status.success(),
        "The Antigravity CLI did not report a version"
    );
    let version = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    ensure!(
        !version.is_empty(),
        "The Antigravity CLI did not report a version"
    );
    Ok(version)
}

/// Returns the folder where the CLI keeps its settings, conversations and logs.
pub fn data_dir() -> Result<PathBuf> {
    Ok(platform::home_dir()?
        .join(".gemini")
        .join("antigravity-cli"))
}

/// Reports a conversation identity that the CLI could have made: a UUID, so it never names a
/// path outside the data folder.
pub fn valid_conversation(conversation: &str) -> bool {
    conversation.len() == 36
        && conversation
            .chars()
            .all(|character| character.is_ascii_hexdigit() || character == '-')
}

/// Returns the file that holds every step of a conversation, oldest first.
pub fn transcript(conversation: &str) -> Result<PathBuf> {
    ensure!(
        valid_conversation(conversation),
        "\"{conversation}\" is not an Antigravity conversation"
    );
    Ok(data_dir()?
        .join("brain")
        .join(conversation)
        .join(".system_generated")
        .join("logs")
        .join("transcript_full.jsonl"))
}

/// Returns the local path of a `file://` URI, as the CLI writes the folders of a conversation.
pub fn file_path(uri: &str) -> Option<PathBuf> {
    let encoded = uri.strip_prefix("file://")?;
    let mut bytes = Vec::with_capacity(encoded.len());
    let mut rest = encoded.as_bytes();
    while let Some((&byte, tail)) = rest.split_first() {
        if byte == b'%'
            && let Some(hex) = tail.get(..2)
            && let Ok(value) = u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16)
        {
            bytes.push(value);
            rest = &tail[2..];
            continue;
        }
        bytes.push(byte);
        rest = tail;
    }
    let path = String::from_utf8(bytes).ok()?;
    // A Windows URI names its drive after the first slash: `file:///C:/Users`.
    let path = if cfg!(windows) {
        path.strip_prefix('/').unwrap_or(&path).replace('/', "\\")
    } else {
        path
    };
    Some(PathBuf::from(path))
}

/// One conversation of the CLI's index.
#[derive(Clone, Debug)]
pub struct Summary {
    pub conversation: String,
    pub title: String,
    /// The folders of the conversation, in the CLI's order.
    pub folders: Vec<PathBuf>,
    /// Wall-clock milliseconds of the last change.
    pub updated: u64,
    /// The conversation that started this one as a subagent; absent for a top-level one.
    pub parent: Option<String>,
    /// The CLI's run state, such as `CASCADE_RUN_STATUS_IDLE`.
    pub status: String,
}
impl Summary {
    /// Reports a conversation that the CLI still works on.
    pub fn running(&self) -> bool {
        self.status.contains("RUNNING")
    }
}

/// Lists the top-level conversations of the CLI's index, the most recently changed first.
///
/// The CLI may write the index meanwhile, so it is opened read-only and waits briefly for a
/// lock. A missing index means that the CLI has stored no conversation yet.
pub fn summaries() -> Result<Vec<Summary>> {
    read_summaries(&data_dir()?.join("conversation_summaries.db"), None)
}

/// Reads the index entry of one conversation.
pub fn summary(conversation: &str) -> Result<Option<Summary>> {
    Ok(read_summaries(
        &data_dir()?.join("conversation_summaries.db"),
        Some(conversation),
    )?
    .into_iter()
    .next())
}

fn read_summaries(index: &Path, conversation: Option<&str>) -> Result<Vec<Summary>> {
    if !index.is_file() {
        return Ok(Vec::new());
    }
    let connection = Connection::open_with_flags(
        index,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .context("Could not open the Antigravity conversation index")?;
    connection.busy_timeout(Duration::from_secs(2))?;
    let mut statement = connection.prepare(
        "SELECT conversation_id, title, workspace_uris, CAST(last_modified_time AS TEXT),
                parent_conversation_id, status
         FROM conversation_summaries
         WHERE (?1 IS NULL AND parent_conversation_id = '') OR conversation_id = ?1
         ORDER BY last_modified_time DESC LIMIT ?2",
    )?;
    let rows = statement.query_map(
        rusqlite::params![conversation, LISTING_LIMIT as i64],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        },
    )?;
    let mut summaries = Vec::new();
    for row in rows {
        let Ok((conversation, title, folders, updated, parent, status)) = row else {
            continue;
        };
        if !valid_conversation(&conversation) {
            continue;
        }
        let folders = serde_json::from_str::<Vec<String>>(&folders)
            .unwrap_or_default()
            .iter()
            .filter_map(|uri| file_path(uri))
            .collect();
        summaries.push(Summary {
            conversation,
            title,
            folders,
            updated: updated.as_deref().and_then(unix_millis).unwrap_or(0),
            parent: (!parent.is_empty()).then_some(parent),
            status,
        });
    }
    Ok(summaries)
}
