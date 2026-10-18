//! The browser of the window's side pane. Each task has its own page, which the user and the
//! agents of that task share: the user sees what an agent opens, and an agent reads and acts on
//! what the user sees.
//!
//! The page is the platform's web view placed over the pane: WebKit on macOS, WebView2 on
//! Windows and WebKitGTK on Linux, where it needs an X11 display (XWayland on Wayland).
mod engine;
#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(windows, path = "windows.rs")]
#[cfg_attr(not(any(target_os = "macos", windows)), path = "linux.rs")]
mod native;
mod server;

pub use engine::Browsers;
#[cfg(not(any(target_os = "macos", windows)))]
pub use native::{prefer_x11, pump};
pub use server::serve;

use kodem_core::browser::{BrowserCommand, BrowserReply};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

/// The page that a task's browser opens first.
pub const HOME: &str = "https://www.google.com/";
/// The longest time that a page may take to load before Kodem stops waiting for it.
pub const LOAD_LIMIT: Duration = Duration::from_secs(15);

/// A browser command of an agent, waiting for the window to run it.
pub struct Job {
    pub task_id: String,
    pub command: BrowserCommand,
    /// Answers with `Outcome::Loading` instead of running while the page loads.
    pub patient: bool,
    pub reply: mpsc::Sender<Outcome>,
}

/// What the window did with a job.
pub enum Outcome {
    /// The reply, and the page to wait for before sending it when the command may navigate.
    Done {
        reply: BrowserReply,
        settle: Option<Arc<Mutex<Status>>>,
    },
    /// The page loads; the job runs again once it has loaded.
    Loading(Arc<Mutex<Status>>),
}

impl Outcome {
    /// Returns a reply that needs no wait.
    pub fn reply(reply: BrowserReply) -> Self {
        Self::Done {
            reply,
            settle: None,
        }
    }

    /// Returns an error reply.
    pub fn error(text: impl Into<String>) -> Self {
        Self::reply(BrowserReply::Error(text.into()))
    }
}

/// The page of a task, as its load events report it.
#[derive(Clone, Debug, Default)]
pub struct Status {
    /// When the page started to load, while it loads.
    pub loading: Option<Instant>,
    pub url: String,
    pub title: String,
}

impl Status {
    /// Tells whether the page loads, and has not taken so long that it counts as failed.
    pub fn is_loading(&self) -> bool {
        self.loading
            .is_some_and(|since| since.elapsed() < LOAD_LIMIT)
    }
}

/// What the pane shows about the page of a task.
pub struct Page {
    pub url: String,
    pub loading: bool,
}

/// A change of a page that the window shows.
pub enum Event {
    /// The address, the title or the load state of a page changed.
    Changed,
    /// An agent opened a page for the task, so the pane shows it.
    Opened(String),
    /// The page asked for a new window; the task's page opens the address instead.
    NewWindow(String, String),
    /// The page of a task may have taken the keyboard: the user clicked it, or it took the
    /// keyboard in another way.
    Focused(String),
    /// A task's page measured the width that its content needs and the width of its view, in
    /// CSS pixels. `fresh` marks the first measure of a document.
    Fit {
        task_id: String,
        width: f64,
        viewport: f64,
        fresh: bool,
    },
}

/// What an address or a path opens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Web(String),
    /// A local file, and the folder whose files the page may read.
    File {
        path: PathBuf,
        access: PathBuf,
    },
}

