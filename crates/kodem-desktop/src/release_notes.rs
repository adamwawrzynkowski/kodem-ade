//! The changes of this release, embedded by the release commands from Git history, and whether
//! this installation showed them since it updated. Desktop-owned, like the window placement.
use crate::updater::{Channel, VERSION};
use serde_json::{Value, json};
use std::{fs, path::Path};

/// Commit subjects since the previous release of this channel, newest first, one per line.
const NOTES: &str = include_str!(concat!(env!("OUT_DIR"), "/release-notes.txt"));

/// The release of this channel that the notes start after, or empty for the first release.
pub const PREVIOUS: &str = env!("KODEM_RELEASE_PREVIOUS");

/// Where the desktop records the last version whose notes it showed.
const FILE: &str = "release-notes.json";

/// Returns the changes of this release, one entry per commit.
pub fn changes() -> Vec<&'static str> {
    NOTES
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect()
}

/// Tells whether this version runs for the first time after an update and has notes to show,
/// and records that it did, so the notes show once. A new installation shows none.
pub fn take_unseen() -> bool {
    if Channel::current() == Channel::Debug {
        return false;
    }
    let Ok(root) = kodem_core::ipc::data_dir() else {
        return false;
    };
    take_unseen_in(&root, !changes().is_empty())
}

// Decides and records for one data directory whether notes of this version are due.
fn take_unseen_in(root: &Path, has_notes: bool) -> bool {
    let seen = fs::read(root.join(FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|value| value["seen"].as_str().map(str::to_owned));
    if seen.as_deref() == Some(VERSION) {
        return false;
    }
    // Versions before this file existed left their window placement behind.
    let updated = seen.is_some() || root.join("window-state.json").is_file();
    // Showing notes that cannot be recorded would show them at every launch.
    if let Err(error) = record(root) {
        eprintln!("Could not record the shown release notes: {error:#}");
        return false;
    }
    updated && has_notes
}

// Atomically saves this version as the last one whose notes were handled.
fn record(root: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(root)?;
    let temporary = root.join(format!("release-notes.{}.tmp", std::process::id()));
    fs::write(&temporary, serde_json::to_vec(&json!({ "seen": VERSION }))?)?;
    if let Err(error) = fs::rename(&temporary, root.join(FILE)) {
        let _ = fs::remove_file(temporary);
        return Err(error.into());
    }
    Ok(())
}
