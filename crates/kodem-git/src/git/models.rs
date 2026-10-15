// Some fields mirror git data that the UI does not show yet.
#![allow(dead_code)]

use std::path::PathBuf;

/// One commit row in the history list.
#[derive(Debug, Clone)]
pub struct Commit {
    pub hash: String,
    pub short_hash: String,
    pub parents: Vec<String>,
    pub author_name: String,
    pub author_email: String,
    pub author_date: i64,
    pub committer_date: i64,
    pub refs: Vec<CommitRef>,
    pub subject: String,
    pub body: String,
    pub additions: Option<u32>,
    pub deletions: Option<u32>,
}

/// A ref (branch, remote branch, tag, HEAD) that points to a commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitRef {
    pub name: String,
    pub kind: RefKind,
}

/// The kind of a ref that points to a commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefKind {
    Head,
    LocalBranch,
    RemoteBranch,
    Tag,
}

/// A local or remote-tracking branch with its upstream, ahead and behind counts, and last commit.
#[derive(Debug, Clone)]
pub struct Branch {
    pub name: String,
    pub is_current: bool,
    pub is_remote: bool,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub last_commit_date: i64,
    pub hash: String,
    pub short_hash: String,
    pub subject: String,
}

impl Branch {
    /// Local name, or the path after `origin/` for a remote branch.
    pub fn display_name(&self) -> &str {
        if self.is_remote {
            self.name
                .split_once('/')
                .map(|(_, rest)| rest)
                .unwrap_or(self.name.as_str())
        } else {
            self.name.as_str()
        }
    }

    /// Remote name for a remote-tracking branch, such as `origin`.
    pub fn remote_name(&self) -> Option<&str> {
        if self.is_remote {
            self.name.split_once('/').map(|(head, _)| head)
        } else {
            None
        }
    }
}

/// A remote and its fetch URL.
#[derive(Debug, Clone)]
pub struct Remote {
    pub name: String,
    pub fetch_url: String,
}

/// A tag with its short target hash and its creation date as a Unix timestamp.
#[derive(Debug, Clone)]
pub struct Tag {
    pub name: String,
    pub target: String,
    pub date: i64,
}

/// One stash entry, such as `stash@{0}`, with its message and relative date.
#[derive(Debug, Clone)]
pub struct Stash {
    pub index: usize,
    pub reference: String,
    pub message: String,
    pub relative_date: String,
}

/// A checkpoint ref created by an AI coding tool (t3, synara).
#[derive(Debug, Clone)]
pub struct Checkpoint {
    pub reference: String,
    pub hash: String,
    pub message: String,
}

impl Checkpoint {
    /// Human label taken from the `/turn/N` suffix of the ref.
    pub fn turn_label(&self) -> String {
        match self.reference.rfind("/turn/") {
            Some(ix) => {
                let turn = &self.reference[ix + "/turn/".len()..];
                if turn.is_empty() {
                    "Checkpoint".to_string()
                } else {
                    format!("Turn {turn}")
                }
            }
            None => "Checkpoint".to_string(),
        }
    }
}

/// Git LFS state for the sidebar LFS category.
#[derive(Debug, Clone, Default)]
pub struct LfsInfo {
    pub available: bool,
    pub patterns: Vec<String>,
    pub object_count: usize,
}

/// One worktree of a repository, with its short `HEAD` hash and its checked-out branch.
#[derive(Debug, Clone)]
pub struct Worktree {
    pub path: PathBuf,
    pub head: String,
    pub branch: Option<String>,
    pub is_current: bool,
}

/// State of one file in `git status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    Untracked,
    Conflicted,
}

impl FileChangeKind {
    /// Returns the one-letter badge of the change: `N` for an untracked file and `!` for a
    /// conflict.
    pub fn letter(&self) -> &'static str {
        match self {
            FileChangeKind::Added => "A",
            FileChangeKind::Modified => "M",
            FileChangeKind::Deleted => "D",
            FileChangeKind::Renamed => "R",
            FileChangeKind::Copied => "C",
            FileChangeKind::Untracked => "N",
            FileChangeKind::Conflicted => "!",
        }
    }
}

/// One changed file in the status, with its change kind and line counts.
#[derive(Debug, Clone)]
pub struct StatusEntry {
    pub path: String,
    pub original_path: Option<String>,
    pub kind: FileChangeKind,
    pub staged: bool,
    pub is_submodule: bool,
    pub additions: u32,
    pub deletions: u32,
}

/// The message of `HEAD` and the changes that an amend of it would commit.
#[derive(Debug, Clone)]
pub struct AmendState {
    pub title: String,
    pub description: String,
    pub staged: Vec<StatusEntry>,
}

/// The operation that stopped with conflicts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictOperation {
    Rebase,
    Merge,
    CherryPick,
    Revert,
}

impl ConflictOperation {
    /// Returns the name of the operation as the interface shows it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Rebase => "Rebase",
            Self::Merge => "Merge",
            Self::CherryPick => "Cherry-pick",
            Self::Revert => "Revert",
        }
    }

    /// Returns the label of the button that aborts the operation.
    pub fn abort_label(self) -> &'static str {
        match self {
            Self::Rebase => "Abort Rebase",
            Self::Merge => "Abort Merge",
            Self::CherryPick => "Abort Cherry-pick",
            Self::Revert => "Abort Revert",
        }
    }
}

/// The staged, unstaged and conflicted files of a repository, and the operation that caused the
/// conflicts.
#[derive(Debug, Clone, Default)]
pub struct RepoStatus {
    pub staged: Vec<StatusEntry>,
    pub unstaged: Vec<StatusEntry>,
    pub conflicted: Vec<StatusEntry>,
    pub conflict_operation: Option<ConflictOperation>,
}

