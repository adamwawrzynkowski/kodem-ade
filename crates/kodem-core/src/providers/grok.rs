//! The Grok Build CLI (`grok`): where it is installed, its release, and the sessions it stores
//! on this computer.
//!
//! The CLI keeps its data in its home folder, `~/.grok` or the folder that `GROK_HOME` names.
//! Each session is a folder under `sessions/<encoded folder>/<session>`: `summary.json` describes
//! it and `updates.jsonl` holds every ACP update of the conversation, oldest first. Both formats
//! are internal to the CLI, so a reader skips what it cannot read.
use crate::{model::*, platform};
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

/// Most sessions that one listing returns.
const LISTING_LIMIT: usize = 5000;
/// Largest summary file that is read, in bytes.
const SUMMARY_LIMIT: u64 = 1024 * 1024;
/// Words of the standing instructions that Kodem gives every session it starts. The CLI keeps
/// them in the session's system prompt, which tells Kodem's sessions from the others.
pub const KODEM_MARKER: &str = "Kodem shows your todo list to the user";

/// Finds the CLI: `KODEM_GROK_PATH`, then the folder of its own installer, then the folders
/// where package managers put programs, then PATH.
pub fn executable() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("KODEM_GROK_PATH") {
        return Ok(PathBuf::from(path));
    }
    let mut directories = Vec::new();
    // The installers put the CLI here and link it from `~/.local/bin` on macOS and Linux.
    if let Ok(home) = platform::home_dir() {
        directories.push(home.join(".grok").join("bin"));
    }
    directories.extend(platform::tool_directories());
    platform::find_program("grok", &directories)
        .context("Grok Build is not installed. Install it from Settings → Providers.")
}

/// Runs the CLI to read its release, such as `1.0.62`, without starting a session.
pub fn version() -> Result<String> {
    let mut command = Command::new(executable()?);
    platform::hide_console(&mut command);
    let output = command
        .arg("--version")
        .env("GROK_DISABLE_AUTOUPDATER", "1")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .context("Grok Build did not start")?;
    ensure!(
        output.status.success(),
        "Grok Build did not report a version"
    );
    // The CLI prints `grok 1.0.62 (1474f9cbdd91) [stable]`.
    let text = String::from_utf8_lossy(&output.stdout);
    let version = text
        .split_whitespace()
        .find(|word| word.starts_with(|character: char| character.is_ascii_digit()))
        .unwrap_or_default()
        .to_owned();
    ensure!(!version.is_empty(), "Grok Build did not report a version");
    Ok(version)
}

/// Returns the CLI's home folder for an account: the folder of an added account, else the
/// CLI's own.
pub fn home(account: Option<&Path>) -> Result<PathBuf> {
    match account {
        Some(home) => Ok(home.to_owned()),
        None => Ok(platform::home_dir()?.join(".grok")),
    }
}

/// Reports a session identity that the CLI could have made: a UUID, so it never names a path
/// outside the sessions folder.
pub fn valid_session(session: &str) -> bool {
    session.len() == 36
        && session
            .chars()
            .all(|character| character.is_ascii_hexdigit() || character == '-')
}

/// What the CLI stored about one session.
#[derive(Clone, Debug)]
pub struct Summary {
    pub session: String,
    pub folder: PathBuf,
    pub title: String,
    pub cwd: PathBuf,
    /// Unix milliseconds.
    pub created: u64,
    pub updated: u64,
    pub model: Option<String>,
    /// The session belongs to an agent that another session started.
    pub subagent: bool,
}

impl Summary {
    /// Reports a session that Kodem started, by the instructions it gave the session.
    pub fn kodem(&self) -> bool {
        std::fs::read_to_string(self.folder.join("system_prompt.txt"))
            .is_ok_and(|prompt| prompt.contains(KODEM_MARKER))
    }

    /// Returns the file that holds every update of the session, oldest first.
    pub fn updates(&self) -> PathBuf {
        self.folder.join("updates.jsonl")
    }
}

// Reads the summary of one session folder.
fn read_summary(folder: &Path) -> Option<Summary> {
    let session = folder.file_name()?.to_str()?.to_owned();
    if !valid_session(&session) {
        return None;
    }
    let path = folder.join("summary.json");
    if std::fs::metadata(&path).ok()?.len() > SUMMARY_LIMIT {
        return None;
    }
    let summary: Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    let text = |name: &str| summary[name].as_str().map(str::trim).unwrap_or_default();
    let title = [text("generated_title"), text("session_summary")]
        .into_iter()
        .find(|title| !title.is_empty())
        .unwrap_or_default()
        .to_owned();
    let updated = summary["updated_at"].as_str().and_then(unix_millis)?;
    Some(Summary {
        session,
        folder: folder.to_owned(),
        title,
        cwd: PathBuf::from(summary["info"]["cwd"].as_str()?),
        created: summary["created_at"]
            .as_str()
            .and_then(unix_millis)
            .unwrap_or(updated),
        updated,
        model: summary["current_model_id"].as_str().map(str::to_owned),
        subagent: !summary["session_kind"].is_null(),
    })
}

/// Lists the sessions that the CLI stored in a home folder, newest first.
pub fn summaries(home: &Path) -> Result<Vec<Summary>> {
    let sessions = home.join("sessions");
    let Ok(groups) = std::fs::read_dir(&sessions) else {
        return Ok(Vec::new());
    };
    let mut summaries = Vec::new();
    for group in groups.flatten() {
        let Ok(folders) = std::fs::read_dir(group.path()) else {
            continue;
        };
        summaries.extend(
            folders
                .flatten()
                .filter_map(|folder| read_summary(&folder.path())),
        );
    }
    summaries.sort_by_key(|summary| std::cmp::Reverse(summary.updated));
    summaries.truncate(LISTING_LIMIT);
    Ok(summaries)
}

/// Reads one stored session, or `None` when the CLI no longer has it.
pub fn summary(home: &Path, session: &str) -> Result<Option<Summary>> {
    ensure!(
        valid_session(session),
        "\"{session}\" is not a Grok Build session"
    );
    let Ok(groups) = std::fs::read_dir(home.join("sessions")) else {
        return Ok(None);
    };
    Ok(groups
        .flatten()
        .map(|group| group.path().join(session))
        .find(|folder| folder.is_dir())
        .and_then(|folder| read_summary(&folder)))
}
