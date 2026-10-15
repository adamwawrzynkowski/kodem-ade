use std::collections::HashMap;
use std::path::Path;

use super::models::*;
use super::repo::{FIELD_SEP, RECORD_SEP};

/// Parses `git log` output in the format that `GitRepo::log` uses. Skips records with fewer than
/// ten fields.
pub fn parse_log(out: &str) -> Vec<Commit> {
    let mut commits = Vec::new();
    for record in out.split(RECORD_SEP) {
        let record = record.trim_start_matches('\n');
        if record.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = record.split(FIELD_SEP).collect();
        if fields.len() < 10 {
            continue;
        }
        commits.push(Commit {
            hash: fields[0].to_string(),
            short_hash: fields[1].to_string(),
            parents: fields[2]
                .split_whitespace()
                .map(|s| s.to_string())
                .collect(),
            author_name: fields[3].to_string(),
            author_email: fields[4].to_string(),
            author_date: fields[5].parse().unwrap_or(0),
            committer_date: fields[6].parse().unwrap_or(0),
            refs: parse_refs(fields[7]),
            subject: fields[8].to_string(),
            body: fields[9].trim_end().to_string(),
            additions: None,
            deletions: None,
        });
    }
    commits
}

fn parse_refs(decoration: &str) -> Vec<CommitRef> {
    let mut refs = Vec::new();
    for part in decoration.split(", ") {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some(rest) = part.strip_prefix("HEAD -> ") {
            refs.push(CommitRef {
                name: "HEAD".into(),
                kind: RefKind::Head,
            });
            refs.push(CommitRef {
                name: rest.to_string(),
                kind: RefKind::LocalBranch,
            });
        } else if part == "HEAD" {
            refs.push(CommitRef {
                name: "HEAD".into(),
                kind: RefKind::Head,
            });
        } else if let Some(tag) = part.strip_prefix("tag: ") {
            refs.push(CommitRef {
                name: tag.to_string(),
                kind: RefKind::Tag,
            });
        } else if part.contains('/') {
            refs.push(CommitRef {
                name: part.to_string(),
                kind: RefKind::RemoteBranch,
            });
        } else {
            refs.push(CommitRef {
                name: part.to_string(),
                kind: RefKind::LocalBranch,
            });
        }
    }
    refs
}

/// Parse `git log --pretty=format:<RS>%H --numstat` output.
pub fn parse_log_stats(out: &str) -> Vec<(String, u32, u32)> {
    let mut stats = Vec::new();
    for record in out.split(RECORD_SEP) {
        let record = record.trim();
        if record.is_empty() {
            continue;
        }
        let mut lines = record.lines();
        let Some(hash) = lines.next() else { continue };
        let mut additions = 0u32;
        let mut deletions = 0u32;
        for line in lines {
            let mut cols = line.split('\t');
            let a = cols.next().unwrap_or("");
            let d = cols.next().unwrap_or("");
            // Binary files show "-" in numstat.
            additions += a.parse::<u32>().unwrap_or(0);
            deletions += d.parse::<u32>().unwrap_or(0);
        }
        stats.push((hash.trim().to_string(), additions, deletions));
    }
    stats
}

/// Parse `git status --porcelain=v1 -z` output.
pub fn parse_status(out: &str) -> RepoStatus {
    let mut status = RepoStatus::default();
    let mut parts = out.split('\0').peekable();
    while let Some(entry) = parts.next() {
        if entry.len() < 4 {
            continue;
        }
        let index_char = entry.as_bytes()[0] as char;
        let work_char = entry.as_bytes()[1] as char;
        let path = entry[3..].to_string();

        let mut original_path = None;
        if index_char == 'R' || index_char == 'C' {
            // In -z mode the original path follows as a separate record.
            original_path = parts.next().map(|s| s.to_string());
        }

        let conflicted = matches!(
            (index_char, work_char),
            ('U', _) | (_, 'U') | ('A', 'A') | ('D', 'D')
        );
        if conflicted {
            status.conflicted.push(StatusEntry {
                path,
                original_path,
                kind: FileChangeKind::Conflicted,
                staged: false,
                is_submodule: false,
                additions: 0,
                deletions: 0,
            });
            continue;
        }

        if index_char == '?' {
            status.unstaged.push(StatusEntry {
                path,
                original_path: None,
                kind: FileChangeKind::Untracked,
                staged: false,
                is_submodule: false,
                additions: 0,
                deletions: 0,
            });
            continue;
        }

        if index_char != ' ' {
            status.staged.push(StatusEntry {
                path: path.clone(),
                original_path: original_path.clone(),
                kind: change_kind(index_char),
                staged: true,
                is_submodule: false,
                additions: 0,
                deletions: 0,
            });
        }
        if work_char != ' ' {
            status.unstaged.push(StatusEntry {
                path,
                original_path,
                kind: change_kind(work_char),
                staged: false,
                is_submodule: false,
                additions: 0,
                deletions: 0,
            });
        }
    }
    status
}