/// One Git LFS pointer stored in Git instead of the file data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LfsPointer {
    pub version: String,
    pub oid: String,
    pub size: u64,
    pub pointer_lines: Vec<String>,
}

impl LfsPointer {
    /// Parses the lines of an LFS pointer file. Returns `None` when the spec version, the `sha256`
    /// oid or the size is missing or invalid.
    pub fn parse(lines: &[String]) -> Option<Self> {
        let mut version = None;
        let mut oid = None;
        let mut size = None;
        let mut pointer_lines = Vec::new();

        for raw_line in lines {
            let line = raw_line.trim();
            if let Some(value) = line.strip_prefix("version ") {
                version = Some(value.to_string());
                pointer_lines.push(line.to_string());
            } else if let Some(value) = line.strip_prefix("oid ") {
                oid = Some(value.to_string());
                pointer_lines.push(line.to_string());
            } else if let Some(value) = line.strip_prefix("size ") {
                size = value.parse::<u64>().ok();
                pointer_lines.push(line.to_string());
            } else if line.starts_with("ext-") {
                pointer_lines.push(line.to_string());
            }
        }

        let version = version?;
        let oid = oid?;
        if !version.starts_with("https://git-lfs.github.com/spec/") || !oid.starts_with("sha256:") {
            return None;
        }

        Some(Self {
            version,
            oid,
            size: size?,
            pointer_lines,
        })
    }

    /// Returns the text of the pointer file, ending with a newline.
    pub fn pointer_content(&self) -> String {
        format!("{}\n", self.pointer_lines.join("\n"))
    }

    /// Returns the oid with the hash cut to 12 characters, such as `sha256:0123456789ab`.
    pub fn short_oid(&self) -> String {
        self.oid
            .strip_prefix("sha256:")
            .map(|hash| format!("sha256:{}", hash.chars().take(12).collect::<String>()))
            .unwrap_or_else(|| self.oid.clone())
    }
}

/// An image format that GPUI can decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewImageFormat {
    Png,
    Jpeg,
    Webp,
    Gif,
    Svg,
    Bmp,
    Tiff,
    Ico,
    Pnm,
}

impl PreviewImageFormat {
    /// Returns the name of the format as the interface shows it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
            Self::Webp => "WEBP",
            Self::Gif => "GIF",
            Self::Svg => "SVG",
            Self::Bmp => "BMP",
            Self::Tiff => "TIFF",
            Self::Ico => "ICO",
            Self::Pnm => "PNM",
        }
    }
}

/// One side of a file comparison.
#[derive(Debug, Clone, Default)]
pub struct FileRevision {
    pub image_bytes: Option<Vec<u8>>,
    pub image_format: Option<PreviewImageFormat>,
    pub dimensions: Option<(u32, u32)>,
    pub logical_byte_count: Option<u64>,
    pub decoded_byte_count: Option<u64>,
    pub lfs_pointer: Option<LfsPointer>,
    pub load_error: Option<String>,
}

/// The before and after revisions for an image, binary file, or LFS file.
#[derive(Debug, Clone, Default)]
pub struct FileComparison {
    pub before: Option<FileRevision>,
    pub after: Option<FileRevision>,
    pub is_image: bool,
}

impl FileComparison {
    /// Returns whether either revision is stored in Git LFS.
    pub fn is_lfs(&self) -> bool {
        self.before
            .as_ref()
            .and_then(|revision| revision.lfs_pointer.as_ref())
            .is_some()
            || self
                .after
                .as_ref()
                .and_then(|revision| revision.lfs_pointer.as_ref())
                .is_some()
    }
}

/// A parsed unified diff for one file.
#[derive(Debug, Clone, Default)]
pub struct FileDiff {
    pub old_path: String,
    pub new_path: String,
    pub hunks: Vec<DiffHunk>,
    pub is_binary: bool,
    pub lfs_before: Option<LfsPointer>,
    pub lfs_after: Option<LfsPointer>,
    pub comparison: Option<FileComparison>,
}

impl FileDiff {
    /// Sets the LFS pointers when the diff has 2 to 10 lines and either side of it is an LFS
    /// pointer.
    pub fn detect_lfs_pointers(&mut self) {
        let mut old_lines = Vec::new();
        let mut new_lines = Vec::new();
        let mut content_line_count = 0usize;

        for hunk in &self.hunks {
            for line in &hunk.lines {
                content_line_count += 1;
                match line.kind {
                    DiffLineKind::Context => {
                        old_lines.push(line.text.clone());
                        new_lines.push(line.text.clone());
                    }
                    DiffLineKind::Added => new_lines.push(line.text.clone()),
                    DiffLineKind::Removed => old_lines.push(line.text.clone()),
                }
            }
        }

        if !(2..=10).contains(&content_line_count) {
            return;
        }

        let before = LfsPointer::parse(&old_lines);
        let after = LfsPointer::parse(&new_lines);
        if before.is_some() || after.is_some() {
            self.lfs_before = before;
            self.lfs_after = after;
        }
    }
}

/// One hunk of a diff: its header, line ranges and lines.
#[derive(Debug, Clone)]
pub struct DiffHunk {
    pub header: String,
    pub old_start: u32,
    pub old_count: u32,
    pub new_start: u32,
    pub new_count: u32,
    pub lines: Vec<DiffLine>,
}

/// Whether a diff line is context, added or removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineKind {
    Context,
    Added,
    Removed,
}

/// One line of a diff hunk with its old and new line numbers.
#[derive(Debug, Clone)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
    pub text: String,
}

/// One file inside a commit, from `--numstat`.
#[derive(Debug, Clone)]
pub struct CommitFile {
    pub path: String,
    pub additions: u32,
    pub deletions: u32,
    pub kind: FileChangeKind,
}
