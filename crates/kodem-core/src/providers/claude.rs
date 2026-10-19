//! Claude Code identities and the CLI's location, shared with the host driver.
use crate::platform;
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    path::PathBuf,
    process::{Command, Stdio},
};

/// Resolves the native CLI, including the installer location absent from GUI PATH values.
pub fn executable() -> PathBuf {
    if let Some(path) = std::env::var_os("KODEM_CLAUDE_PATH") {
        return PathBuf::from(path);
    }
    platform::find_program("claude", &platform::tool_directories())
        .unwrap_or_else(|| PathBuf::from("claude"))
}

/// Runs the CLI to read its version, such as `2.1.300 (Claude Code)`, without starting a session.
pub fn version() -> Result<String> {
    let mut command = Command::new(executable());
    platform::hide_console(&mut command);
    let output = command
        .arg("--version")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .context("Claude Code did not start")?;
    ensure!(
        output.status.success(),
        "Claude Code did not report a version"
    );
    let version = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    ensure!(!version.is_empty(), "Claude Code did not report a version");
    Ok(version)
}

/// Derives the stable message identity Claude Code echoes for one accepted submission.
///
/// The CLI requires a UUID. Deriving it from the submission keeps receipts verifiable after a
/// host restart without storing a second identity.
pub fn message_uuid(submission: &str) -> String {
    let digest = Sha256::digest(format!("kodem-claude-message:{submission}"));
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    uuid::Builder::from_random_bytes(bytes)
        .into_uuid()
        .to_string()
}