/// Parse per-file line counts from `git diff --numstat -z`.
pub fn parse_numstat(out: &str) -> HashMap<String, (u32, u32)> {
    let records: Vec<&str> = out.split('\0').collect();
    let mut metrics = HashMap::new();
    let mut index = 0;

    while index < records.len() {
        let record = records[index];
        index += 1;
        if record.is_empty() {
            continue;
        }

        let mut fields = record.splitn(3, '\t');
        let (Some(additions), Some(deletions), Some(path)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };

        let path = if path.is_empty() {
            if index + 1 >= records.len() {
                break;
            }
            let new_path = records[index + 1];
            index += 2;
            new_path
        } else {
            path
        };
        if path.is_empty() {
            continue;
        }

        metrics.insert(
            path.to_string(),
            (
                additions.parse().unwrap_or(0),
                deletions.parse().unwrap_or(0),
            ),
        );
    }

    metrics
}

/// Parse file states from `git diff --name-status -z`.
pub fn parse_name_status(out: &str, staged: bool) -> Vec<StatusEntry> {
    let mut fields = out.split('\0');
    let mut entries = Vec::new();

    while let Some(code) = fields.next() {
        if code.is_empty() {
            continue;
        }
        let kind = change_kind(code.chars().next().unwrap_or('M'));
        let (original_path, path) =
            if matches!(kind, FileChangeKind::Renamed | FileChangeKind::Copied) {
                let Some(original_path) = fields.next().filter(|path| !path.is_empty()) else {
                    break;
                };
                let Some(path) = fields.next().filter(|path| !path.is_empty()) else {
                    break;
                };
                (Some(original_path.to_string()), path.to_string())
            } else {
                let Some(path) = fields.next().filter(|path| !path.is_empty()) else {
                    break;
                };
                (None, path.to_string())
            };

        entries.push(StatusEntry {
            path,
            original_path,
            kind,
            staged,
            is_submodule: false,
            additions: 0,
            deletions: 0,
        });
    }

    entries
}

fn change_kind(c: char) -> FileChangeKind {
    match c {
        'A' => FileChangeKind::Added,
        'D' => FileChangeKind::Deleted,
        'R' => FileChangeKind::Renamed,
        'C' => FileChangeKind::Copied,
        _ => FileChangeKind::Modified,
    }
}

/// Parses `git for-each-ref` branch output. Skips the `HEAD` alias of each remote.
pub fn parse_branches(out: &str, is_remote: bool) -> Vec<Branch> {
    let mut branches = Vec::new();
    for line in out.lines() {
        let fields: Vec<&str> = line.split(FIELD_SEP).collect();
        if fields.len() < 5 {
            continue;
        }
        let name = fields[0].to_string();
        if is_remote && name.ends_with("/HEAD") {
            continue;
        }
        let (ahead, behind) = parse_track(fields[3]);
        branches.push(Branch {
            name,
            is_current: fields[1] == "*",
            is_remote,
            upstream: if fields[2].is_empty() {
                None
            } else {
                Some(fields[2].to_string())
            },
            ahead,
            behind,
            last_commit_date: fields[4].trim().parse().unwrap_or(0),
            hash: fields.get(5).unwrap_or(&"").trim().to_string(),
            short_hash: fields.get(6).unwrap_or(&"").trim().to_string(),
            subject: fields.get(7).unwrap_or(&"").trim().to_string(),
        });
    }
    branches
}