impl Target {
    /// Reads what the user typed or an agent asked for. A relative path starts at `folder`.
    pub fn parse(text: &str, folder: Option<&Path>) -> Result<Self, String> {
        let text = text.trim();
        if text.is_empty() {
            return Err("Type an address or a file path.".into());
        }
        let lower = text.to_ascii_lowercase();
        if let Some(path) = lower
            .starts_with("file://")
            .then(|| decode(&text["file://".len()..]))
        {
            // A Windows address names the drive after a slash, as in file:///C:/index.html.
            #[cfg(windows)]
            let path = match path.strip_prefix('/') {
                Some(rest) if rest.as_bytes().get(1) == Some(&b':') => rest.to_owned(),
                _ => path,
            };
            return Self::file(PathBuf::from(path), folder);
        }
        if ["http://", "https://", "about:", "data:"]
            .iter()
            .any(|scheme| lower.starts_with(scheme))
        {
            return Ok(Self::Web(text.into()));
        }
        let local = ["localhost", "127.0.0.1", "0.0.0.0", "[::1]"]
            .iter()
            .any(|host| {
                lower
                    .strip_prefix(host)
                    .is_some_and(|rest| rest.is_empty() || rest.starts_with([':', '/']))
            });
        if local {
            return Ok(Self::Web(format!("http://{text}")));
        }
        let path = match text.strip_prefix("~/") {
            Some(rest) => std::env::home_dir().map(|home| home.join(rest)),
            None => Some(PathBuf::from(text)),
        };
        if let Some(path) = path {
            let resolved = match folder {
                Some(folder) if path.is_relative() => folder.join(&path),
                _ => path.clone(),
            };
            if path.is_absolute() || text.starts_with(['.', '~']) || resolved.exists() {
                return Self::file(resolved, folder);
            }
        }
        if !text.contains(char::is_whitespace) && text.contains(['.', ':']) {
            return Ok(Self::Web(format!("https://{text}")));
        }
        Err(format!(
            "\"{text}\" is not a web address or a file that exists."
        ))
    }

    // A file to open, or the index page of a folder. The page may read the files of the task's
    // folder when the file is in it, and else those beside the file.
    fn file(path: PathBuf, folder: Option<&Path>) -> Result<Self, String> {
        let path = std::fs::canonicalize(&path)
            .map_err(|_| format!("There is no file at {}.", path.display()))?;
        let path = if path.is_dir() {
            let index = path.join("index.html");
            if !index.is_file() {
                return Err(format!("The folder {} has no index.html.", path.display()));
            }
            index
        } else {
            path
        };
        let access = folder
            .and_then(|folder| std::fs::canonicalize(folder).ok())
            .filter(|folder| path.starts_with(folder))
            .or_else(|| path.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| path.clone());
        Ok(Self::File { path, access })
    }

    /// Returns the address that the page shows for this target.
    pub fn url(&self) -> String {
        match self {
            Self::Web(url) => url.clone(),
            Self::File { path, .. } => format!("file://{}", path.display()),
        }
    }
}

/// Returns the address of a local file, with the characters that an address cannot hold
/// escaped.
// macOS's WebKit opens a file by its path.
#[cfg(not(target_os = "macos"))]
fn file_url(path: &Path) -> String {
    use std::fmt::Write as _;
    let text = path.to_string_lossy();
    // A canonical Windows path starts with `\\?\`, or `\\?\UNC\` for a share.
    let text = match text.strip_prefix(r"\\?\UNC\") {
        Some(share) => format!(r"\\{share}"),
        None => text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned(),
    }
    .replace('\\', "/");
    let mut url = String::from(if text.starts_with("//") {
        "file:"
    } else if text.starts_with('/') {
        "file://"
    } else {
        "file:///"
    });
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"/-_.~:".contains(&byte) {
            url.push(char::from(byte));
        } else {
            let _ = write!(url, "%{byte:02X}");
        }
    }
    url
}

// Decodes the percent escapes of a file address.
fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let escape = (bytes[index] == b'%')
            .then(|| text.get(index + 1..index + 3))
            .flatten()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match escape {
            Some(byte) => {
                decoded.push(byte);
                index += 3;
            }
            None => {
                decoded.push(bytes[index]);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

/// Waits up to `limit` until the page of `status` has loaded or counts as failed.
pub fn wait_for_load(status: &Mutex<Status>, limit: Duration) {
    let until = Instant::now() + limit;
    while status.lock().is_ok_and(|status| status.is_loading()) && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(50));
    }
}
