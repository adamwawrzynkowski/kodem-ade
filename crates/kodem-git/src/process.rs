//! Child processes of the Git workspace: Git, Git LFS, GitHub CLI, curl and the Codex CLI.
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command;

use kodem_core::platform;

/// Tool folders that a Mac app opened from Finder lacks in its `PATH`, beyond the ones that
/// Kodem's Git search path already adds: Homebrew's and MacPorts' `sbin` folders and Apple's
/// own tool folder.
#[cfg(target_os = "macos")]
const EXTRA_DIRECTORIES: &[&str] = &[
    "/opt/homebrew/sbin",
    "/usr/local/sbin",
    "/opt/local/bin",
    "/opt/local/sbin",
    "/Library/Apple/usr/bin",
];
#[cfg(not(target_os = "macos"))]
const EXTRA_DIRECTORIES: &[&str] = &[];

/// Returns the `PATH` for the workspace's tools: the process's own `PATH` with the folders
/// where Git, Git LFS, GitHub CLI and package managers install their programs.
pub fn search_path() -> OsString {
    let base = kodem_core::git::search_path()
        .unwrap_or_else(|_| std::env::var_os("PATH").unwrap_or_default());
    let mut directories = std::env::split_paths(&base).collect::<Vec<_>>();
    for extra in EXTRA_DIRECTORIES {
        let extra = PathBuf::from(extra);
        if !directories.contains(&extra) {
            directories.push(extra);
        }
    }
    std::env::join_paths(directories).unwrap_or(base)
}

/// Builds a command for one of the workspace's tools with the tool search path. On Windows the
/// tool gets no console window.
pub fn command(program: &str) -> Command {
    let mut command = Command::new(program);
    platform::hide_console(&mut command);
    command.env("PATH", search_path());
    command
}