/// Parse "[ahead 2, behind 1]" from `%(upstream:track)`.
fn parse_track(track: &str) -> (u32, u32) {
    let mut ahead = 0;
    let mut behind = 0;
    let track = track.trim_start_matches('[').trim_end_matches(']');
    for part in track.split(", ") {
        if let Some(n) = part.strip_prefix("ahead ") {
            ahead = n.parse().unwrap_or(0);
        } else if let Some(n) = part.strip_prefix("behind ") {
            behind = n.parse().unwrap_or(0);
        }
    }
    (ahead, behind)
}

/// Parses `git remote -v` output and keeps the fetch URL of each remote.
pub fn parse_remotes(out: &str) -> Vec<Remote> {
    let mut remotes: Vec<Remote> = Vec::new();
    for line in out.lines() {
        let mut cols = line.split_whitespace();
        let (Some(name), Some(url), Some(kind)) = (cols.next(), cols.next(), cols.next()) else {
            continue;
        };
        if kind != "(fetch)" {
            continue;
        }
        remotes.push(Remote {
            name: name.to_string(),
            fetch_url: url.to_string(),
        });
    }
    remotes
}

/// Parses `git for-each-ref` tag output.
pub fn parse_tags(out: &str) -> Vec<Tag> {
    let mut tags = Vec::new();
    for line in out.lines() {
        let fields: Vec<&str> = line.split(FIELD_SEP).collect();
        if fields.len() < 3 {
            continue;
        }
        tags.push(Tag {
            name: fields[0].to_string(),
            target: fields[1].to_string(),
            date: fields[2].trim().parse().unwrap_or(0),
        });
    }
    tags
}

/// Parses `git stash list` output. The index of each stash is its line number.
pub fn parse_stashes(out: &str) -> Vec<Stash> {
    let mut stashes = Vec::new();
    for (index, line) in out.lines().enumerate() {
        let fields: Vec<&str> = line.split(FIELD_SEP).collect();
        if fields.len() < 3 {
            continue;
        }
        stashes.push(Stash {
            index,
            reference: fields[0].to_string(),
            message: fields[1].to_string(),
            relative_date: fields[2].to_string(),
        });
    }
    stashes
}

