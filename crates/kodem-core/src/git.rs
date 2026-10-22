//! Byte-preserving Git capture and guarded whole-file staging.
use crate::{model::*, platform};
use anyhow::{Context, Result, bail, ensure};
use sha2::{Digest, Sha256};
use std::{
    ffi::{OsStr, OsString},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
const MAX_FILE: u64 = 2 * 1024 * 1024;
const MAX_FOLDER_FILES: usize = 20_000;
pub mod checkpoint;
mod repositories;
mod review;
pub use repositories::{repositories, repository_baselines};
pub use review::{PreparedReview, discard_review, prepare_review, review_targets};

/// Returns the PATH that Kodem gives Git, GitHub CLI and Git LFS. A desktop session's PATH can
/// omit installed tools, and Git's filters and credential helpers need them too, even when Git
/// itself is already discoverable. On Windows it also holds the folders where the installers of
/// Git, Git LFS and GitHub CLI put them, which a running app's PATH does not have yet.
pub fn search_path() -> Result<OsString> {
    let mut paths: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    let mut fallbacks = platform::tool_directories();
    #[cfg(unix)]
    fallbacks.extend(["/usr/bin", "/bin", "/usr/sbin", "/sbin"].map(PathBuf::from));
    #[cfg(windows)]
    if let Some(programs) = std::env::var_os("ProgramFiles").map(PathBuf::from) {
        fallbacks.extend([
            programs.join("Git").join("cmd"),
            programs.join("Git LFS"),
            programs.join("GitHub CLI"),
        ]);
    }
    for path in fallbacks {
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    std::env::join_paths(paths).context("Build Git PATH")
}

fn git_process(cwd: &Path) -> Result<Command> {
    let mut command = Command::new("git");
    platform::hide_console(&mut command);
    // Kodem runs Git at the root of a checkout. A project folder without a repository of its own
    // must not use the repository of a folder above it.
    if let Some(parent) = cwd
        .canonicalize()
        .map(crate::platform::plain_path)
        .ok()
        .as_deref()
        .and_then(Path::parent)
    {
        command.env("GIT_CEILING_DIRECTORIES", parent);
    }
    command
        .current_dir(cwd)
        .env("PATH", search_path()?)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .args(["--no-pager", "-c", "core.quotepath=false"])
        .stdin(Stdio::null());
    Ok(command)
}

/// Executes a Git machine-format query without shell interpretation or external diff drivers.
pub fn command(cwd: &Path, args: &[&OsStr]) -> Result<Vec<u8>> {
    let out = git_process(cwd)?.args(args).output()?;
    ensure!(
        out.status.success(),
        "Git: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    );
    Ok(out.stdout)
}
fn query(cwd: &Path, args: &[&str]) -> Result<Vec<u8>> {
    command(cwd, &args.iter().map(OsStr::new).collect::<Vec<_>>())
}
/// Tells whether a folder is the root of a Git checkout.
///
/// Git never looks above the folder that it runs in, so a folder without its own `.git` has no
/// repository, even inside the checkout of another one.
pub fn is_repository(path: &Path) -> bool {
    path.join(".git").exists()
}
/// Creates an empty repository in a folder, without a commit.
pub fn initialize(path: &Path) -> Result<()> {
    query(path, &["init", "--quiet"])?;
    Ok(())
}
/// Resolves and validates a local repository root without changing branches.
pub fn repository(path: &Path) -> Result<PathBuf> {
    let canonical = path.canonicalize().map(crate::platform::plain_path)?;
    ensure!(
        query(&canonical, &["rev-parse", "--is-bare-repository"])? == b"false\n",
        "Choose a working-tree repository"
    );
    let root = query(&canonical, &["rev-parse", "--show-toplevel"])?;
    Ok(PathBuf::from(platform::os_string(
        root.strip_suffix(b"\n").unwrap_or(&root),
    )))
}
/// Returns the current commit, including the unborn-repository case.
pub fn head(cwd: &Path) -> Option<String> {
    query(cwd, &["rev-parse", "--verify", "HEAD"])
        .ok()
        .map(|v| String::from_utf8_lossy(&v).trim().to_owned())
}
/// Returns branch metadata without checking out the branch. A folder without Git has no branch,
/// so its name is empty.
pub fn branch(cwd: &Path) -> String {
    if !is_repository(cwd) {
        return String::new();
    }
    query(cwd, &["symbolic-ref", "--short", "HEAD"])
        .ok()
        .map(|v| String::from_utf8_lossy(&v).trim().to_owned())
        .unwrap_or_else(|| "detached HEAD".into())
}
fn resolve_commit(cwd: &Path, reference: &str) -> Option<String> {
    query(
        cwd,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{reference}^{{commit}}"),
        ],
    )
    .ok()
    .map(|bytes| String::from_utf8_lossy(&bytes).trim().to_owned())
}
fn default_branch(cwd: &Path) -> Option<(String, Option<String>)> {
    let remote = query(cwd, &["symbolic-ref", "refs/remotes/origin/HEAD"])
        .ok()
        .map(|bytes| String::from_utf8_lossy(&bytes).trim().to_owned());
    let name = remote
        .as_deref()
        .and_then(|reference| reference.strip_prefix("refs/remotes/origin/"))
        .map(str::to_owned)
        .or_else(|| {
            ["main", "master"]
                .into_iter()
                .find(|name| resolve_commit(cwd, &format!("refs/heads/{name}")).is_some())
                .map(str::to_owned)
        })?;
    Some((name, remote))
}
fn branch_base(cwd: &Path, observed_head: Option<&str>) -> Option<String> {
    let (default, remote) = default_branch(cwd)?;
    let current = branch(cwd);
    if current == default || current == "detached HEAD" {
        return None;
    }
    let target = resolve_commit(cwd, &format!("refs/heads/{default}"))
        .or_else(|| remote.and_then(|reference| resolve_commit(cwd, &reference)))?;
    query(cwd, &["merge-base", observed_head?, &target])
        .ok()
        .map(|bytes| String::from_utf8_lossy(&bytes).trim().to_owned())
}
/// Detects committed task work merged into a locally known default branch without fetching.
/// A task on the default branch counts as merged once the files that its turns changed are
/// committed there. Unchanged work and dirty checkouts are never considered merged.
pub fn branch_merged(project: &Path, task: &Task) -> bool {
    let Some(baseline) = &task.baseline else {
        return false;
    };
    let Some((default, remote)) = default_branch(project) else {
        return false;
    };
    if task.branch == default {
        return work_committed(task, baseline, &default);
    }
    if task.branch == "detached HEAD" {
        return false;
    }
    let Some(tip) = resolve_commit(project, &format!("refs/heads/{}", task.branch)) else {
        return false;
    };
    if &tip == baseline
        || query(project, &["merge-base", "--is-ancestor", baseline, &tip]).is_err()
        || !query(
            &task.cwd,
            &["status", "--porcelain=v1", "-z", "--untracked-files=normal"],
        )
        .is_ok_and(|status| status.is_empty())
    {
        return false;
    }
    [Some(format!("refs/heads/{default}")), remote]
        .into_iter()
        .flatten()
        .filter_map(|reference| resolve_commit(project, &reference))
        .any(|target| query(project, &["merge-base", "--is-ancestor", &tip, &target]).is_ok())
}
// A task that works on the default branch has no branch to merge. Its work is merged once every
// file that its turns left changed is committed after the baseline, and no file that they
// touched has uncommitted changes. Committed work stays merged until the next turn, so that a
// later edit of the same files by the user or another task does not reopen it.
fn work_committed(task: &Task, baseline: &str, default: &str) -> bool {
    if task.branch_merged {
        return true;
    }
    let Some(observed) = head(&task.cwd) else {
        return false;
    };
    if observed == baseline || branch(&task.cwd) != default {
        return false;
    }
    let Ok(changes) = checkpoint::turn_changes(&task.cwd, &task.id) else {
        return false;
    };
    if changes.kept.is_empty() {
        return false;
    }
    let Ok(committed) = query(
        &task.cwd,
        &[
            "diff",
            "--name-only",
            "-z",
            "--no-renames",
            baseline,
            &observed,
            "--",
        ],
    ) else {
        return false;
    };
    let committed: Vec<&[u8]> = committed
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .collect();
    if !changes
        .kept
        .iter()
        .all(|path| committed.contains(&path.as_slice()))
    {
        return false;
    }
    status(&task.cwd).is_ok_and(|files| {
        !files.iter().any(|(path, old, _)| {
            changes.touched.contains(path)
                || old
                    .as_ref()
                    .is_some_and(|old| changes.touched.contains(old))
        })
    })
}
/// Creates an isolated task worktree on a new branch, preserving the original checkout.
///
/// The branch starts at `start` when it names a commit, otherwise at the project's current commit.
pub fn worktree(project: &Path, root: &Path, id: &str, start: Option<&str>) -> Result<PathBuf> {
    ensure!(
        is_repository(project),
        "Initialize Git in this project before making a task worktree, or choose the current folder"
    );
    ensure!(
        head(project).is_some(),
        "Create an initial commit before making a task worktree, or choose the current checkout"
    );
    let start = match start {
        Some(start) => resolve_commit(project, start)
            .context("The commit to start the worktree from is not in this repository")?,
        None => "HEAD".to_owned(),
    };
    fs::create_dir_all(root)?;
    let path = root.join(id);
    let b = format!("kodem/{}", &id[..8]);
    command(
        project,
        &[
            OsStr::new("worktree"),
            OsStr::new("add"),
            OsStr::new("-b"),
            OsStr::new(&b),
            path.as_os_str(),
            OsStr::new(&start),
        ],
    )?;
    Ok(path)
}
/// Returns a SHA-256 content identity distinct from Git object IDs.
pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn index_path(cwd: &Path) -> Result<PathBuf> {
    let bytes = query(cwd, &["rev-parse", "--git-path", "index"])?;
    let path = PathBuf::from(platform::os_string(
        bytes.strip_suffix(b"\n").unwrap_or(&bytes),
    ));
    Ok(if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    })
}
fn index_digest(cwd: &Path) -> Result<String> {
    match fs::read(index_path(cwd)?) {
        Ok(b) => Ok(digest(&b)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(digest(b"unborn-index")),
        Err(e) => Err(e.into()),
    }
}
fn path_digest(cwd: &Path, path: &[u8]) -> Result<String> {
    let path = cwd.join(platform::os_string(path));
    match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(digest(b"missing")),
        Err(e) => Err(e.into()),
        Ok(meta) if meta.file_type().is_symlink() => Ok(digest(&platform::os_bytes(
            fs::read_link(path)?.as_os_str(),
        ))),
        Ok(meta) if meta.is_dir() => {
            let mut state = query(&path, &["status", "--porcelain=v1", "-z"])?;
            state.extend_from_slice(head(&path).unwrap_or_default().as_bytes());
            Ok(digest(&state))
        }
        Ok(meta) => {
            let mut hash = Sha256::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                hash.update(meta.permissions().mode().to_le_bytes());
            }
            #[cfg(not(unix))]
            hash.update([u8::from(meta.permissions().readonly())]);
            let mut file = fs::File::open(path)?;
            let mut buf = [0; 65536];
            loop {
                let n = file.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                hash.update(&buf[..n]);
            }
            Ok(format!("{:x}", hash.finalize()))
        }
    }
}
type StatusEntry = (Vec<u8>, Option<Vec<u8>>, String);
fn status(cwd: &Path) -> Result<Vec<StatusEntry>> {
    let bytes = query(
        cwd,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?;
    let mut parts = bytes.split(|b| *b == 0);
    let mut files = Vec::new();
    while let Some(p) = parts.next() {
        if p.is_empty() {
            continue;
        }
        ensure!(p.len() > 3, "Malformed Git status");
        let kind = String::from_utf8_lossy(&p[..2]).to_string();
        let old = if kind.contains(['R', 'C']) {
            Some(parts.next().context("Missing rename path")?.to_vec())
        } else {
            None
        };
        files.push((p[3..].to_vec(), old, kind));
    }
    Ok(files)
}
/// Lists the paths that differ from `base`, each with its one-letter Git status.
fn changed_since(cwd: &Path, base: &str, renames: bool) -> Result<Vec<(Vec<u8>, String)>> {
    let mut args = vec!["diff", "--name-status", "-z"];
    if !renames {
        args.push("--no-renames");
    }
    args.extend([base, "--"]);
    let bytes = query(cwd, &args)?;
    let mut parts = bytes.split(|b| *b == 0).filter(|p| !p.is_empty());
    let mut files = Vec::new();
    while let Some(record) = parts.next() {
        // A rename or copy carries a similarity score and lists its source path first.
        let kind = String::from_utf8_lossy(&record[..1]).to_string();
        if kind.contains(['R', 'C']) {
            parts.next().context("Missing rename path")?;
        }
        let path = parts.next().context("Missing changed path")?;
        files.push((path.to_vec(), kind));
    }
    Ok(files)
}
fn patch(
    cwd: &Path,
    path: &[u8],
    old: Option<&[u8]>,
    comparison: &Comparison,
    base: Option<&str>,
) -> Result<Vec<u8>> {
    let mut args = vec![
        OsString::from("diff"),
        "--no-ext-diff".into(),
        "--no-textconv".into(),
        "--no-color".into(),
        "--unified=5".into(),
        "--find-renames".into(),
    ];
    match comparison {
        Comparison::Staged => {
            args.push("--cached".into());
        }
        Comparison::Unstaged => {}
        Comparison::WorkingTree => {
            if let Some(h) = base {
                args.push(h.into());
            }
        }
        Comparison::Baseline => {
            args.push(base.context("This task has no baseline commit")?.into());
        }
        Comparison::Branch => {
            if let Some(base) = base {
                args.push(base.into());
            }
        }
    }
    args.push("--".into());
    args.push(platform::os_string(path));
    if let Some(old) = old {
        args.push(platform::os_string(old));
    }
    command(
        cwd,
        &args.iter().map(OsString::as_os_str).collect::<Vec<_>>(),
    )
}
fn parse_patch(bytes: &[u8]) -> (Vec<DiffLine>, Option<String>) {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return (
            Vec::new(),
            Some("Non-UTF8 diff. Use the external editor to inspect its bytes.".into()),
        );
    };
    let mut lines = Vec::new();
    let (mut old, mut new) = (0, 0);
    let mut in_hunk = false;
    let mut notice = None;
    for line in text.split_terminator('\n') {
        if line.starts_with("@@ ") {
            let parts: Vec<_> = line.split_whitespace().collect();
            old = parts
                .get(1)
                .and_then(|s| s.trim_start_matches('-').split(',').next()?.parse().ok())
                .unwrap_or(0);
            new = parts
                .get(2)
                .and_then(|s| s.trim_start_matches('+').split(',').next()?.parse().ok())
                .unwrap_or(0);
            in_hunk = true;
            lines.push(DiffLine {
                old: None,
                new: None,
                kind: LineKind::Header,
                text: line.into(),
            });
            continue;
        }
        if line.starts_with("diff --git ") {
            in_hunk = false;
            continue;
        }
        if line.starts_with("Binary files") || line.starts_with("GIT binary patch") {
            notice = Some("Binary content changed".into());
        }
        if !in_hunk {
            continue;
        }
        let (kind, o, n) = match line.as_bytes().first() {
            Some(b'+') => {
                let n = new;
                new += 1;
                (LineKind::Added, None, Some(n))
            }
            Some(b'-') => {
                let o = old;
                old += 1;
                (LineKind::Removed, Some(o), None)
            }
            Some(b' ') => {
                let (o, n) = (old, new);
                old += 1;
                new += 1;
                (LineKind::Context, Some(o), Some(n))
            }
            _ => {
                lines.push(DiffLine {
                    old: None,
                    new: None,
                    kind: LineKind::Header,
                    text: line.into(),
                });
                continue;
            }
        };
        lines.push(DiffLine {
            old: o,
            new: n,
            kind,
            text: line[1..].into(),
        });
    }
    if lines.is_empty() && notice.is_none() {
        notice =
            Some("Metadata-only change, rename, or unchanged content in this comparison".into())
    }
    (lines, notice)
}
/// Captures each related checkout independently, retrying boundedly when its files change.
pub fn capture(task: &Task, comparison: Comparison) -> Result<DiffSnapshot> {
    ensure!(
        is_repository(&task.cwd),
        "This folder has no Git repository, so its changes are not tracked"
    );
    let roots = repositories(&task.cwd)?;
    let mut snapshot = capture_repository(task, comparison.clone())?;
    for root in roots.iter().skip(1) {
        let mut linked = task.clone();
        linked.cwd = root.clone();
        linked.baseline = task
            .repository_baselines
            .iter()
            .find(|base| &base.root == root)
            .and_then(|base| base.head.clone());
        let missing_baseline = comparison == Comparison::Baseline && linked.baseline.is_none();
        let mut captured = capture_repository(
            &linked,
            if missing_baseline {
                Comparison::WorkingTree
            } else {
                comparison.clone()
            },
        )
        .with_context(|| format!("Capture repository {}", root.display()))?;
        if missing_baseline {
            captured.repositories[0].notice = Some(
                "No task baseline for this repository. Showing current uncommitted changes.".into(),
            );
        }
        snapshot.files.extend(captured.files);
        snapshot.repositories.extend(captured.repositories);
    }
    for repo in &mut snapshot.repositories {
        repo.label = repositories::label(&repo.root, &roots[0]);
    }
    Ok(snapshot)
}

