//! The MCP servers and skills of the providers' CLIs: what each CLI's own configuration holds,
//! the catalogs that offer more, and the changes that the Plugins workspace makes.
//!
//! Every function here reads or writes the user's files or the network, so callers run it on a
//! background thread. Each CLI keeps its own configuration: Kodem never merges them into one
//! file of its own, it writes each CLI's file in that CLI's format.
pub mod catalog;
pub mod mcp;
pub mod skills;

use anyhow::{Context, Result};
use kodem_core::{
    Account, Provider, platform,
    providers::{antigravity, claude, codex, grok, opencode},
};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

/// Returns the providers whose CLI is on this computer, in the order of `Provider::ALL`.
pub fn installed_providers() -> Vec<Provider> {
    Provider::ALL
        .into_iter()
        .filter(|provider| installed(*provider))
        .collect()
}

// Reports whether a provider's CLI is on this computer, without starting it.
fn installed(provider: Provider) -> bool {
    match provider {
        // A name without a folder is the fallback for a CLI that was not found.
        Provider::Claude => claude::executable().is_absolute(),
        Provider::Codex => codex::executable().is_ok(),
        Provider::OpenCode => opencode::executable().is_ok(),
        Provider::Antigravity => antigravity::executable().is_ok(),
        Provider::Grok => grok::executable().is_ok(),
    }
}

/// Returns the configuration folder of an account, as its CLI reads it: the folder of an added
/// account, else the CLI's own.
fn account_home(account: &Account) -> Result<PathBuf> {
    if let Some(home) = &account.home {
        return Ok(home.clone());
    }
    let home = platform::home_dir()?;
    Ok(match account.provider {
        Provider::Claude => home.join(".claude"),
        Provider::Codex => home.join(".codex"),
        Provider::OpenCode => opencode_config_dir()?,
        Provider::Antigravity => home.join(".gemini").join("config"),
        Provider::Grok => grok::home(None)?,
    })
}

/// Returns OpenCode's configuration folder. Its accounts share it: an added account changes
/// only the folder of credentials and sessions.
fn opencode_config_dir() -> Result<PathBuf> {
    match std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from) {
        Some(path) if path.is_absolute() => Ok(path.join("opencode")),
        _ => Ok(platform::home_dir()?.join(".config").join("opencode")),
    }
}

/// Returns the accounts of `provider` among `accounts`, with the CLI's own account first even
/// when the list does not hold it.
fn provider_accounts(accounts: &[Account], provider: Provider) -> Vec<Account> {
    let mut found: Vec<Account> = accounts
        .iter()
        .filter(|account| account.provider == provider)
        .cloned()
        .collect();
    if !found.iter().any(Account::is_default) {
        found.insert(
            0,
            Account {
                id: kodem_core::DEFAULT_ACCOUNT.into(),
                provider,
                name: String::new(),
                home: None,
                color: None,
                created: 0,
            },
        );
    }
    found
}

/// Returns `path` with its links resolved when it exists, so that a write through a linked
/// file of an added account changes the file it links to, and two accounts that share a file
/// name it the same way.
fn resolved(path: &Path) -> PathBuf {
    fs::canonicalize(path)
        .map(platform::plain_path)
        .unwrap_or_else(|_| path.to_path_buf())
}

/// Replaces a file's content in one step: the bytes go to a file beside it, which then takes its
/// place, with the permissions of the file it replaces. A reader never sees half a file.
fn write_atomically(path: &Path, bytes: &[u8]) -> Result<()> {
    let path = resolved(path);
    let folder = path
        .parent()
        .context("The configuration file has no folder")?;
    fs::create_dir_all(folder)
        .with_context(|| format!("Cannot create the folder {}", folder.display()))?;
    let mut staged = tempfile::Builder::new()
        .prefix(".kodem-")
        .tempfile_in(folder)
        .with_context(|| format!("Cannot write in {}", folder.display()))?;
    staged.write_all(bytes)?;
    staged.as_file().sync_all()?;
    if let Ok(metadata) = fs::metadata(&path) {
        staged.as_file().set_permissions(metadata.permissions())?;
    }
    staged
        .persist(&path)
        .map_err(|error| error.error)
        .with_context(|| format!("Cannot replace {}", path.display()))?;
    Ok(())
}

/// Shortens a path under the home folder to `~/…` for display.
pub fn display_path(path: &Path) -> String {
    match platform::home_dir() {
        Ok(home) => match path.strip_prefix(&home) {
            Ok(rest) => format!("~/{}", rest.display()),
            Err(_) => path.display().to_string(),
        },
        Err(_) => path.display().to_string(),
    }
}
