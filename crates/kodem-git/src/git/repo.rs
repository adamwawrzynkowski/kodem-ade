// Some operations form the full git API surface; the UI does not call
// every one of them yet.
#![allow(dead_code)]

use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Condvar, Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result, bail};

use super::models::*;
use super::parse;

/// Field separator used in `--pretty=format:` strings.
pub const FIELD_SEP: char = '\u{1f}';
/// Record separator used in `--pretty=format:` strings.
pub const RECORD_SEP: char = '\u{1e}';

/// A local reference or a remote branch to check out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BranchCheckout {
    Local(String),
    Remote {
        reference: String,
        local_name: String,
    },
}

const DETAILED_STATUS_METRIC_CHANGE_LIMIT: usize = 2_000;
const UNTRACKED_METRIC_BYTE_LIMIT: u64 = 5 * 1024 * 1024;
/// A git command with the shared environment and the linked account
/// credentials, not tied to a repository.
pub(crate) fn base_git_command() -> Command {
    let mut command = crate::process::command("git");
    command.env("GIT_TERMINAL_PROMPT", "0").env("LC_ALL", "C");
    super::clone::apply_account_credentials(&mut command);
    command
}

/// The editor that Git treats as no editor at all: it keeps the prepared message unchanged.
const NO_EDITOR: &str = ":";

/// Quotes a path for the shell that Git runs an editor command in. Git for Windows runs it in
/// its own shell as well, which reads forward slashes.
fn shell_quoted(path: &Path) -> String {
    let text = path.to_string_lossy();
    let text = if cfg!(windows) {
        text.replace('\\', "/")
    } else {
        text.into_owned()
    };
    format!("'{}'", text.replace('\'', "'\\''"))
}

fn git_command(repository_path: &Path) -> Command {
    let mut command = base_git_command();
    command.arg("-C").arg(repository_path);
    command
}

/// The most git commands that run at the same time. Apps opened from
/// Finder may only open 256 files, and each running git process holds
/// several pipes. Without a limit, loading many repositories at once
/// fails to spawn git and the lists come back empty.
const MAX_RUNNING_GIT_COMMANDS: usize = 16;

static RUNNING_GIT_COMMANDS: Mutex<usize> = Mutex::new(0);
static GIT_COMMAND_FINISHED: Condvar = Condvar::new();

/// One of the `MAX_RUNNING_GIT_COMMANDS` slots, given back when dropped.
struct GitCommandSlot;

impl GitCommandSlot {
    /// Waits until a slot is free.
    fn acquire() -> Self {
        let mut running = RUNNING_GIT_COMMANDS
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        while *running >= MAX_RUNNING_GIT_COMMANDS {
            running = GIT_COMMAND_FINISHED
                .wait(running)
                .unwrap_or_else(PoisonError::into_inner);
        }
        *running += 1;
        Self
    }
}

impl Drop for GitCommandSlot {
    fn drop(&mut self) {
        let mut running = RUNNING_GIT_COMMANDS
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        *running -= 1;
        GIT_COMMAND_FINISHED.notify_one();
    }
}

fn git_command_error(args: &[&str], stderr: &str) -> anyhow::Error {
    let stderr = stderr.trim();
    if (stderr.contains("git-lfs")
        && (stderr.contains("command not found") || stderr.contains("not found on your path")))
        || stderr.contains("'lfs' is not a git command")
    {
        return anyhow::anyhow!(
            "Git LFS is required for this repository, but Git could not find it. Install Git LFS, then restart Kodem."
        );
    }
    anyhow::anyhow!("git {} failed: {}", args.join(" "), stderr)
}

fn apply_status_metrics(entries: &mut [StatusEntry], metrics: &HashMap<String, (u32, u32)>) {
    for entry in entries {
        if let Some((additions, deletions)) = metrics.get(&entry.path) {
            entry.additions = *additions;
            entry.deletions = *deletions;
        }
    }
}

fn parse_submodule_index_paths(output: &[u8]) -> HashSet<String> {
    output
        .split(|byte| *byte == 0)
        .filter_map(|record| {
            if !record.starts_with(b"160000 ") {
                return None;
            }
            let tab = record.iter().position(|byte| *byte == b'\t')?;
            Some(String::from_utf8_lossy(&record[tab + 1..]).into_owned())
        })
        .collect()
}

/// Options for a fetch: all remotes or only the default one, pruning of deleted remote branches,
/// and submodule recursion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FetchOptions {
    pub all_remotes: bool,
    pub prune: bool,
    pub recurse_submodules: bool,
}

/// Options for a pull. After the pull, `recurse_submodules` also updates the submodules and
/// `pull_lfs` also pulls the LFS objects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PullOptions {
    pub rebase: bool,
    pub ff_only: bool,
    pub no_commit: bool,
    pub pull_lfs: bool,
    pub auto_stash: bool,
    pub recurse_submodules: bool,
}

/// Options for a push. A branch without an upstream always gets one, even when `set_upstream` is
/// off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PushOptions {
    pub force: bool,
    pub force_with_lease: bool,
    pub set_upstream: bool,
    pub tags: bool,
    pub recurse_submodules: bool,
}

/// Options for a merge. `stash_first` stashes local changes before the merge and restores them
/// after it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MergeOptions {
    pub no_ff: bool,
    pub squash: bool,
    pub no_commit: bool,
    pub message: Option<String>,
    pub stash_first: bool,
}

/// Options for a rebase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RebaseOptions {
    pub autostash: bool,
}

/// One line of an interactive rebase plan: the action (`pick`, `squash`, `fixup` or `drop`), the
/// commit hash and its subject.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InteractiveRebaseItem {
    pub action: String,
    pub hash: String,
    pub subject: String,
}

#[derive(Debug, Clone)]
enum FileRevisionSource {
    Worktree(String),
    GitObject(String),
}

fn is_image_path(path: &str) -> bool {
    let extension = Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(
        extension.as_str(),
        "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "bmp"
            | "tiff"
            | "tif"
            | "webp"
            | "ico"
            | "svg"
            | "heic"
            | "heif"
            | "psd"
            | "pnm"
            | "pbm"
            | "pgm"
            | "ppm"
    )
}

fn preview_image_format(path: &str, bytes: &[u8]) -> Option<PreviewImageFormat> {
    let detected = image::guess_format(bytes)
        .ok()
        .and_then(|format| match format {
            image::ImageFormat::Png => Some(PreviewImageFormat::Png),
            image::ImageFormat::Jpeg => Some(PreviewImageFormat::Jpeg),
            image::ImageFormat::WebP => Some(PreviewImageFormat::Webp),
            image::ImageFormat::Gif => Some(PreviewImageFormat::Gif),
            image::ImageFormat::Bmp => Some(PreviewImageFormat::Bmp),
            image::ImageFormat::Tiff => Some(PreviewImageFormat::Tiff),
            image::ImageFormat::Ico => Some(PreviewImageFormat::Ico),
            image::ImageFormat::Pnm => Some(PreviewImageFormat::Pnm),
            _ => None,
        });
    if detected.is_some() {
        return detected;
    }

    let extension = Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();
    if extension.eq_ignore_ascii_case("svg") {
        Some(PreviewImageFormat::Svg)
    } else {
        None
    }
}

fn supports_image_preview_path(path: &str) -> bool {
    let extension = Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(
        extension.as_str(),
        "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "bmp"
            | "tiff"
            | "tif"
            | "webp"
            | "ico"
            | "svg"
            | "pnm"
            | "pbm"
            | "pgm"
            | "ppm"
    )
}

fn image_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()
}

