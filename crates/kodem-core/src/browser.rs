//! The protocol between the tools of a provider agent and the browser of the Kodem window.
//!
//! The browser lives in the desktop app, not in the host, so the window serves its own private
//! socket beside the host's. The MCP server of a task sends one request per tool call and reads
//! one reply. The socket accepts only processes of the same user, as the host's does.
use crate::ipc::{self, UnixStream};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

/// The longest wait of a command that waits for the page, in seconds.
pub const WAIT_LIMIT: u64 = 30;

/// Returns the private endpoint of the window's browser.
pub fn socket(root: &Path) -> PathBuf {
    root.join("browser-v1.sock")
}

/// One tool call of an agent for the browser of its task.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BrowserRequest {
    pub task_id: String,
    pub command: BrowserCommand,
}

/// What an agent asks the browser of its task to do.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum BrowserCommand {
    /// Loads a web address or a file. A relative path starts at the task's folder.
    Open { target: String },
    /// Goes back or forward in the history, or loads the page again.
    Navigate { action: BrowserHistory },
    /// Captures the visible part of the page as a PNG image.
    Screenshot,
    /// Runs an action of the page script, such as `snapshot` or `click`, with the arguments of
    /// the tool call.
    Page {
        action: String,
        arguments: serde_json::Value,
    },
}

/// A step in the browser's history.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserHistory {
    Back,
    Forward,
    Reload,
}

/// The answer to one browser request.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum BrowserReply {
    Text(String),
    /// A PNG image in base64, with a line that describes it.
    Image {
        png: String,
        text: String,
    },
    Error(String),
}

impl BrowserCommand {
    /// Returns how long the window may take to answer this command.
    pub fn wait(&self) -> Duration {
        let seconds = match self {
            Self::Open { .. } | Self::Navigate { .. } => 20,
            Self::Page { action, arguments } if action == "wait_for" => {
                arguments["timeout_seconds"]
                    .as_u64()
                    .unwrap_or(10)
                    .min(WAIT_LIMIT)
                    + 5
            }
            Self::Page { action, .. } if action == "evaluate" => WAIT_LIMIT,
            Self::Screenshot | Self::Page { .. } => 15,
        };
        Duration::from_secs(seconds)
    }
}

/// Sends one command to the browser of the Kodem window and waits for its reply.
pub fn request(root: &Path, request: &BrowserRequest) -> Result<BrowserReply> {
    let mut stream = UnixStream::connect(socket(root))
        .context("The Kodem window is not open, so its browser cannot be used")?;
    stream.set_read_timeout(Some(request.command.wait() + Duration::from_secs(10)))?;
    stream.set_write_timeout(Some(Duration::from_secs(15)))?;
    ipc::write_frame(&mut stream, request)?;
    ipc::read_frame(&mut stream)
}
