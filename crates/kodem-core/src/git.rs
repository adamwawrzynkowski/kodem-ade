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

/// Executes a Git machine-format query without shell interpretation or external diff drivers.
pub fn command(cwd: &Path, args: &[&OsStr]) -> Result<Vec<u8>> {
    let out = Command::new("git")
        .current_dir(cwd)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .args(["--no-pager", "-c", "core.quotepath=false"])
        .args(args)
        .stdin(Stdio::null())
        .output()?;
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
/// Detects committed task work merged into a locally known default branch without fetching.
/// Unchanged branches, default-branch tasks and dirty checkouts are never considered merged.
pub fn branch_merged(project: &Path, task: &Task) -> bool {
    let Some(baseline) = &task.baseline else {
        return false;
    };
    let resolve = |reference: &str| -> Option<String> {
        query(
            project,
            &[
                "rev-parse",
                "--verify",
                "--end-of-options",
                &format!("{reference}^{{commit}}"),
            ],
        )
        .ok()
        .map(|bytes| String::from_utf8_lossy(&bytes).trim().to_owned())
    };
    let remote = query(project, &["symbolic-ref", "refs/remotes/origin/HEAD"])
        .ok()
        .map(|bytes| String::from_utf8_lossy(&bytes).trim().to_owned());
    let default = remote
        .as_deref()
        .and_then(|reference| reference.strip_prefix("refs/remotes/origin/"))
        .map(str::to_owned)
        .or_else(|| {
            ["main", "master"]
                .into_iter()
                .find(|name| resolve(&format!("refs/heads/{name}")).is_some())
                .map(str::to_owned)
        });
    let Some(default) = default else {
        return false;
    };
    if task.branch == default || task.branch == "detached HEAD" {
        return false;
    }
    let Some(tip) = resolve(&format!("refs/heads/{}", task.branch)) else {
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
        .filter_map(|reference| resolve(&reference))
        .any(|target| query(project, &["merge-base", "--is-ancestor", &tip, &target]).is_ok())
}
/// Creates an isolated task worktree on a new branch, preserving the original checkout.
pub fn worktree(project: &Path, root: &Path, id: &str) -> Result<PathBuf> {
    ensure!(
        head(project).is_some(),
        "Create an initial commit before making a task worktree, or choose the current checkout"
    );
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
            OsStr::new("HEAD"),
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
            Ok(digest(&query(&path, &["status", "--porcelain=v1", "-z"])?))
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
/// Captures stable observed content, retrying boundedly when external writes race the capture.
pub fn capture(task: &Task, comparison: Comparison) -> Result<DiffSnapshot> {
    for _ in 0..3 {
        let before = index_digest(&task.cwd)?;
        let observed_head = head(&task.cwd);
        let comparison_base = if comparison == Comparison::Baseline {
            task.baseline.clone()
        } else {
            observed_head.clone()
        };
        let files = status(&task.cwd)?;
        let status_before = serde_json::to_vec(&files)?;
        let mut entries = Vec::new();
        // Baseline may include committed paths absent from current status.
        let mut paths = files.clone();
        if comparison == Comparison::Baseline {
            let base = task
                .baseline
                .as_deref()
                .context("No task baseline exists yet")?;
            for p in query(&task.cwd, &["diff", "--name-only", "-z", base, "--"])?
                .split(|b| *b == 0)
                .filter(|p| !p.is_empty())
            {
                if !paths.iter().any(|f| f.0 == p) {
                    paths.push((p.to_vec(), None, "baseline".into()));
                }
            }
        }
        let mut stable = true;
        for (path, old, kind) in paths {
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
                    Some("Submodule or directory change. Inspect with Git.".into()),
                )
            } else if meta.as_ref().is_some_and(|m| m.len() > MAX_FILE) {
                (
                    Vec::new(),
                    Some("File exceeds the 2 MiB inline review limit.".into()),
                )
            } else if kind == "??"
                || (comparison == Comparison::WorkingTree
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
                comparison,
                baseline: comparison_base,
                captured: now(),
                index_digest: before,
                files: entries,
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
        label: file.label.clone(),
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
        label: file.label.clone(),
        side,
        start: *numbers.first().context("No source lines")?,
        end: *numbers.last().context("No source lines")?,
        excerpt,
        comment,
        digest: file.digest.clone(),
    })
}
/// Stages one captured whole file after validating both content and index identity.
pub fn stage(task: &Task, snapshot: &DiffSnapshot, file: usize) -> Result<()> {
    ensure!(
        snapshot.task_id == task.id,
        "Snapshot belongs to another task"
    );
    let f = snapshot.files.get(file).context("Unknown file")?;
    ensure!(
        !f.status.contains('U') && f.status != "AA" && f.status != "DD",
        "Resolve this conflict before staging"
    );
    ensure!(
        snapshot.index_digest == index_digest(&task.cwd)?
            && f.digest == path_digest(&task.cwd, &f.path)?,
        "Review is stale. Refresh before staging."
    );
    let index = index_path(&task.cwd)?;
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
            snapshot.index_digest == index_digest(&task.cwd)?,
            "Index changed before staging"
        );
        if index.exists() {
            fs::copy(&index, &temp)?;
        }
        let mut cmd = Command::new("git");
        cmd.current_dir(&task.cwd)
            .env("GIT_INDEX_FILE", &temp)
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
            f.digest == path_digest(&task.cwd, &f.path)?,
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
