use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{PoisonError, RwLock};

use anyhow::{Context as _, Result, bail};

use super::repo::base_git_command;

/// The sign-in details of one linked account, used as a fallback
/// credential for HTTPS remotes on its host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountCredential {
    pub host: String,
    pub username: String,
    pub token: String,
}

static ACCOUNT_CREDENTIALS: RwLock<Vec<AccountCredential>> = RwLock::new(Vec::new());

/// Replaces the account credentials that git commands can use.
pub fn set_account_credentials(credentials: Vec<AccountCredential>) {
    *ACCOUNT_CREDENTIALS
        .write()
        .unwrap_or_else(PoisonError::into_inner) = credentials;
}

/// Adds one credential helper per linked account, scoped to the HTTPS
/// URLs of its host. The helpers come after the user's own helpers, so a
/// saved keychain login still wins and the account only fills the gap.
/// The token goes through the environment, never the argument list, so
/// other processes cannot read it from `ps`.
pub(crate) fn apply_account_credentials(command: &mut std::process::Command) {
    let credentials = ACCOUNT_CREDENTIALS
        .read()
        .unwrap_or_else(PoisonError::into_inner);
    for (ix, credential) in credentials.iter().enumerate() {
        if credential.host.is_empty() || credential.token.is_empty() {
            continue;
        }
        let username_key = format!("KODEM_GIT_USERNAME_{ix}");
        let token_key = format!("KODEM_GIT_TOKEN_{ix}");
        command
            .arg("-c")
            .arg(credential_helper_config(
                &credential.host,
                &username_key,
                &token_key,
            ))
            .env(&username_key, account_username(credential))
            .env(&token_key, &credential.token);
    }
}

/// The `-c` value that installs a shell credential helper for `host`.
fn credential_helper_config(host: &str, username_key: &str, token_key: &str) -> String {
    format!(
        "credential.https://{host}.helper=!f() {{ test \"$1\" = get || exit 0; \
         echo \"username=${username_key}\"; echo \"password=${token_key}\"; }}; f"
    )
}

/// GitHub accepts any user name with a token; an empty name still needs
/// a placeholder so git does not ask for one.
fn account_username(credential: &AccountCredential) -> &str {
    if credential.username.is_empty() {
        "x-access-token"
    } else {
        &credential.username
    }
}

/// One step of `git clone --progress` output.
#[derive(Debug, Clone, PartialEq)]
pub struct CloneProgress {
    pub phase: String,
    pub percent: Option<u32>,
}

impl CloneProgress {
    /// Reads one progress line such as `Receiving objects:  45% (9/20)`.
    pub fn parse(line: &str) -> Option<Self> {
        let line = line.trim().trim_start_matches("remote:").trim();
        if line.is_empty() {
            return None;
        }
        let Some((phase, rest)) = line.split_once(':') else {
            return Some(Self {
                phase: line.to_string(),
                percent: None,
            });
        };
        let percent = rest
            .trim()
            .split('%')
            .next()
            .and_then(|value| value.trim().parse::<u32>().ok())
            .filter(|_| rest.contains('%'));
        Some(Self {
            phase: phase.trim().to_string(),
            percent,
        })
    }

    /// The share of the whole clone that is done, from `0` to `1`.
    /// Downloading takes most of the time, so it gets most of the bar.
    pub fn overall_fraction(&self) -> f32 {
        let (start, span) = match self.phase.as_str() {
            "Enumerating objects" | "Counting objects" => (0.0, 0.04),
            "Compressing objects" => (0.04, 0.06),
            "Receiving objects" => (0.1, 0.7),
            "Resolving deltas" => (0.8, 0.12),
            "Updating files" | "Checking out files" | "Filtering content" => (0.92, 0.08),
            _ => return 0.0,
        };
        start + span * self.percent.unwrap_or(0).min(100) as f32 / 100.0
    }
}

/// The folder name that `git clone` picks for `url`.
pub fn repository_name_from_url(url: &str) -> String {
    let trimmed = url.trim().trim_end_matches('/');
    let last = trimmed
        .rsplit(['/', ':'])
        .next()
        .unwrap_or_default()
        .trim_end_matches(".git");
    if last.is_empty() {
        "repository".to_string()
    } else {
        last.to_string()
    }
}

/// Clones `url` into a new folder inside `directory` and returns the
/// path of the new repository. `on_progress` gets each progress step.
/// Blocking; run on a background thread.
pub fn clone_repository(
    url: &str,
    directory: &Path,
    mut on_progress: impl FnMut(CloneProgress),
) -> Result<PathBuf> {
    let url = url.trim();
    if url.is_empty() {
        bail!("Enter a repository URL.");
    }
    let destination = directory.join(repository_name_from_url(url));
    if destination
        .read_dir()
        .is_ok_and(|mut entries| entries.next().is_some())
    {
        bail!(
            "The folder {} already exists and is not empty.",
            destination.display()
        );
    }

    let mut command = base_git_command();
    command
        .current_dir(directory)
        .args(["clone", "--progress", url])
        .arg(&destination)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let mut child = command.spawn().context("failed to spawn git")?;
    let mut stderr = child.stderr.take().context("failed to read git output")?;

    // Progress lines end in `\r` while they update and in `\n` when done.
    let mut output = String::new();
    let mut line = Vec::new();
    let mut buffer = [0u8; 1024];
    loop {
        let read = stderr.read(&mut buffer).unwrap_or(0);
        if read == 0 {
            break;
        }
        for byte in &buffer[..read] {
            if *byte == b'\r' || *byte == b'\n' {
                let text = String::from_utf8_lossy(&line).into_owned();
                if let Some(progress) = CloneProgress::parse(&text) {
                    on_progress(progress);
                }
                if *byte == b'\n' {
                    output.push_str(&text);
                    output.push('\n');
                }
                line.clear();
            } else {
                line.push(*byte);
            }
        }
    }

    let status = child.wait().context("git clone did not finish")?;
    if !status.success() {
        let message = output
            .lines()
            .filter(|line| line.starts_with("fatal:") || line.starts_with("error:"))
            .map(|line| {
                line.trim_start_matches("fatal:")
                    .trim_start_matches("error:")
                    .trim()
            })
            .collect::<Vec<_>>()
            .join(" ");
        if message.is_empty() {
            bail!("git clone failed.");
        }
        bail!("{message}");
    }
    Ok(destination)
}