/// A handle to one repository on disk. All methods block; run them on a
/// background thread.
#[derive(Debug, Clone)]
pub struct GitRepo {
    pub path: PathBuf,
}

impl GitRepo {
    /// Creates a handle for the repository at `path`. It does not check that the path holds a
    /// repository.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Returns the folder name of the repository, or the full path when the path has no folder
    /// name.
    pub fn name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| self.path.display().to_string())
    }

    /// Run git with the given arguments. Returns stdout on success.
    pub fn run(&self, args: &[&str]) -> Result<String> {
        self.run_with_environment(args, &[])
    }

    fn run_with_environment(&self, args: &[&str], environment: &[(&str, &str)]) -> Result<String> {
        let mut command = git_command(&self.path);
        command.args(args);
        for (key, value) in environment {
            command.env(key, value);
        }
        let slot = GitCommandSlot::acquire();
        let output = command.output();
        drop(slot);
        let output = output.context("failed to spawn git")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(git_command_error(args, &stderr));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    /// Returns whether `path` is inside a Git working tree.
    pub fn is_repository(path: &Path) -> bool {
        let output = git_command(path)
            .args(["rev-parse", "--is-inside-work-tree"])
            .output();
        matches!(output, Ok(o) if o.status.success())
    }

    fn conflict_operation(&self) -> Option<ConflictOperation> {
        let git_directory = self.run(&["rev-parse", "--absolute-git-dir"]).ok()?;
        let git_directory = PathBuf::from(git_directory.trim());
        if git_directory.join("rebase-merge").exists()
            || git_directory.join("rebase-apply").exists()
        {
            Some(ConflictOperation::Rebase)
        } else if git_directory.join("MERGE_HEAD").exists() {
            Some(ConflictOperation::Merge)
        } else if git_directory.join("CHERRY_PICK_HEAD").exists() {
            Some(ConflictOperation::CherryPick)
        } else if git_directory.join("REVERT_HEAD").exists() {
            Some(ConflictOperation::Revert)
        } else {
            None
        }
    }

    /// Aborts the rebase, merge, cherry-pick or revert that stopped with conflicts. When none is in
    /// progress, it runs `git reset --merge HEAD`.
    pub fn abort_conflict_operation(&self) -> Result<()> {
        match self.conflict_operation() {
            Some(ConflictOperation::Rebase) => self.run(&["rebase", "--abort"]),
            Some(ConflictOperation::Merge) => self.run(&["merge", "--abort"]),
            Some(ConflictOperation::CherryPick) => self.run(&["cherry-pick", "--abort"]),
            Some(ConflictOperation::Revert) => self.run(&["revert", "--abort"]),
            None => self.run(&["reset", "--merge", "HEAD"]),
        }
        .map(|_| ())
    }

    // ------------------------------------------------------------------
    // Read operations
    // ------------------------------------------------------------------

    /// Returns the short name of the checked-out branch, or `HEAD` when `HEAD` is detached.
    pub fn current_branch(&self) -> Result<String> {
        let out = self.run(&["rev-parse", "--abbrev-ref", "HEAD"])?;
        Ok(out.trim().to_string())
    }

    /// Returns the full hash of `HEAD`.
    pub fn head_hash(&self) -> Result<String> {
        let out = self.run(&["rev-parse", "HEAD"])?;
        Ok(out.trim().to_string())
    }

    /// Returns the revision that an amend of `HEAD` compares the index to: the first parent of
    /// `HEAD`, or the empty tree when `HEAD` is a root commit.
    pub fn amend_base_ref(&self) -> Result<String> {
        let revisions = self.run(&["rev-list", "--parents", "-n", "1", "HEAD"])?;
        let mut revisions = revisions.split_whitespace();
        revisions
            .next()
            .context("Git did not return the current commit")?;
        if let Some(parent) = revisions.next() {
            return Ok(parent.to_string());
        }

        let empty_tree = self.run(&["mktree"])?;
        let empty_tree = empty_tree.trim();
        if empty_tree.is_empty() {
            bail!("Git did not return the empty tree");
        }
        Ok(empty_tree.to_string())
    }

    /// Reads the message of `HEAD` and the changes that an amend of it would commit: the index
    /// compared to the parent of `HEAD`, with line counts and submodule flags.
    pub fn amend_state(&self) -> Result<AmendState> {
        let message = self.run(&["show", "--no-patch", "--format=%s%x1f%b", "HEAD"])?;
        let (title, description) = message
            .split_once(FIELD_SEP)
            .map(|(title, description)| (title.trim_end(), description.trim_end()))
            .unwrap_or((message.trim_end(), ""));
        let base = self.amend_base_ref()?;
        let name_status = self.run(&[
            "diff",
            "--cached",
            "--name-status",
            "-z",
            "--no-color",
            &base,
            "--",
        ])?;
        let mut staged = parse::parse_name_status(&name_status, true);
        if !staged.is_empty() {
            let numstat = self.run(&[
                "diff",
                "--cached",
                "--numstat",
                "-z",
                "--no-color",
                &base,
                "--",
            ])?;
            apply_status_metrics(&mut staged, &parse::parse_numstat(&numstat));
            let status = RepoStatus {
                staged: staged.clone(),
                ..RepoStatus::default()
            };
            let submodule_paths = self.changed_submodule_paths(&status);
            for entry in &mut staged {
                entry.is_submodule = submodule_paths.contains(&entry.path);
            }
        }

        Ok(AmendState {
            title: title.to_string(),
            description: description.to_string(),
            staged,
        })
    }

    /// Load history. `limit` bounds the number of commits.
    pub fn log(&self, limit: usize) -> Result<Vec<Commit>> {
        let format = format!(
            "%H{s}%h{s}%P{s}%an{s}%ae{s}%at{s}%ct{s}%D{s}%s{s}%b{r}",
            s = FIELD_SEP,
            r = RECORD_SEP
        );
        let limit_arg = format!("--max-count={limit}");
        let format_arg = format!("--pretty=format:{format}");
        let out = self.run(&[
            "log",
            "--topo-order",
            "--branches",
            "--remotes",
            "--tags",
            "HEAD",
            &limit_arg,
            &format_arg,
        ])?;
        Ok(parse::parse_log(&out))
    }

    /// Loads additions and deletions of the given commits with one
    /// numstat pass. Returns (hash, additions, deletions) tuples.
    pub fn commit_stats(&self, hashes: &[String]) -> Result<Vec<(String, u32, u32)>> {
        if hashes.is_empty() {
            return Ok(Vec::new());
        }
        let format_arg = format!("--pretty=format:{}%H", RECORD_SEP);
        let mut args = vec!["log", "--no-walk=unsorted", &format_arg, "--numstat"];
        args.extend(hashes.iter().map(String::as_str));
        args.push("--");
        let out = self.run(&args)?;
        Ok(parse::parse_log_stats(&out))
    }

    /// Reads the status with submodule flags and line counts.
    pub fn status(&self) -> Result<RepoStatus> {
        Ok(self.status_details(self.status_fast()?))
    }

    /// Adds submodule flags and line counts to a status that
    /// [`Self::status_fast`] read, so the status is not read again.
    pub fn status_details(&self, mut status: RepoStatus) -> RepoStatus {
        let submodule_paths = self.changed_submodule_paths(&status);
        for entry in status
            .staged
            .iter_mut()
            .chain(&mut status.unstaged)
            .chain(&mut status.conflicted)
        {
            entry.is_submodule = submodule_paths.contains(&entry.path);
        }

        if !status.staged.is_empty()
            && let Ok(out) = self.run(&["diff", "--cached", "--numstat", "-z", "--no-color", "--"])
        {
            apply_status_metrics(&mut status.staged, &parse::parse_numstat(&out));
        }
        if status
            .unstaged
            .iter()
            .any(|entry| entry.kind != FileChangeKind::Untracked)
            && let Ok(out) = self.run(&["diff", "--numstat", "-z", "--no-color", "--"])
        {
            let metrics = parse::parse_numstat(&out);
            apply_status_metrics(&mut status.unstaged, &metrics);
            apply_status_metrics(&mut status.conflicted, &metrics);
        }

        let change_count = status.staged.len() + status.unstaged.len() + status.conflicted.len();
        if change_count <= DETAILED_STATUS_METRIC_CHANGE_LIMIT {
            for entry in &mut status.unstaged {
                if entry.kind == FileChangeKind::Untracked {
                    entry.additions = self.untracked_text_line_count(&entry.path).unwrap_or(0);
                }
            }
        }

        status
    }

    /// Reads the status without line counts or submodule flags, and adds the conflict operation
    /// when files are conflicted. Use [`Self::status_details`] to add the rest later.
    pub fn status_fast(&self) -> Result<RepoStatus> {
        let out = self.run(&["status", "--porcelain=v1", "-z", "--untracked-files=all"])?;
        let mut status = parse::parse_status(&out);
        if !status.conflicted.is_empty() {
            status.conflict_operation = self.conflict_operation();
        }
        Ok(status)
    }

    fn changed_submodule_paths(&self, status: &RepoStatus) -> HashSet<String> {
        let paths = status
            .staged
            .iter()
            .chain(&status.unstaged)
            .chain(&status.conflicted)
            .map(|entry| entry.path.as_str())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        if paths.is_empty() {
            return HashSet::new();
        }

        let mut submodule_paths = HashSet::new();
        for paths in paths.chunks(500) {
            let mut args = vec!["ls-files", "--stage", "-z", "--"];
            args.extend_from_slice(paths);
            if let Ok(output) = self.run(&args) {
                submodule_paths.extend(parse_submodule_index_paths(output.as_bytes()));
            }
        }
        submodule_paths
    }

    fn untracked_text_line_count(&self, path: &str) -> Option<u32> {
        let path = self.path.join(path);
        let metadata = std::fs::metadata(&path).ok()?;
        if !metadata.is_file() || metadata.len() > UNTRACKED_METRIC_BYTE_LIMIT {
            return None;
        }
        let data = std::fs::read(path).ok()?;
        std::str::from_utf8(&data).ok()?;
        if data.is_empty() {
            return Some(0);
        }
        let line_breaks = data.iter().filter(|byte| **byte == b'\n').count();
        let line_count = if data.last() == Some(&b'\n') {
            line_breaks
        } else {
            line_breaks + 1
        };
        Some(line_count as u32)
    }

    /// Returns the local branches, then the remote-tracking branches.
    pub fn branches(&self) -> Result<Vec<Branch>> {
        let format = format!(
            "%(refname:short){s}%(HEAD){s}%(upstream:short){s}%(upstream:track){s}%(committerdate:unix){s}%(objectname){s}%(objectname:short){s}%(contents:subject)",
            s = FIELD_SEP
        );
        let format_arg = format!("--format={format}");
        let local = self.run(&["for-each-ref", "refs/heads", &format_arg])?;
        let remote = self.run(&["for-each-ref", "refs/remotes", &format_arg])?;
        let mut branches = parse::parse_branches(&local, false);
        branches.extend(parse::parse_branches(&remote, true));
        Ok(branches)
    }

    /// Returns the remotes with their fetch URLs.
    pub fn remotes(&self) -> Result<Vec<Remote>> {
        let out = self.run(&["remote", "-v"])?;
        Ok(parse::parse_remotes(&out))
    }

    /// Returns the tags, newest first.
    pub fn tags(&self) -> Result<Vec<Tag>> {
        let format = format!(
            "--format=%(refname:short){s}%(objectname:short){s}%(creatordate:unix)",
            s = FIELD_SEP
        );
        let out = self.run(&["for-each-ref", "refs/tags", "--sort=-creatordate", &format])?;
        Ok(parse::parse_tags(&out))
    }

    /// Returns the stash entries, newest first.
    pub fn stashes(&self) -> Result<Vec<Stash>> {
        let format = format!("--format=%gd{s}%gs{s}%cr", s = FIELD_SEP);
        let out = self.run(&["stash", "list", &format])?;
        Ok(parse::parse_stashes(&out))
    }

    /// Returns the worktrees of the repository and marks the one at this repository's path as
    /// current.
    pub fn worktrees(&self) -> Result<Vec<Worktree>> {
        let out = self.run(&["worktree", "list", "--porcelain"])?;
        Ok(parse::parse_worktrees(&out, &self.path))
    }

    /// Checkpoint refs created by AI coding tools.
    pub fn checkpoints(&self) -> Result<Vec<Checkpoint>> {
        let format = format!(
            "--format=%(refname){s}%(objectname:short){s}%(contents:subject)",
            s = FIELD_SEP
        );
        let out = self.run(&[
            "for-each-ref",
            "--sort=-creatordate",
            &format,
            "refs/t3/checkpoints",
            "refs/synara/checkpoints",
        ])?;
        let mut checkpoints = Vec::new();
        for line in out.lines() {
            let fields: Vec<&str> = line.split(FIELD_SEP).collect();
            if fields.len() < 3 {
                continue;
            }
            checkpoints.push(Checkpoint {
                reference: fields[0].to_string(),
                hash: fields[1].to_string(),
                message: fields[2].to_string(),
            });
        }
        Ok(checkpoints)
    }

    /// Tracked LFS patterns and object count. Slow; call it only when
    /// the LFS category is visible.
    pub fn lfs_info(&self) -> LfsInfo {
        let Ok(track) = self.run(&["lfs", "track"]) else {
            return LfsInfo::default();
        };
        // Pattern lines are indented under "Listing tracked patterns".
        let patterns: Vec<String> = track
            .lines()
            .take_while(|l| !l.trim_start().starts_with("Listing excluded"))
            .filter(|l| l.starts_with(char::is_whitespace))
            .filter_map(|l| l.trim().split(" (").next().map(|p| p.to_string()))
            .filter(|p| !p.is_empty())
            .collect();
        let object_count = self
            .run(&["lfs", "ls-files", "--name-only"])
            .map(|out| out.lines().filter(|l| !l.trim().is_empty()).count())
            .unwrap_or(0);
        LfsInfo {
            available: true,
            patterns,
            object_count,
        }
    }

    /// Diff of one file in the working tree or the index.
    pub fn diff_file(
        &self,
        path: &str,
        original_path: Option<&str>,
        staged: bool,
        context: u32,
    ) -> Result<FileDiff> {
        let context_arg = format!("-U{context}");
        let mut args: Vec<&str> = vec!["diff", "--no-color", &context_arg];
        if staged {
            args.push("--cached");
        }
        args.push("--");
        args.push(path);
        let out = self.run(&args)?;
        if out.trim().is_empty() && !staged {
            // The file can be untracked. Diff it against /dev/null.
            let out = git_command(&self.path)
                .args([
                    "diff",
                    "--no-color",
                    &context_arg,
                    "--no-index",
                    "/dev/null",
                    path,
                ])
                .output()
                .context("failed to spawn git")?;
            // `--no-index` exits with 1 when there are differences.
            let text = String::from_utf8_lossy(&out.stdout).into_owned();
            let mut diff = parse::parse_diff(&text)
                .into_iter()
                .next()
                .unwrap_or_default();
            self.attach_file_comparison(
                &mut diff,
                path,
                Some(FileRevisionSource::GitObject(format!(
                    ":{}",
                    original_path.unwrap_or(path)
                ))),
                Some(FileRevisionSource::Worktree(path.to_string())),
            );
            return Ok(diff);
        }
        let mut diff = parse::parse_diff(&out)
            .into_iter()
            .next()
            .unwrap_or_default();
        let before_path = original_path.unwrap_or(path);
        let before = if staged {
            FileRevisionSource::GitObject(format!("HEAD:{before_path}"))
        } else {
            FileRevisionSource::GitObject(format!(":{before_path}"))
        };
        let after = if staged {
            FileRevisionSource::GitObject(format!(":{path}"))
        } else {
            FileRevisionSource::Worktree(path.to_string())
        };
        self.attach_file_comparison(&mut diff, path, Some(before), Some(after));
        Ok(diff)
    }

    /// Diff of one file between the parent of `HEAD` and the index, which is what an amend of
    /// `HEAD` commits.
    pub fn diff_amend_file(
        &self,
        path: &str,
        original_path: Option<&str>,
        context: u32,
    ) -> Result<FileDiff> {
        let context_arg = format!("-U{context}");
        let base = self.amend_base_ref()?;
        let out = self.run(&[
            "diff",
            "--no-color",
            &context_arg,
            "--cached",
            &base,
            "--",
            path,
        ])?;
        let mut diff = parse::parse_diff(&out)
            .into_iter()
            .next()
            .unwrap_or_default();
        let before_path = original_path.unwrap_or(path);
        let before = (diff.old_path != "/dev/null")
            .then(|| FileRevisionSource::GitObject(format!("{base}:{before_path}")));
        let after = (diff.new_path != "/dev/null")
            .then(|| FileRevisionSource::GitObject(format!(":{path}")));
        self.attach_file_comparison(&mut diff, path, before, after);
        Ok(diff)
    }

    /// Diff of one file inside one commit.
    pub fn diff_commit_file(&self, hash: &str, path: &str, context: u32) -> Result<FileDiff> {
        let context_arg = format!("-U{context}");
        let range = format!("{hash}^!");
        let out = self.run(&["diff", "--no-color", &context_arg, &range, "--", path])?;
        let mut diff = parse::parse_diff(&out)
            .into_iter()
            .next()
            .unwrap_or_default();
        let before_path = if diff.old_path.is_empty() || diff.old_path == "/dev/null" {
            path
        } else {
            &diff.old_path
        };
        let after_path = if diff.new_path.is_empty() || diff.new_path == "/dev/null" {
            path
        } else {
            &diff.new_path
        };
        let before = FileRevisionSource::GitObject(format!("{hash}^:{before_path}"));
        let after = FileRevisionSource::GitObject(format!("{hash}:{after_path}"));
        self.attach_file_comparison(&mut diff, path, Some(before), Some(after));
        Ok(diff)
    }

    fn attach_file_comparison(
        &self,
        diff: &mut FileDiff,
        path: &str,
        before_source: Option<FileRevisionSource>,
        after_source: Option<FileRevisionSource>,
    ) {
        let is_image = is_image_path(path);
        let has_lfs = diff.lfs_before.is_some() || diff.lfs_after.is_some();
        if !is_image && !has_lfs && !diff.is_binary {
            return;
        }

        diff.comparison = Some(FileComparison {
            before: self.load_file_revision(
                before_source,
                path,
                is_image,
                diff.lfs_before.as_ref(),
            ),
            after: self.load_file_revision(after_source, path, is_image, diff.lfs_after.as_ref()),
            is_image,
        });
    }

    fn load_file_revision(
        &self,
        source: Option<FileRevisionSource>,
        path: &str,
        is_image: bool,
        known_lfs_pointer: Option<&LfsPointer>,
    ) -> Option<FileRevision> {
        let source = source?;
        if !is_image {
            if let Some(pointer) = known_lfs_pointer {
                return Some(FileRevision {
                    logical_byte_count: Some(pointer.size),
                    lfs_pointer: Some(pointer.clone()),
                    ..FileRevision::default()
                });
            }
            return self.load_file_revision_size(source);
        }

        let stored_bytes = match &source {
            FileRevisionSource::Worktree(relative_path) => {
                match std::fs::read(self.path.join(relative_path)) {
                    Ok(bytes) => bytes,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
                    Err(error) => {
                        return Some(FileRevision {
                            load_error: Some(format!("Version could not be loaded: {error}")),
                            ..FileRevision::default()
                        });
                    }
                }
            }
            FileRevisionSource::GitObject(object) => match self.run_bytes(&["show", object]) {
                Ok(bytes) => bytes,
                Err(_) => return None,
            },
        };
        let stored_pointer = if stored_bytes.len() <= 4096 {
            std::str::from_utf8(&stored_bytes).ok().and_then(|content| {
                LfsPointer::parse(&content.lines().map(ToString::to_string).collect::<Vec<_>>())
            })
        } else {
            None
        };
        let pointer = stored_pointer.as_ref().or(known_lfs_pointer).cloned();
        let logical_byte_count = pointer
            .as_ref()
            .map(|pointer| pointer.size)
            .or(Some(stored_bytes.len() as u64));

        if stored_pointer.is_some() && !supports_image_preview_path(path) {
            return Some(FileRevision {
                logical_byte_count,
                lfs_pointer: pointer,
                load_error: Some("Preview is not available for this image format.".to_string()),
                ..FileRevision::default()
            });
        }

        let image_bytes = if let Some(stored_pointer) = &stored_pointer {
            match self.run_with_stdin_bytes(
                &["lfs", "smudge"],
                stored_pointer.pointer_content().as_bytes(),
            ) {
                Ok(bytes)
                    if std::str::from_utf8(&bytes).ok().is_some_and(|content| {
                        LfsPointer::parse(
                            &content.lines().map(ToString::to_string).collect::<Vec<_>>(),
                        )
                        .is_some()
                    }) =>
                {
                    return Some(FileRevision {
                        logical_byte_count,
                        lfs_pointer: pointer,
                        load_error: Some(
                            "LFS object could not be resolved. Fetch the object first.".to_string(),
                        ),
                        ..FileRevision::default()
                    });
                }
                Ok(bytes) => bytes,
                Err(error) => {
                    return Some(FileRevision {
                        logical_byte_count,
                        lfs_pointer: pointer,
                        load_error: Some(format!("LFS object could not be resolved: {error}")),
                        ..FileRevision::default()
                    });
                }
            }
        } else {
            stored_bytes
        };
        let image_format = preview_image_format(path, &image_bytes);
        let dimensions = image_dimensions(&image_bytes);
        let decoded_byte_count = Some(image_bytes.len() as u64);
        let load_error = image_format
            .is_none()
            .then(|| "Preview is not available for this image format.".to_string());

        Some(FileRevision {
            image_bytes: image_format.map(|_| image_bytes),
            image_format,
            dimensions,
            logical_byte_count,
            decoded_byte_count,
            lfs_pointer: pointer,
            load_error,
        })
    }

    fn load_file_revision_size(&self, source: FileRevisionSource) -> Option<FileRevision> {
        let byte_count = match source {
            FileRevisionSource::Worktree(relative_path) => {
                match std::fs::metadata(self.path.join(relative_path)) {
                    Ok(metadata) => metadata.len(),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
                    Err(error) => {
                        return Some(FileRevision {
                            load_error: Some(format!("Version could not be loaded: {error}")),
                            ..FileRevision::default()
                        });
                    }
                }
            }
            FileRevisionSource::GitObject(object) => self
                .run(&["cat-file", "-s", &object])
                .ok()?
                .trim()
                .parse::<u64>()
                .ok()?,
        };
        Some(FileRevision {
            logical_byte_count: Some(byte_count),
            ..FileRevision::default()
        })
    }

    /// Files changed by one commit.
    pub fn commit_files(&self, hash: &str) -> Result<Vec<CommitFile>> {
        let range = format!("{hash}^!");
        let numstat = self.run(&["diff", "--numstat", "--no-color", &range])?;
        let status = self.run(&["diff", "--name-status", "--no-color", &range])?;
        Ok(parse::parse_commit_files(&numstat, &status))
    }

    /// Returns the full message of a commit without trailing whitespace.
    pub fn commit_body(&self, hash: &str) -> Result<String> {
        let out = self.run(&["show", "--no-patch", "--pretty=format:%B", hash])?;
        Ok(out.trim_end().to_string())
    }

    // ------------------------------------------------------------------
    // Write operations
    // ------------------------------------------------------------------

    /// Stages one path.
    pub fn stage(&self, path: &str) -> Result<()> {
        self.run(&["add", "--", path]).map(|_| ())
    }

    /// Stages the given paths. Does nothing when `paths` is empty.
    pub fn stage_paths(&self, paths: &[String]) -> Result<()> {
        self.run_pathspec_command(
            &["add", "--pathspec-from-file=-", "--pathspec-file-nul"],
            paths,
        )
    }

    /// Stages all changes, including new and deleted files.
    pub fn stage_all(&self) -> Result<()> {
        self.run(&["add", "-A"]).map(|_| ())
    }

    /// Unstages one path. The working tree does not change.
    pub fn unstage(&self, path: &str) -> Result<()> {
        self.run(&["reset", "HEAD", "--", path]).map(|_| ())
    }

    /// Unstages the given paths. Does nothing when `paths` is empty.
    pub fn unstage_paths(&self, paths: &[String]) -> Result<()> {
        self.run_pathspec_command(
            &[
                "reset",
                "HEAD",
                "--pathspec-from-file=-",
                "--pathspec-file-nul",
            ],
            paths,
        )
    }

    /// Unstages all changes. The working tree does not change.
    pub fn unstage_all(&self) -> Result<()> {
        self.run(&["reset", "HEAD"]).map(|_| ())
    }

    /// Unstages the given paths from an amend: resets them in the index to the parent of `HEAD`.
    /// Does nothing when `paths` is empty.
    pub fn unstage_amend_paths(&self, paths: &[String]) -> Result<()> {
        let base = self.amend_base_ref()?;
        self.run_pathspec_command(
            &[
                "reset",
                &base,
                "--pathspec-from-file=-",
                "--pathspec-file-nul",
            ],
            paths,
        )
    }

    /// Runs a mixed reset to the parent of `HEAD`. This moves the branch back one commit, and the
    /// changes of `HEAD` and of the index become unstaged changes in the working tree.
    pub fn unstage_amend_all(&self) -> Result<()> {
        let base = self.amend_base_ref()?;
        self.run(&["reset", &base]).map(|_| ())
    }

    /// Discards the working tree changes of one entry: deletes an untracked file, checks out a
    /// submodule at its recorded commit, or restores a tracked file from the index.
    pub fn discard(&self, entry: &StatusEntry) -> Result<()> {
        if entry.kind == FileChangeKind::Untracked {
            let full = self.path.join(&entry.path);
            std::fs::remove_file(&full).context("failed to delete untracked file")?;
            Ok(())
        } else if entry.is_submodule {
            self.run(&["submodule", "update", "--checkout", "--", &entry.path])
                .map(|_| ())
        } else {
            self.run(&["checkout", "--", &entry.path]).map(|_| ())
        }
    }

    /// Discards the working tree changes of the given entries as [`Self::discard`] does, with one
    /// restore for all tracked files. Stops at the first error.
    pub fn discard_entries(&self, entries: &[StatusEntry]) -> Result<()> {
        let mut tracked_paths = Vec::new();
        for entry in entries {
            if entry.kind == FileChangeKind::Untracked {
                let full_path = self.path.join(&entry.path);
                std::fs::remove_file(&full_path)
                    .with_context(|| format!("failed to delete {}", entry.path))?;
            } else if entry.is_submodule {
                self.run(&["submodule", "update", "--checkout", "--", &entry.path])?;
            } else {
                tracked_paths.push(entry.path.as_str());
            }
        }

        if tracked_paths.is_empty() {
            return Ok(());
        }
        let mut input = Vec::new();
        for path in tracked_paths {
            input.extend_from_slice(path.as_bytes());
            input.push(0);
        }
        self.run_with_stdin(
            &[
                "restore",
                "--worktree",
                "--pathspec-from-file=-",
                "--pathspec-file-nul",
            ],
            &input,
        )
    }

    /// Discards all working tree changes and deletes untracked files and folders. Staged changes
    /// stay in the index.
    pub fn discard_all(&self) -> Result<()> {
        self.run(&["restore", "--worktree", "--ignore-unmerged", "--", "."])?;
        self.run(&["clean", "-fd"]).map(|_| ())
    }

    /// Commits the index with `title` as the subject and `description` as the body, when the
    /// description is not blank. With `amend`, the commit replaces `HEAD`.
    pub fn commit(&self, title: &str, description: &str, amend: bool) -> Result<()> {
        let mut args = vec!["commit", "-m", title];
        if !description.trim().is_empty() {
            args.push("-m");
            args.push(description);
        }
        if amend {
            args.push("--amend");
        }
        self.run(&args).map(|_| ())
    }

    /// Checks out a branch, tag or commit.
    pub fn checkout(&self, reference: &str) -> Result<()> {
        self.run(&["checkout", reference]).map(|_| ())
    }

    /// Checks out a reference, optionally stashing and restoring local changes.
    /// If checkout fails, the saved changes remain in the stash.
    pub fn checkout_branch(&self, target: &BranchCheckout, autostash: bool) -> Result<()> {
        let created_stash = if autostash {
            let previous = self.run(&["rev-parse", "--verify", "refs/stash"]).ok();
            self.stash_push(Some("Auto-stash before checkout"))?;
            let current = self.run(&["rev-parse", "--verify", "refs/stash"]).ok();
            current.is_some() && current != previous
        } else {
            false
        };
        let result = match target {
            BranchCheckout::Local(reference) => self.checkout(reference),
            BranchCheckout::Remote {
                reference,
                local_name,
            } => self.checkout_remote(reference, local_name),
        };
        if created_stash {
            result.map_err(|error| {
                anyhow::anyhow!("Checkout failed: {error}. Local changes remain in stash@{{0}}.")
            })?;
            self.run(&["stash", "pop", "--index", "stash@{0}"])
                .map_err(|error| {
                    anyhow::anyhow!("Checkout succeeded, but local changes could not be fully restored: {error}. The stash was kept.")
                })?;
        } else {
            result?;
        }
        Ok(())
    }

    /// Creates a branch at `HEAD`, and checks it out when `checkout` is set.
    pub fn create_branch(&self, name: &str, checkout: bool) -> Result<()> {
        if checkout {
            self.run(&["checkout", "-b", name]).map(|_| ())
        } else {
            self.run(&["branch", name]).map(|_| ())
        }
    }

    /// Deletes a local branch. Without `force`, Git refuses to delete a branch that is not fully
    /// merged.
    pub fn delete_branch(&self, name: &str, force: bool) -> Result<()> {
        let flag = if force { "-D" } else { "-d" };
        self.run(&["branch", flag, name]).map(|_| ())
    }

    /// Fetches all remotes and prunes remote branches that no longer exist.
    pub fn fetch(&self) -> Result<()> {
        self.run(&["fetch", "--all", "--prune"]).map(|_| ())
    }

    /// Fetches with the given options.
    pub fn fetch_with_options(&self, options: FetchOptions) -> Result<()> {
        self.run(&fetch_args(options)).map(|_| ())
    }

    /// Pulls into the current branch, with rebase when `rebase` is set.
    pub fn pull(&self, rebase: bool) -> Result<()> {
        if rebase {
            self.run(&["pull", "--rebase"]).map(|_| ())
        } else {
            self.run(&["pull"]).map(|_| ())
        }
    }

    /// Pulls with the given options. Then it updates the submodules when `recurse_submodules` is
    /// set, and pulls the LFS objects when `pull_lfs` is set and Git LFS is installed.
    pub fn pull_with_options(&self, options: PullOptions) -> Result<()> {
        self.run(&pull_args(options))?;
        if options.recurse_submodules {
            self.run(&["submodule", "update", "--init", "--recursive"])?;
        }
        if options.pull_lfs && self.run(&["lfs", "version"]).is_ok() {
            self.run(&["lfs", "pull"])?;
        }
        Ok(())
    }

    /// Pushes the current branch, with `--force-with-lease` when `force_with_lease` is set.
    pub fn push(&self, force_with_lease: bool) -> Result<()> {
        if force_with_lease {
            self.run(&["push", "--force-with-lease"]).map(|_| ())
        } else {
            self.run(&["push"]).map(|_| ())
        }
    }

    /// Pushes the current branch with the given options. The remote is the branch's push remote,
    /// `remote.pushDefault`, the upstream remote, or the first remote, in that order.
    pub fn push_with_options(&self, options: PushOptions) -> Result<()> {
        let branch = self.current_branch().unwrap_or_else(|_| "HEAD".to_string());
        let has_upstream = self
            .run(&["rev-parse", "--abbrev-ref", "@{upstream}"])
            .is_ok();
        let remote = self.push_remote_name(&branch);
        let args = push_args(options, &remote, &branch, has_upstream);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        self.run(&refs).map(|_| ())
    }

    fn push_remote_name(&self, branch: &str) -> String {
        let branch_key = format!("branch.{branch}.pushRemote");
        for args in [
            vec!["config", "--get", branch_key.as_str()],
            vec!["config", "--get", "remote.pushDefault"],
        ] {
            if let Ok(remote) = self.run(&args) {
                let remote = remote.trim();
                if !remote.is_empty() {
                    return remote.to_string();
                }
            }
        }

        if let Ok(upstream) = self.run(&["rev-parse", "--abbrev-ref", "@{upstream}"])
            && let Some((remote, _)) = upstream.trim().split_once('/')
        {
            return remote.to_string();
        }

        self.run(&["remote"])
            .ok()
            .and_then(|output| {
                output
                    .lines()
                    .map(str::trim)
                    .find(|name| !name.is_empty())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "origin".to_string())
    }

    /// Stashes all local changes, including untracked files. A blank message uses the default stash
    /// message.
    pub fn stash_push(&self, message: Option<&str>) -> Result<()> {
        match message {
            Some(m) if !m.trim().is_empty() => {
                self.run(&["stash", "push", "-u", "-m", m]).map(|_| ())
            }
            _ => self.run(&["stash", "push", "-u"]).map(|_| ()),
        }
    }

    /// Stashes the changes of the given paths, and their untracked files when `include_untracked`
    /// is set. Does nothing when `paths` is empty.
    pub fn stash_paths(&self, paths: &[String], include_untracked: bool) -> Result<()> {
        if paths.is_empty() {
            return Ok(());
        }
        let mut args = vec!["stash", "push"];
        if include_untracked {
            args.push("--include-untracked");
        }
        args.extend(["--pathspec-from-file=-", "--pathspec-file-nul"]);

        let mut input = Vec::new();
        for path in paths {
            input.extend_from_slice(path.as_bytes());
            input.push(0);
        }
        self.run_with_stdin(&args, &input)
    }

    fn run_pathspec_command(&self, args: &[&str], paths: &[String]) -> Result<()> {
        if paths.is_empty() {
            return Ok(());
        }
        let mut input = Vec::new();
        for path in paths {
            input.extend_from_slice(path.as_bytes());
            input.push(0);
        }
        self.run_with_stdin(args, &input)
    }

    /// Applies a stash entry and keeps it in the stash.
    pub fn stash_apply(&self, reference: &str) -> Result<()> {
        self.run(&["stash", "apply", reference]).map(|_| ())
    }

    /// Deletes a stash entry.
    pub fn stash_drop(&self, reference: &str) -> Result<()> {
        self.run(&["stash", "drop", reference]).map(|_| ())
    }

    /// Applies the changes of a commit as a new commit on the current branch.
    pub fn cherry_pick(&self, hash: &str) -> Result<()> {
        self.run(&["cherry-pick", hash]).map(|_| ())
    }

    /// Creates a commit that reverts `hash`, with the default message.
    pub fn revert(&self, hash: &str) -> Result<()> {
        self.run(&["revert", "--no-edit", hash]).map(|_| ())
    }

    /// Resets `HEAD` to `hash`. `mode` is the reset mode without dashes, such as `soft`, `mixed` or
    /// `hard`.
    pub fn reset(&self, hash: &str, mode: &str) -> Result<()> {
        let flag = format!("--{mode}");
        self.run(&["reset", &flag, hash]).map(|_| ())
    }

    /// Creates a lightweight tag at `hash`.
    pub fn create_tag(&self, name: &str, hash: &str) -> Result<()> {
        self.run(&["tag", name, hash]).map(|_| ())
    }

    /// Deletes a local tag.
    pub fn delete_tag(&self, name: &str) -> Result<()> {
        self.run(&["tag", "-d", name]).map(|_| ())
    }

    /// Creates a branch at `start` and checks it out.
    pub fn create_and_checkout_branch_at(&self, name: &str, start: &str) -> Result<()> {
        self.run(&["checkout", "-b", name, start]).map(|_| ())
    }

    /// Merges a branch into the current branch with the default options.
    pub fn merge(&self, branch: &str) -> Result<()> {
        self.merge_with_options(branch, MergeOptions::default())
    }

    /// Rebases the current branch onto `onto` with the default options.
    pub fn rebase(&self, onto: &str) -> Result<()> {
        self.rebase_with_options(onto, RebaseOptions::default())
    }

    /// Merges a branch into the current branch. With `stash_first`, local changes are stashed
    /// before the merge and popped after it; when the merge fails, they stay in the stash.
    pub fn merge_with_options(&self, branch: &str, options: MergeOptions) -> Result<()> {
        let args = merge_args(branch, &options);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        if !options.stash_first {
            return self.run(&refs).map(|_| ());
        }

        let previous_stash = self.run(&["rev-parse", "--verify", "refs/stash"]).ok();
        self.stash_push(Some(&format!("Auto-stash before merge {branch}")))?;
        let current_stash = self.run(&["rev-parse", "--verify", "refs/stash"]).ok();
        let created_stash = current_stash.is_some() && current_stash != previous_stash;
        self.run(&refs)?;
        if created_stash {
            self.stash_pop("stash@{0}")?;
        }
        Ok(())
    }

    /// Rebases the current branch onto `onto` with the given options.
    pub fn rebase_with_options(&self, onto: &str, options: RebaseOptions) -> Result<()> {
        self.rebase_commits(onto, None, None, options)
    }

    /// Rebases onto `onto`. With `after`, only the commits after that
    /// revision move (`git rebase --onto`). With `branch`, Git checks out
    /// that branch first; otherwise the current branch is rebased.
    pub fn rebase_commits(
        &self,
        onto: &str,
        after: Option<&str>,
        branch: Option<&str>,
        options: RebaseOptions,
    ) -> Result<()> {
        let args = rebase_args(onto, after, branch, options);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        self.run_with_environment(&refs, &[("GIT_EDITOR", NO_EDITOR)])
            .map(|_| ())
    }

    /// Local branches whose history contains `revision`.
    pub fn branches_containing(&self, revision: &str) -> Result<Vec<String>> {
        let out = self.run(&[
            "for-each-ref",
            "--contains",
            revision,
            "--format=%(refname:short)",
            "refs/heads",
        ])?;
        Ok(out
            .lines()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .collect())
    }

    /// The local branch closest to `revision`, as `git name-rev` finds it.
    /// First-parent paths win over paths through merges, so this is the
    /// branch the commit was most likely made on.
    pub fn nearest_branch(&self, revision: &str) -> Result<Option<String>> {
        let out = self.run(&["name-rev", "--name-only", "--refs=refs/heads/*", revision])?;
        Ok(parse_name_rev_branch(&out))
    }

    /// Number of commits in a revision range, such as `a..b`.
    pub fn count_commits(&self, range: &str) -> Result<usize> {
        let out = self.run(&["rev-list", "--count", range])?;
        out.trim()
            .parse()
            .context("git rev-list returned an invalid count")
    }

    /// Points a branch that is not checked out at `target`.
    pub fn move_branch(&self, name: &str, target: &str) -> Result<()> {
        self.run(&["branch", "--force", name, target]).map(|_| ())
    }

    /// Returns the commits in `base_ref..HEAD`, oldest first.
    pub fn commits_between(&self, base_ref: &str) -> Result<Vec<Commit>> {
        let format = format!(
            "%H{s}%h{s}%P{s}%an{s}%ae{s}%at{s}%ct{s}%D{s}%s{s}%b{r}",
            s = FIELD_SEP,
            r = RECORD_SEP
        );
        let format_arg = format!("--pretty=format:{format}");
        let range = format!("{base_ref}..HEAD");
        let out = self.run(&["log", "--reverse", "--topo-order", &range, &format_arg])?;
        Ok(parse::parse_log(&out))
    }

    /// Returns the hashes of the commits on `HEAD` that its upstream does not have. Fails when the
    /// branch has no upstream.
    pub fn unpushed_commit_hashes(&self) -> Result<HashSet<String>> {
        let out = self.run(&["rev-list", "@{upstream}..HEAD"])?;
        Ok(out
            .lines()
            .map(str::trim)
            .filter(|hash| !hash.is_empty())
            .map(str::to_string)
            .collect())
    }

    /// Runs an interactive rebase onto `upstream` with `items` as the plan, so no editor opens.
    /// Fails when the plan is empty or has an action other than `pick`, `squash`, `fixup` or
    /// `drop`. The plan goes to a temporary folder that is deleted after the rebase.
    pub fn interactive_rebase(
        &self,
        upstream: &str,
        options: RebaseOptions,
        items: &[InteractiveRebaseItem],
    ) -> Result<()> {
        if items.is_empty() {
            bail!("the interactive rebase plan has no commits");
        }
        for item in items {
            if !matches!(item.action.as_str(), "pick" | "squash" | "fixup" | "drop") {
                bail!("the interactive rebase plan has an invalid action");
            }
        }

        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("kodem-rebase-{}-{suffix}", std::process::id()));
        std::fs::create_dir(&directory).context("failed to create the rebase directory")?;

        let result = (|| {
            let todo_path = directory.join("git-rebase-todo");
            let editor_path = directory.join("sequence-editor.sh");
            let todo = items
                .iter()
                .map(|item| {
                    format!(
                        "{} {} {}",
                        item.action,
                        item.hash,
                        item.subject.replace(['\n', '\r'], " ")
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
                + "\n";
            std::fs::write(&todo_path, todo).context("failed to write the rebase plan")?;
            std::fs::write(
                &editor_path,
                "#!/bin/sh\nset -eu\ncat \"$KODEM_REBASE_TODO\" > \"$1\"\n",
            )
            .context("failed to write the rebase editor")?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                let mut permissions = std::fs::metadata(&editor_path)?.permissions();
                permissions.set_mode(0o700);
                std::fs::set_permissions(&editor_path, permissions)?;
            }

            let args = rebase_args(upstream, None, None, options);
            let mut interactive_args = vec!["rebase".to_string(), "--interactive".to_string()];
            interactive_args.extend(args.into_iter().skip(1));
            let refs: Vec<&str> = interactive_args.iter().map(String::as_str).collect();
            let todo_value = todo_path.to_string_lossy().into_owned();
            let editor_value = shell_quoted(&editor_path);
            self.run_with_environment(
                &refs,
                &[
                    ("KODEM_REBASE_TODO", todo_value.as_str()),
                    ("GIT_SEQUENCE_EDITOR", editor_value.as_str()),
                    ("GIT_EDITOR", NO_EDITOR),
                ],
            )
            .map(|_| ())
        })();

        let _ = std::fs::remove_dir_all(&directory);
        result
    }

    /// Checks out the local branch `local_name` when it exists. Otherwise creates a local branch
    /// that tracks `remote_ref` and checks it out.
    pub fn checkout_remote(&self, remote_ref: &str, local_name: &str) -> Result<()> {
        let local_ref = format!("refs/heads/{local_name}");
        if self
            .run(&["rev-parse", "--verify", "--quiet", &local_ref])
            .is_ok()
        {
            self.checkout(local_name)
        } else {
            self.run(&["checkout", "--track", remote_ref]).map(|_| ())
        }
    }

    /// Applies a stash entry and deletes it from the stash.
    pub fn stash_pop(&self, reference: &str) -> Result<()> {
        self.run(&["stash", "pop", reference]).map(|_| ())
    }

    /// Deletes the records of worktrees whose folders no longer exist.
    pub fn prune_worktrees(&self) -> Result<()> {
        self.run(&["worktree", "prune"]).map(|_| ())
    }

    /// Removes the worktree at `path`. Git refuses when the worktree has local changes.
    pub fn remove_worktree(&self, path: &Path) -> Result<()> {
        self.run(&["worktree", "remove", &path.to_string_lossy()])
            .map(|_| ())
    }

    /// Reads the full status of the worktree at `path`.
    pub fn worktree_status(&self, path: &Path) -> Result<RepoStatus> {
        GitRepo::new(path).status()
    }

    /// Copies the uncommitted changes of the worktree at `path` into this worktree. Applies its
    /// diff against its `HEAD`, then copies the given untracked files. Fails when an untracked path
    /// leaves either worktree or already exists here.
    pub fn apply_worktree_changes(&self, path: &Path, untracked_paths: &[String]) -> Result<()> {
        let source = GitRepo::new(path);
        let patch =
            source.run_bytes(&["diff", "HEAD", "--binary", "--full-index", "--no-color"])?;
        if !patch.is_empty() {
            self.run_with_stdin(&["apply", "--whitespace=nowarn"], &patch)?;
        }

        let source_root =
            std::fs::canonicalize(path).context("failed to resolve the source worktree path")?;
        let target_root = std::fs::canonicalize(&self.path)
            .context("failed to resolve the target worktree path")?;
        for relative in untracked_paths {
            let relative_path = Path::new(relative);
            if !relative_path
                .components()
                .all(|component| matches!(component, std::path::Component::Normal(_)))
            {
                bail!("the worktree has an unsafe untracked path: {relative}");
            }
            let source_path = std::fs::canonicalize(source_root.join(relative_path))
                .with_context(|| format!("failed to resolve {relative}"))?;
            if !source_path.starts_with(&source_root) || !source_path.is_file() {
                bail!("the worktree has an unsafe untracked file: {relative}");
            }
            let target_path = target_root.join(relative_path);
            if target_path.exists() {
                bail!("the target already contains {relative}");
            }
            let Some(parent) = target_path.parent() else {
                bail!("the target path is invalid: {relative}");
            };
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create a folder for {relative}"))?;
            let resolved_parent = std::fs::canonicalize(parent)
                .with_context(|| format!("failed to resolve a folder for {relative}"))?;
            if !resolved_parent.starts_with(&target_root) {
                bail!("the target has an unsafe path: {relative}");
            }
            std::fs::copy(&source_path, &target_path)
                .with_context(|| format!("failed to copy {relative}"))?;
        }
        Ok(())
    }

    fn run_bytes(&self, args: &[&str]) -> Result<Vec<u8>> {
        let output = git_command(&self.path)
            .args(args)
            .output()
            .context("failed to spawn git")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(git_command_error(args, &stderr));
        }
        Ok(output.stdout)
    }

    fn run_with_stdin_bytes(&self, args: &[&str], input: &[u8]) -> Result<Vec<u8>> {
        let mut child = git_command(&self.path)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("failed to spawn git")?;
        child
            .stdin
            .take()
            .context("failed to open git input")?
            .write_all(input)
            .context("failed to write git input")?;
        let output = child.wait_with_output().context("failed to wait for git")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(git_command_error(args, &stderr));
        }
        Ok(output.stdout)
    }

    fn run_with_stdin(&self, args: &[&str], input: &[u8]) -> Result<()> {
        self.run_with_stdin_bytes(args, input).map(|_| ())
    }

    /// Stops tracking a pattern with Git LFS.
    pub fn lfs_untrack(&self, pattern: &str) -> Result<()> {
        self.run(&["lfs", "untrack", pattern]).map(|_| ())
    }

    /// Tracks a pattern with Git LFS.
    pub fn lfs_track(&self, pattern: &str) -> Result<()> {
        self.run(&["lfs", "track", pattern]).map(|_| ())
    }

    /// Downloads the LFS objects of all refs without changing the working tree.
    pub fn lfs_fetch(&self) -> Result<()> {
        self.run(&["lfs", "fetch", "--all"]).map(|_| ())
    }

    /// Downloads the LFS objects of the checkout and replaces their pointer files in the working
    /// tree.
    pub fn lfs_pull(&self) -> Result<()> {
        self.run(&["lfs", "pull"]).map(|_| ())
    }

    /// Uploads the LFS objects of all refs to the remote of the current branch's upstream, or
    /// to `origin` or the only remote when the branch has no upstream.
    pub fn lfs_push(&self) -> Result<()> {
        let remote = self.lfs_push_remote()?;
        self.run(&["lfs", "push", "--all", &remote]).map(|_| ())
    }

    // Returns the remote that `lfs_push` uploads to; `git lfs push` needs one by name.
    fn lfs_push_remote(&self) -> Result<String> {
        let remotes = self.run(&["remote"])?;
        let remotes: Vec<&str> = remotes
            .lines()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .collect();
        if let Ok(upstream) = self.run(&[
            "rev-parse",
            "--abbrev-ref",
            "--symbolic-full-name",
            "@{upstream}",
        ]) && let Some(remote) = remotes
            .iter()
            .find(|remote| upstream.trim().starts_with(&format!("{remote}/")))
        {
            return Ok((*remote).to_string());
        }
        match remotes.as_slice() {
            [] => bail!("the repository has no remote to push LFS objects to"),
            [only] => Ok((*only).to_string()),
            _ if remotes.contains(&"origin") => Ok("origin".to_string()),
            _ => bail!("the current branch has no upstream; set one to push LFS objects"),
        }
    }

    /// Deletes local LFS objects that Git LFS no longer needs.
    pub fn lfs_prune(&self) -> Result<()> {
        self.run(&["lfs", "prune"]).map(|_| ())
    }

    /// Changes the URL of a remote.
    pub fn set_remote_url(&self, name: &str, url: &str) -> Result<()> {
        self.run(&["remote", "set-url", name, url]).map(|_| ())
    }
}