fn capture_repository(task: &Task, comparison: Comparison) -> Result<DiffSnapshot> {
    let root = task.cwd.canonicalize().map(crate::platform::plain_path)?;
    for _ in 0..3 {
        let before = index_digest(&task.cwd)?;
        let observed_head = head(&task.cwd);
        let branch_base = branch_base(&task.cwd, observed_head.as_deref());
        let comparison_base = match comparison {
            Comparison::Baseline => task.baseline.clone(),
            Comparison::Branch => branch_base.clone().or_else(|| observed_head.clone()),
            _ => observed_head.clone(),
        };
        let files = status(&task.cwd)?;
        let status_before = serde_json::to_vec(&files)?;
        let mut entries = Vec::new();
        // Task and branch comparisons include committed paths absent from current status.
        let mut paths = files.clone();
        if matches!(comparison, Comparison::Baseline | Comparison::Branch)
            && (comparison == Comparison::Baseline || comparison_base.is_some())
        {
            let base = comparison_base
                .as_deref()
                .context("No task baseline exists yet")?;
            for (path, kind) in changed_since(&task.cwd, base, comparison != Comparison::Branch)? {
                if !paths.iter().any(|f| f.0 == path) {
                    paths.push((path, None, kind));
                }
            }
        }
        let mut stable = true;
        for (path, old, kind) in paths {
            // Branch paths list both sides of renames, so each patch owns one path.
            let old = if comparison == Comparison::Branch {
                None
            } else {
                old
            };
            if comparison == Comparison::Staged && (kind.starts_with(' ') || kind == "??") {
                continue;
            }
            if comparison == Comparison::Unstaged && (kind.ends_with(' ') && kind != "??") {
                continue;
            }
            let observed = path_digest(&task.cwd, &path)?;
            let full = task.cwd.join(platform::os_string(&path));
            let meta = fs::symlink_metadata(&full).ok();
            let (lines, notice) = if kind.contains('U') || kind == "AA" || kind == "DD" {
                (
                    Vec::new(),
                    Some("Unmerged file. Resolve base/ours/theirs before staging.".into()),
                )
            } else if meta.as_ref().is_some_and(|m| m.is_dir()) {
                (
                    Vec::new(),
                    Some("Repository reference or directory change. Nested files appear under their own repository.".into()),
                )
            } else if meta.as_ref().is_some_and(|m| m.len() > MAX_FILE) {
                (
                    Vec::new(),
                    Some("File exceeds the 2 MiB inline review limit.".into()),
                )
            } else if kind == "??"
                || (matches!(comparison, Comparison::WorkingTree | Comparison::Branch)
                    && observed_head.is_none()
                    && meta.is_some())
            {
                if meta.as_ref().is_some_and(|m| m.file_type().is_symlink()) {
                    (
                        Vec::new(),
                        Some(format!("Symlink → {}", fs::read_link(&full)?.display())),
                    )
                } else {
                    let bytes = fs::read(&full)?;
                    match String::from_utf8(bytes) {
                        Ok(text) if !text.contains('\0') => (
                            text.split_inclusive('\n')
                                .enumerate()
                                .map(|(i, s)| DiffLine {
                                    old: None,
                                    new: Some(i as u32 + 1),
                                    kind: LineKind::Added,
                                    text: s.strip_suffix('\n').unwrap_or(s).into(),
                                })
                                .collect(),
                            None,
                        ),
                        _ => (Vec::new(), Some("Untracked binary or non-UTF8 file".into())),
                    }
                }
            } else {
                let bytes = patch(
                    &task.cwd,
                    &path,
                    old.as_deref(),
                    &comparison,
                    comparison_base.as_deref(),
                )?;
                if comparison == Comparison::Branch && bytes.is_empty() {
                    stable &= observed == path_digest(&task.cwd, &path)?;
                    continue;
                }
                if bytes.len() > MAX_FILE as usize {
                    (
                        Vec::new(),
                        Some("Patch exceeds the 2 MiB inline review limit.".into()),
                    )
                } else {
                    parse_patch(&bytes)
                }
            };
            stable &= observed == path_digest(&task.cwd, &path)?;
            entries.push(FileDelta {
                repository: Some(root.clone()),
                label: platform::os_string(&path).to_string_lossy().into(),
                path,
                old_path: old,
                status: kind,
                lines,
                notice,
                digest: observed,
            });
        }
        if stable
            && observed_head == head(&task.cwd)
            && before == index_digest(&task.cwd)?
            && status_before == serde_json::to_vec(&status(&task.cwd)?)?
        {
            return Ok(DiffSnapshot {
                id: String::new(),
                task_id: task.id.clone(),
                comparison: comparison.clone(),
                baseline: comparison_base.clone(),
                captured: now(),
                index_digest: before.clone(),
                files: entries,
                repositories: vec![RepositorySnapshot {
                    root: root.clone(),
                    label: String::new(),
                    baseline: comparison_base.clone(),
                    branch_base: branch_base.clone(),
                    index_digest: before.clone(),
                    notice: (comparison == Comparison::Branch && branch_base.is_none()).then(
                        || {
                            "No branch comparison available. Showing current uncommitted changes."
                                .into()
                        },
                    ),
                }],
            });
        }
    }
    bail!("The repository changed during capture. Pause external writes and refresh.")
}
/// Binds a comment to source coordinates and exact immutable captured text.
pub fn feedback(
    snapshot: &DiffSnapshot,
    file: usize,
    side: Side,
    start: u32,
    end: u32,
    comment: String,
) -> Result<Feedback> {
    ensure!(
        start > 0 && end >= start,
        "Choose a nonempty source-line range"
    );
    ensure!(
        !comment.trim().is_empty() && comment.len() <= 64 * 1024,
        "Write a review comment under 64 KiB"
    );
    let file = snapshot
        .files
        .get(file)
        .context("File is not in this snapshot")?;
    ensure!(end - start <= 10000, "Choose at most 10001 source lines");
    let mut excerpt = Vec::new();
    for number in start..=end {
        let line = file
            .lines
            .iter()
            .find(|l| match side {
                Side::Old => l.old == Some(number),
                Side::New => l.new == Some(number),
            })
            .context("Range crosses hidden context or does not belong to this side")?;
        excerpt.push(line.text.as_str());
    }
    ensure!(
        excerpt.iter().map(|line| line.len() + 1).sum::<usize>() <= 512 * 1024,
        "Selected context exceeds 512 KiB"
    );
    Ok(Feedback {
        repository: file.repository.clone(),
        old_range: if side == Side::Old {
            Some((start, end))
        } else {
            None
        },
        new_range: if side == Side::New {
            Some((start, end))
        } else {
            None
        },
        first_row: None,
        last_row: None,
        id: new_id(),
        snapshot_id: snapshot.id.clone(),
        path: file.path.clone(),
        label: feedback_label(snapshot, file),
        side,
        start,
        end,
        excerpt: excerpt.join("\n"),
        comment,
        digest: file.digest.clone(),
    })
}
/// Captures an inclusive displayed diff-row range, preserving both removed and added lines.
pub fn feedback_rows(
    snapshot: &DiffSnapshot,
    file: usize,
    first_row: usize,
    last_row: usize,
    comment: String,
) -> Result<Feedback> {
    ensure!(first_row <= last_row, "Choose a nonempty diff range");
    ensure!(
        !comment.trim().is_empty() && comment.len() <= 64 * 1024,
        "Write a review comment under 64 KiB"
    );
    let file = snapshot
        .files
        .get(file)
        .context("File is not in this snapshot")?;
    let rows = file
        .lines
        .get(first_row..=last_row)
        .context("Selection is outside this snapshot")?;
    ensure!(
        rows.iter()
            .any(|line| line.old.is_some() || line.new.is_some()),
        "Choose source lines"
    );
    let side = if rows.iter().any(|line| line.new.is_some()) {
        Side::New
    } else {
        Side::Old
    };
    let numbers: Vec<_> = rows
        .iter()
        .filter_map(|line| match side {
            Side::New => line.new,
            Side::Old => line.old,
        })
        .collect();
    let excerpt = rows
        .iter()
        .map(|line| {
            let marker = match line.kind {
                LineKind::Added => "+",
                LineKind::Removed => "-",
                LineKind::Context => " ",
                LineKind::Header => "",
            };
            format!("{marker}{}", line.text)
        })
        .collect::<Vec<_>>()
        .join("\n");
    ensure!(
        excerpt.len() <= 512 * 1024,
        "Selected context exceeds 512 KiB"
    );
    Ok(Feedback {
        repository: file.repository.clone(),
        old_range: rows
            .iter()
            .filter_map(|l| l.old)
            .next()
            .zip(rows.iter().rev().find_map(|l| l.old)),
        new_range: rows
            .iter()
            .filter_map(|l| l.new)
            .next()
            .zip(rows.iter().rev().find_map(|l| l.new)),
        first_row: Some(first_row),
        last_row: Some(last_row),
        id: new_id(),
        snapshot_id: snapshot.id.clone(),
        path: file.path.clone(),
        label: feedback_label(snapshot, file),
        side,
        start: *numbers.first().context("No source lines")?,
        end: *numbers.last().context("No source lines")?,
        excerpt,
        comment,
        digest: file.digest.clone(),
    })
}
fn feedback_label(snapshot: &DiffSnapshot, file: &FileDelta) -> String {
    snapshot
        .repositories
        .iter()
        .find(|repo| Some(&repo.root) == file.repository.as_ref())
        .map_or_else(
            || file.label.clone(),
            |repo| format!("{}: {}", repo.label, file.label),
        )
}
/// Stages one captured whole file after validating both content and index identity.
pub fn stage(task: &Task, snapshot: &DiffSnapshot, file: usize) -> Result<()> {
    ensure!(
        snapshot.task_id == task.id,
        "Snapshot belongs to another task"
    );
    let f = snapshot.files.get(file).context("Unknown file")?;
    let cwd = f.repository.as_deref().unwrap_or(&task.cwd);
    let expected_index = if f.repository.is_some() {
        ensure!(
            repositories(&task.cwd)?.iter().any(|root| root == cwd),
            "Repository is no longer linked to this task. Refresh before staging."
        );
        &snapshot
            .repositories
            .iter()
            .find(|repo| repo.root == cwd)
            .context("Snapshot is missing the owning repository")?
            .index_digest
    } else {
        &snapshot.index_digest
    };
    ensure!(
        !f.status.contains('U') && f.status != "AA" && f.status != "DD",
        "Resolve this conflict before staging"
    );
    ensure!(
        *expected_index == index_digest(cwd)? && f.digest == path_digest(cwd, &f.path)?,
        "Review is stale. Refresh before staging."
    );
    let index = index_path(cwd)?;
    let lock = index.with_file_name("index.lock");
    let mut guard =
        platform::create_private(&lock).context("Git index is locked by another operation")?;
    let temp = index.with_file_name(format!("kodem-index-{}", new_id()));
    let result = (|| -> Result<()> {
        ensure!(
            *expected_index == index_digest(cwd)?,
            "Index changed before staging"
        );
        if index.exists() {
            fs::copy(&index, &temp)?;
        }
        let mut cmd = git_process(cwd)?;
        cmd.env("GIT_INDEX_FILE", &temp)
            .args(["add", "-A", "--"])
            .arg(platform::os_string(&f.path));
        if let Some(old) = &f.old_path {
            cmd.arg(platform::os_string(old));
        }
        let out = cmd.output()?;
        ensure!(
            out.status.success(),
            "Git add: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        ensure!(
            f.digest == path_digest(cwd, &f.path)?,
            "File changed while staging; index was left unchanged"
        );
        guard.write_all(&fs::read(&temp)?)?;
        guard.sync_all()?;
        fs::rename(&lock, &index)?;
        Ok(())
    })();
    let _ = fs::remove_file(&temp);
    let _ = fs::remove_file(&lock);
    result
}
/// Reads both sides of one captured file for editing: the text that the snapshot's comparison
/// compares it with, and its working-tree text with the digest that a save must still find.
pub fn editable_file(task: &Task, snapshot: &DiffSnapshot, file: usize) -> Result<EditableFile> {
    ensure!(
        snapshot.task_id == task.id,
        "Snapshot belongs to another task"
    );
    let f = snapshot.files.get(file).context("Unknown file")?;
    ensure!(
        !f.status.contains('U') && f.status != "AA" && f.status != "DD",
        "Resolve this conflict before editing"
    );
    let cwd = captured_checkout(task, snapshot, f.repository.as_deref())?;
    let base = snapshot
        .repositories
        .iter()
        .find(|repo| Some(&repo.root) == f.repository.as_ref())
        .map_or(snapshot.baseline.as_deref(), |repo| {
            repo.baseline.as_deref()
        });
    // An empty revision names the index, which unstaged changes are compared with.
    let revision = match snapshot.comparison {
        Comparison::Staged => bail!("Staged changes are in the index; edit the working tree"),
        Comparison::Unstaged => Some(""),
        Comparison::WorkingTree | Comparison::Baseline | Comparison::Branch => base,
    };
    let mut before = Vec::new();
    if let Some(revision) = revision {
        let old_path = f
            .old_path
            .as_deref()
            .filter(|_| snapshot.comparison != Comparison::Branch);
        for path in std::iter::once(f.path.as_slice()).chain(old_path) {
            if let Some(bytes) = blob(&cwd, revision, path)? {
                before = bytes;
                break;
            }
        }
    }
    ensure!(
        before.len() as u64 <= MAX_FILE,
        "The compared file exceeds the 2 MiB editing limit"
    );
    let before = String::from_utf8(before)
        .ok()
        .filter(|text| !text.contains('\0'))
        .context("The compared file is binary or not UTF-8")?;
    let bytes = fs::read(working_file(&cwd, &f.path)?)?;
    let digest = digest(&bytes);
    let after = String::from_utf8(bytes)
        .ok()
        .filter(|text| !text.contains('\0'))
        .context("Binary or non-UTF8 files cannot be edited here")?;
    Ok(EditableFile {
        before,
        after,
        digest,
    })
}
/// Writes `text` over a working-tree file of a task, in a repository that `snapshot` captured.
/// The file must still hold the content with the digest `expected`, so that a save never
/// overwrites a change made after editing began.
pub fn save_file(
    task: &Task,
    snapshot: &DiffSnapshot,
    repository: Option<&Path>,
    path: &[u8],
    expected: &str,
    text: &str,
) -> Result<()> {
    ensure!(
        text.len() as u64 <= MAX_FILE,
        "File exceeds the 2 MiB editing limit"
    );
    let cwd = captured_checkout(task, snapshot, repository)?;
    let full = working_file(&cwd, path)?;
    ensure!(
        digest(&fs::read(&full)?) == expected,
        "The file changed on disk after editing began. Revert and edit it again to keep the newer file."
    );
    fs::write(&full, text)?;
    Ok(())
}
// Returns the checkout of a task file: a repository that the task's capture found, or the
// task's own checkout for snapshots without one. The capture already searched the linked
// repositories, which takes several scans of the working tree.
fn captured_checkout(
    task: &Task,
    snapshot: &DiffSnapshot,
    repository: Option<&Path>,
) -> Result<PathBuf> {
    ensure!(
        snapshot.task_id == task.id,
        "Snapshot belongs to another task"
    );
    let Some(root) = repository else {
        return Ok(task.cwd.clone());
    };
    ensure!(
        snapshot.repositories.iter().any(|repo| repo.root == root),
        "Repository is not part of this capture. Refresh before editing."
    );
    Ok(root.to_owned())
}
// Resolves a regular file of a checkout from its repository-relative path. The path may not
// leave the checkout, and neither the file nor a folder above it may be a link out of it.
fn working_file(cwd: &Path, path: &[u8]) -> Result<PathBuf> {
    let relative = PathBuf::from(platform::os_string(path));
    ensure!(
        relative
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_))),
        "Choose a file inside the checkout"
    );
    let full = cwd.join(&relative);
    let meta = fs::symlink_metadata(&full).context("The file is not in the working tree")?;
    ensure!(meta.is_file(), "Only regular files can be edited");
    ensure!(
        meta.len() <= MAX_FILE,
        "File exceeds the 2 MiB editing limit"
    );
    let folder = full
        .parent()
        .context("Choose a file inside the checkout")?
        .canonicalize()
        .map(crate::platform::plain_path)?;
    ensure!(
        folder.starts_with(cwd.canonicalize().map(crate::platform::plain_path)?),
        "File resolves outside the task checkout"
    );
    Ok(full)
}
// Returns the bytes of `path` at `revision`, where an empty revision names the index, or
// nothing when the revision has no such file.
fn blob(cwd: &Path, revision: &str, path: &[u8]) -> Result<Option<Vec<u8>>> {
    let mut object = OsString::from(format!("{revision}:"));
    object.push(platform::os_string(path));
    let exists = git_process(cwd)?
        .args(["cat-file", "-e"])
        .arg(&object)
        .output()?;
    if !exists.status.success() {
        return Ok(None);
    }
    command(cwd, &[OsStr::new("cat-file"), OsStr::new("blob"), &object]).map(Some)
}
/// Lists tracked and untracked files as original path bytes.
///
/// A folder without Git has no ignore rules that leave out generated files, so its list skips
/// hidden entries and stops at a limit.
pub fn files(cwd: &Path) -> Result<Vec<Vec<u8>>> {
    if !is_repository(cwd) {
        return folder_files(cwd);
    }
    Ok(query(
        cwd,
        &[
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ],
    )?
    .split(|b| *b == 0)
    .filter(|p| !p.is_empty())
    .map(|p| p.to_vec())
    .collect())
}
// Walks the folder breadth first, so the limit keeps the files nearest to its root. Symbolic
// links are listed as files and never followed.
fn folder_files(cwd: &Path) -> Result<Vec<Vec<u8>>> {
    let mut files = Vec::new();
    let mut folders = std::collections::VecDeque::from([PathBuf::new()]);
    while let Some(relative) = folders.pop_front() {
        let entries = match fs::read_dir(cwd.join(&relative)) {
            Ok(entries) => entries,
            Err(error) if relative.as_os_str().is_empty() => return Err(error.into()),
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with('.') {
                continue;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = relative.join(name);
            if kind.is_dir() {
                folders.push_back(path);
            } else {
                files.push(platform::os_bytes(path.as_os_str()));
                if files.len() >= MAX_FOLDER_FILES {
                    return Ok(files);
                }
            }
        }
    }
    Ok(files)
}
/// Reads a bounded regular file inside the task checkout without following outside symlinks.
pub fn read_file(cwd: &Path, path: &[u8]) -> Result<String> {
    let resolved = cwd
        .join(platform::os_string(path))
        .canonicalize()
        .map(crate::platform::plain_path)?;
    ensure!(
        resolved.starts_with(cwd.canonicalize().map(crate::platform::plain_path)?),
        "File resolves outside the task checkout"
    );
    read_text(&resolved)
}
/// Reads a regular text file within the viewer's size limit.
pub fn read_text(path: &Path) -> Result<String> {
    let metadata = fs::metadata(path)?;
    ensure!(metadata.is_file(), "Choose a regular file");
    ensure!(
        metadata.len() <= MAX_FILE,
        "File exceeds 2 MiB viewer limit"
    );
    String::from_utf8(fs::read(path)?).context("Binary/non-UTF8 file has no text preview")
}