/// Parses `git worktree list --porcelain` output. Marks the worktree at `current` as current and
/// keeps the first 7 characters of each `HEAD` hash.
pub fn parse_worktrees(out: &str, current: &Path) -> Vec<Worktree> {
    let mut worktrees = Vec::new();
    let mut path: Option<String> = None;
    let mut head = String::new();
    let mut branch: Option<String> = None;
    for line in out.lines().chain(std::iter::once("")) {
        if line.is_empty() {
            if let Some(p) = path.take() {
                let p = std::path::PathBuf::from(p);
                worktrees.push(Worktree {
                    is_current: p == current,
                    path: p,
                    head: std::mem::take(&mut head),
                    branch: branch.take(),
                });
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("worktree ") {
            path = Some(rest.to_string());
        } else if let Some(rest) = line.strip_prefix("HEAD ") {
            head = rest.chars().take(7).collect();
        } else if let Some(rest) = line.strip_prefix("branch ") {
            branch = Some(rest.trim_start_matches("refs/heads/").to_string());
        }
    }
    worktrees
}

/// Parse unified diff output into per-file structures.
pub fn parse_diff(out: &str) -> Vec<FileDiff> {
    let mut files: Vec<FileDiff> = Vec::new();
    let mut current: Option<FileDiff> = None;
    let mut hunk: Option<DiffHunk> = None;
    let mut old_line = 0u32;
    let mut new_line = 0u32;

    let close_hunk = |current: &mut Option<FileDiff>, hunk: &mut Option<DiffHunk>| {
        if let (Some(file), Some(h)) = (current.as_mut(), hunk.take()) {
            file.hunks.push(h);
        }
    };

    for line in out.lines() {
        if line.starts_with("diff --git ") {
            close_hunk(&mut current, &mut hunk);
            if let Some(mut file) = current.take() {
                file.detect_lfs_pointers();
                files.push(file);
            }
            current = Some(FileDiff::default());
            continue;
        }
        let Some(file) = current.as_mut() else {
            continue;
        };
        if line.starts_with("Binary files") || line.starts_with("GIT binary patch") {
            file.is_binary = true;
            continue;
        }
        if let Some(rest) = line.strip_prefix("--- ") {
            file.old_path = rest
                .trim_start_matches("a/")
                .trim_start_matches("\"a/")
                .trim_end_matches('"')
                .to_string();
            continue;
        }
        if let Some(rest) = line.strip_prefix("+++ ") {
            file.new_path = rest
                .trim_start_matches("b/")
                .trim_start_matches("\"b/")
                .trim_end_matches('"')
                .to_string();
            continue;
        }
        if line.starts_with("@@") {
            close_hunk(&mut current, &mut hunk);
            let file = current.as_mut().unwrap();
            let _ = file;
            let (os, oc, ns, nc) = parse_hunk_header(line);
            old_line = os;
            new_line = ns;
            hunk = Some(DiffHunk {
                header: line.to_string(),
                old_start: os,
                old_count: oc,
                new_start: ns,
                new_count: nc,
                lines: Vec::new(),
            });
            continue;
        }
        let Some(h) = hunk.as_mut() else { continue };
        if let Some(text) = line.strip_prefix('+') {
            h.lines.push(DiffLine {
                kind: DiffLineKind::Added,
                old_line: None,
                new_line: Some(new_line),
                text: text.to_string(),
            });
            new_line += 1;
        } else if let Some(text) = line.strip_prefix('-') {
            h.lines.push(DiffLine {
                kind: DiffLineKind::Removed,
                old_line: Some(old_line),
                new_line: None,
                text: text.to_string(),
            });
            old_line += 1;
        } else if let Some(text) = line.strip_prefix(' ') {
            h.lines.push(DiffLine {
                kind: DiffLineKind::Context,
                old_line: Some(old_line),
                new_line: Some(new_line),
                text: text.to_string(),
            });
            old_line += 1;
            new_line += 1;
        }
    }
    close_hunk(&mut current, &mut hunk);
    if let Some(mut file) = current.take() {
        file.detect_lfs_pointers();
        files.push(file);
    }
    files
}

/// Parse "@@ -6,16 +6,17 @@ context" into start/count pairs.
fn parse_hunk_header(line: &str) -> (u32, u32, u32, u32) {
    let mut old = (0, 0);
    let mut new = (0, 0);
    for part in line.split_whitespace() {
        if let Some(rest) = part.strip_prefix('-') {
            old = parse_range(rest);
        } else if let Some(rest) = part.strip_prefix('+') {
            new = parse_range(rest);
            break;
        }
    }
    (old.0, old.1, new.0, new.1)
}

fn parse_range(s: &str) -> (u32, u32) {
    let mut nums = s.split(',');
    let start = nums.next().and_then(|n| n.parse().ok()).unwrap_or(0);
    let count = nums.next().and_then(|n| n.parse().ok()).unwrap_or(1);
    (start, count)
}

/// Combines the `git diff --numstat` and `--name-status` output of one commit into changed files. A
/// file without a status counts as modified.
pub fn parse_commit_files(numstat: &str, name_status: &str) -> Vec<CommitFile> {
    let mut kinds = std::collections::HashMap::new();
    for line in name_status.lines() {
        let mut cols = line.split('\t');
        let (Some(code), Some(path)) = (cols.next(), cols.next()) else {
            continue;
        };
        // Renames have "R100\told\tnew"; use the new path.
        let path = cols.next().unwrap_or(path);
        let kind = match code.chars().next().unwrap_or('M') {
            'A' => FileChangeKind::Added,
            'D' => FileChangeKind::Deleted,
            'R' => FileChangeKind::Renamed,
            'C' => FileChangeKind::Copied,
            _ => FileChangeKind::Modified,
        };
        kinds.insert(path.to_string(), kind);
    }

    let mut files = Vec::new();
    for line in numstat.lines() {
        let mut cols = line.split('\t');
        let (Some(a), Some(d), Some(path)) = (cols.next(), cols.next(), cols.next()) else {
            continue;
        };
        // Renames appear as "old => new" or "prefix/{old => new}/suffix".
        let clean_path = if let Some(rest) = cols.next() {
            rest.to_string()
        } else {
            path.to_string()
        };
        files.push(CommitFile {
            kind: kinds
                .get(&clean_path)
                .copied()
                .unwrap_or(FileChangeKind::Modified),
            path: clean_path,
            additions: a.parse().unwrap_or(0),
            deletions: d.parse().unwrap_or(0),
        });
    }
    files
}