fn fetch_args(options: FetchOptions) -> Vec<&'static str> {
    let mut args = vec!["fetch"];
    if options.all_remotes {
        args.push("--all");
    }
    if options.prune {
        args.push("--prune");
    }
    if options.recurse_submodules {
        args.push("--recurse-submodules=on-demand");
    }
    args
}

fn pull_args(options: PullOptions) -> Vec<&'static str> {
    let mut args = vec!["pull"];
    if options.rebase {
        args.push("--rebase");
    }
    if options.ff_only {
        args.push("--ff-only");
    }
    if options.no_commit {
        args.push("--no-commit");
    }
    if options.auto_stash {
        args.push("--autostash");
    }
    if options.recurse_submodules {
        args.push("--recurse-submodules=on-demand");
    }
    args
}

fn push_args(options: PushOptions, remote: &str, branch: &str, has_upstream: bool) -> Vec<String> {
    let mut args = vec!["push".to_string()];
    if options.recurse_submodules {
        args.push("--recurse-submodules=on-demand".to_string());
    }
    if options.force_with_lease {
        args.push("--force-with-lease".to_string());
    } else if options.force {
        args.push("--force".to_string());
    }
    if options.set_upstream || !has_upstream {
        args.extend([
            "--set-upstream".to_string(),
            remote.to_string(),
            "HEAD".to_string(),
        ]);
    } else {
        args.extend([remote.to_string(), branch.to_string()]);
    }
    if options.tags {
        args.push("--tags".to_string());
    }
    args
}

