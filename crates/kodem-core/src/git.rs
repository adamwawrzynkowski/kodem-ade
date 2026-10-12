//! Byte-preserving Git capture and guarded whole-file staging.
use crate::model::*;
use anyhow::{Context, Result, bail, ensure};
use sha2::{Digest, Sha256};
use std::{
    ffi::{OsStr, OsString},
    fs,
    io::{Read, Write},
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::OpenOptionsExt,
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
const MAX_FILE: u64 = 2 * 1024 * 1024;
mod repositories;
mod review;
pub use repositories::{repositories, repository_baselines};
pub use review::{PreparedReview, discard_review, prepare_review, review_targets};

// Finder's PATH can omit installed tools. Git's filters and credential helpers, and GitHub CLI,
// need the augmented PATH too, even when Git itself is already discoverable.
fn search_path() -> Result<OsString> {
    let mut paths: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    let mut fallbacks = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        fallbacks.push(PathBuf::from(home).join(".local/bin"));
    }
    fallbacks.extend(
        [
            "/opt/homebrew/bin",
            "/usr/local/bin",
            "/usr/bin",
            "/bin",
            "/usr/sbin",
            "/sbin",
        ]
        .map(PathBuf::from),
    );
    for path in fallbacks {
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    std::env::join_paths(paths).context("Build Git PATH")
}

fn git_process(cwd: &Path) -> Result<Command> {
    let mut command = Command::new("git");
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
/// Resolves and validates a local repository root without changing branches.
pub fn repository(path: &Path) -> Result<PathBuf> {
    let canonical = path.canonicalize()?;
    ensure!(
        query(&canonical, &["rev-parse", "--is-bare-repository"])? == b"false\n",
        "Choose a working-tree repository"
    );
    let root = query(&canonical, &["rev-parse", "--show-toplevel"])?;
    Ok(PathBuf::from(OsString::from_vec(
        root.strip_suffix(b"\n").unwrap_or(&root).to_vec(),
    )))
}
/// Returns the current commit, including the unborn-repository case.
pub fn head(cwd: &Path) -> Option<String> {
    query(cwd, &["rev-parse", "--verify", "HEAD"])
        .ok()
        .map(|v| String::from_utf8_lossy(&v).trim().to_owned())
}
/// Returns branch metadata without checking out the branch.
pub fn branch(cwd: &Path) -> String {
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
/// Unchanged branches, default-branch tasks and dirty checkouts are never considered merged.
pub fn branch_merged(project: &Path, task: &Task) -> bool {
    let Some(baseline) = &task.baseline else {
        return false;
    };
    let Some((default, remote)) = default_branch(project) else {
        return false;
    };
    if task.branch == default || task.branch == "detached HEAD" {
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
/// Creates an isolated task worktree on a new branch, preserving the original checkout.
///
/// The branch starts at `start` when it names a commit, otherwise at the project's current commit.
pub fn worktree(project: &Path, root: &Path, id: &str, start: Option<&str>) -> Result<PathBuf> {
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
    let path = PathBuf::from(OsString::from_vec(
        bytes.strip_suffix(b"\n").unwrap_or(&bytes).to_vec(),
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
    let path = cwd.join(OsStr::from_bytes(path));
    match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(digest(b"missing")),
        Err(e) => Err(e.into()),
        Ok(meta) if meta.file_type().is_symlink() => {
            Ok(digest(fs::read_link(path)?.as_os_str().as_bytes()))
        }
        Ok(meta) if meta.is_dir() => {
            let mut state = query(&path, &["status", "--porcelain=v1", "-z"])?;
            state.extend_from_slice(head(&path).unwrap_or_default().as_bytes());
            Ok(digest(&state))
        }
        Ok(meta) => {
            use std::os::unix::fs::PermissionsExt;
            let mut hash = Sha256::new();
            hash.update(meta.permissions().mode().to_le_bytes());
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
    args.push(OsString::from_vec(path.to_vec()));
    if let Some(old) = old {
        args.push(OsString::from_vec(old.to_vec()));
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
    let root = task.cwd.canonicalize()?;
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
            let full = task.cwd.join(OsStr::from_bytes(&path));
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
                label: OsStr::from_bytes(&path).to_string_lossy().into(),
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
    let mut guard = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&lock)
        .context("Git index is locked by another operation")?;
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
            .arg(OsStr::from_bytes(&f.path));
        if let Some(old) = &f.old_path {
            cmd.arg(OsStr::from_bytes(old));
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
/// Lists tracked and untracked files as original path bytes.
pub fn files(cwd: &Path) -> Result<Vec<Vec<u8>>> {
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
/// Reads a bounded regular file inside the task checkout without following outside symlinks.
pub fn read_file(cwd: &Path, path: &[u8]) -> Result<String> {
    let resolved = cwd.join(OsStr::from_bytes(path)).canonicalize()?;
    ensure!(
        resolved.starts_with(cwd.canonicalize()?),
        "File resolves outside the task checkout"
    );
    ensure!(fs::metadata(&resolved)?.is_file(), "Choose a regular file");
    ensure!(
        fs::metadata(&resolved)?.len() <= MAX_FILE,
        "File exceeds 2 MiB viewer limit"
    );
    String::from_utf8(fs::read(resolved)?).context("Binary/non-UTF8 file has no text preview")
}
