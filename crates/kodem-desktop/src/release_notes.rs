//! The changes of this release, embedded by the release commands from Git history, the
//! highlights that `changelog/highlights.toml` gives this release, and whether this installation
//! showed them since it updated. Desktop-owned, like the window placement.
use crate::updater::{Channel, VERSION};
use serde_json::{Value, json};
use std::{fs, path::Path};

/// Commit subjects since the previous release of this channel, newest first, one per line.
const NOTES: &str = include_str!(concat!(env!("OUT_DIR"), "/release-notes.txt"));

/// The release of this channel that the notes start after, or empty for the first release.
pub const PREVIOUS: &str = env!("KODEM_RELEASE_PREVIOUS");

/// Where the desktop records the last version whose notes it showed.
const FILE: &str = "release-notes.json";

/// A card of the What's new board, from `changelog/highlights.toml`.
pub struct Highlight {
    pub title: &'static str,
    pub text: &'static str,
    /// Takes two columns of the board.
    pub wide: bool,
    pub tone: Tone,
    pub visual: Visual,
}

/// A color of the theme that a highlight names.
#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Each release's highlights use some of the kinds.
pub enum Tone {
    Accent,
    Green,
    Red,
    Amber,
    Blue,
    Muted,
}

/// The picture of a highlight, drawn live on a stage above its title.
#[allow(dead_code)] // Each release's highlights use some of the kinds.
pub enum Visual {
    /// Rows with a state, as a tree under a title or as small plates.
    List {
        title: Option<&'static str>,
        badge: Option<&'static str>,
        items: &'static [Row],
    },
    /// Capsules with an icon, a label and a detail or a diffstat.
    Chips(&'static [Chip]),
    /// A composer with a draft and capsules above it.
    Prompt {
        text: &'static str,
        chips: &'static [Chip],
    },
    /// Dot-matrix bars, one of them lit.
    Bars {
        label: Option<&'static str>,
        bars: &'static [Bar],
        focus: Option<usize>,
    },
    /// A dot-matrix number with a caption.
    Figure {
        value: &'static str,
        label: &'static str,
        detail: Option<&'static str>,
    },
    /// One dot-matrix bar of parts, with their legend.
    Meter {
        segments: &'static [Segment],
        caption: Option<&'static str>,
    },
    /// Keycaps of a shortcut.
    Keys {
        keys: &'static [&'static str],
        label: Option<&'static str>,
    },
    /// One large icon.
    Glyph(&'static str),
    /// Large icon tiles with a name, for agents, providers and other named things.
    Tiles(&'static [Tile]),
    /// Settings rows with a switch, on or off.
    Toggles(&'static [Toggle]),
    /// Settings rows with a slider at a position.
    Sliders(&'static [Slider]),
    /// A capsule as it was, an arrow, and the capsule as it is now.
    Compare {
        before: Chip,
        after: Chip,
        label: Option<&'static str>,
    },
    /// A menu with one row under the pointer.
    Menu {
        items: &'static [MenuItem],
        focus: Option<usize>,
    },
    /// A sketch of the Kodem window with one area lit.
    Window {
        area: Area,
        label: Option<&'static str>,
    },
    /// A dot-matrix ring filled to a share, with a reading in its middle.
    Gauge {
        value: f32,
        text: Option<&'static str>,
        label: &'static str,
    },
    /// Steps along a line. The steps before `current` are done.
    Steps {
        items: &'static [&'static str],
        current: usize,
    },
    /// Lines of a change to a file.
    Diff {
        file: Option<&'static str>,
        lines: &'static [DiffLine],
    },
    /// A dialog or a notification with its buttons; the last button is the main one.
    Notice {
        icon: &'static str,
        title: &'static str,
        text: Option<&'static str>,
        actions: &'static [&'static str],
    },
    /// A question with its options, one of them chosen.
    Choice {
        question: &'static str,
        options: &'static [&'static str],
        chosen: Option<usize>,
    },
}

/// A row of a list visual.
pub struct Row {
    pub text: &'static str,
    pub state: RowState,
    pub tag: Option<&'static str>,
    pub tone: Tone,
}

/// The mark at the start of a row: done, open, a filled mark in the row's tone, or a faded row.
#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Each release's highlights use some of the kinds.
pub enum RowState {
    Done,
    Open,
    Dot,
    Faded,
}

/// A capsule of a chips, prompt or compare visual.
pub struct Chip {
    pub text: &'static str,
    pub icon: Option<&'static str>,
    pub detail: Option<&'static str>,
    pub added: Option<u32>,
    pub removed: Option<u32>,
    pub tone: Tone,
}

/// A bar of a bars visual.
pub struct Bar {
    pub value: f32,
    pub label: Option<&'static str>,
}

/// A part of a meter visual.
pub struct Segment {
    pub label: &'static str,
    pub weight: f32,
    pub tone: Tone,
}

/// A tile of a tiles visual.
pub struct Tile {
    pub text: &'static str,
    pub icon: &'static str,
    pub badge: Option<&'static str>,
    pub tone: Tone,
}

/// A row of a toggles visual.
pub struct Toggle {
    pub text: &'static str,
    pub on: bool,
    pub detail: Option<&'static str>,
}

/// A row of a sliders visual. `value` is the thumb's place from 0 to 1.
pub struct Slider {
    pub text: &'static str,
    pub value: f32,
    pub readout: Option<&'static str>,
}

/// A row of a menu visual.
pub struct MenuItem {
    pub text: &'static str,
    pub icon: Option<&'static str>,
    pub keys: Option<&'static str>,
}

/// The area of the window that a window visual lights.
#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Each release's highlights use some of the kinds.
pub enum Area {
    Titlebar,
    Sidebar,
    Chat,
    Composer,
    Terminal,
    Panel,
}

/// A line of a diff visual.
pub struct DiffLine {
    pub text: &'static str,
    pub change: Change,
}

/// Whether a diff line was added, removed or kept.
#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Each release's highlights use some of the kinds.
pub enum Change {
    Added,
    Removed,
    Same,
}

// `HEADLINE` and `HIGHLIGHTS`: the entries of `changelog/highlights.toml` that this release
// shows, in file order.
include!(concat!(env!("OUT_DIR"), "/highlights.rs"));

/// Tells whether this build has anything for the What's new dialog.
pub fn available() -> bool {
    HEADLINE.is_some() || !HIGHLIGHTS.is_empty() || !changes().is_empty()
}

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
    take_unseen_in(&root, available())
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