fn merge_args(branch: &str, options: &MergeOptions) -> Vec<String> {
    let mut args = vec!["merge".to_string()];
    if options.no_ff {
        args.push("--no-ff".to_string());
    } else if options.squash {
        args.push("--squash".to_string());
    }
    if options.no_commit {
        args.push("--no-commit".to_string());
    }
    if let Some(message) = options
        .message
        .as_deref()
        .filter(|message| !message.trim().is_empty())
    {
        args.extend(["--message".to_string(), message.to_string()]);
    } else {
        args.push("--no-edit".to_string());
    }
    args.push(branch.to_string());
    args
}

fn rebase_args(
    onto: &str,
    after: Option<&str>,
    branch: Option<&str>,
    options: RebaseOptions,
) -> Vec<String> {
    let mut args = vec!["rebase".to_string()];
    if options.autostash {
        args.push("--autostash".to_string());
    }
    match after {
        Some(after) => args.extend(["--onto".to_string(), onto.to_string(), after.to_string()]),
        None => args.push(onto.to_string()),
    }
    if let Some(branch) = branch {
        args.push(branch.to_string());
    }
    args
}

/// Reads the branch from `git name-rev` output such as `feature~2^2`.
fn parse_name_rev_branch(output: &str) -> Option<String> {
    let name = output.trim();
    let branch = name.split(['~', '^']).next().unwrap_or_default();
    (!branch.is_empty() && branch != "undefined").then(|| branch.to_string())
}
