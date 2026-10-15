//! Snapshot of one repository that the Git workspace's views render, and the operations that
//! change it. Git runs on background threads; each result lands here and notifies the views.
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::thread::{self, ScopedJoinHandle};

use chrono::{DateTime, Local};
use gpui::{Context, EventEmitter};

use kodem_git::git::{
    Branch, Checkpoint, Commit, FileChangeKind, GitRepo, GraphRow, LfsInfo, Remote, RepoStatus,
    Stash, StatusEntry, Tag, Worktree, graph,
};

/// Snapshot of all repository data that the views render.
pub struct RepositoryState {
    pub repo: GitRepo,
    pub commits: Vec<Commit>,
    pub graph_rows: Vec<GraphRow>,
    pub status: RepoStatus,
    pub branches: Vec<Branch>,
    pub remotes: Vec<Remote>,
    pub tags: Vec<Tag>,
    pub stashes: Vec<Stash>,
    pub worktrees: Vec<Worktree>,
    pub checkpoints: Vec<Checkpoint>,
    /// `None` until the LFS category is opened the first time.
    pub lfs: Option<LfsInfo>,
    pub current_branch: String,
    pub unpushed_commits: HashSet<String>,
    pub ahead: u32,
    pub behind: u32,
    pub loading: bool,
    pub busy: Option<String>,
    pub active_operation: Option<RepositoryOperation>,
    pub last_error: Option<String>,
    pub notifications: Vec<RepositoryNotification>,
    generation: u64,
    status_generation: u64,
    lfs_loading: bool,
    /// Additions and deletions per commit hash. Commits do not change, so
    /// the values stay valid across refreshes.
    commit_stats: HashMap<String, (u32, u32)>,
}

/// Whether a finished Git operation succeeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryNotificationStatus {
    Success,
    Failure,
}

/// The record of one finished Git operation in the notification center.
#[derive(Debug, Clone)]
pub struct RepositoryNotification {
    pub title: String,
    pub body: String,
    pub status: RepositoryNotificationStatus,
    pub created_at: DateTime<Local>,
    pub is_read: bool,
}

/// A running Git operation: its label and the direction of its remote work, such as
/// `origin -> main`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryOperation {
    pub label: String,
    pub route: Option<String>,
}

/// A change of the working tree status that the views show at once, before Git confirms it.
#[derive(Debug, Clone)]
pub enum StatusMutation {
    StagePaths(Vec<String>),
    UnstageEntries(Vec<StatusEntry>),
    DiscardPaths(Vec<String>),
    StageAll,
    UnstageAll,
    DiscardAll,
}

impl StatusMutation {
    /// Applies the change to a status snapshot and keeps its lists sorted by path.
    pub(crate) fn apply(&self, status: &mut RepoStatus) {
        match self {
            Self::StagePaths(paths) => {
                let paths = paths.iter().cloned().collect::<HashSet<_>>();
                let mut moved = take_matching(&mut status.unstaged, &paths);
                moved.extend(take_matching(&mut status.conflicted, &paths));
                remove_matching(&mut status.staged, &paths);
                for entry in &mut moved {
                    entry.staged = true;
                    if entry.kind == FileChangeKind::Conflicted {
                        entry.kind = FileChangeKind::Modified;
                    }
                }
                status.staged.extend(moved);
            }
            Self::UnstageEntries(entries) => {
                let paths = entries
                    .iter()
                    .flat_map(|entry| {
                        std::iter::once(entry.path.clone()).chain(entry.original_path.clone())
                    })
                    .collect::<HashSet<_>>();
                let mut moved = take_matching(&mut status.staged, &paths);
                for entry in entries {
                    if !moved.iter().any(|moved| moved.path == entry.path) {
                        moved.push(entry.clone());
                    }
                }
                remove_matching(&mut status.unstaged, &paths);
                for entry in &mut moved {
                    entry.staged = false;
                }
                status.unstaged.extend(moved);
            }
            Self::DiscardPaths(paths) => {
                let paths = paths.iter().cloned().collect::<HashSet<_>>();
                remove_matching(&mut status.unstaged, &paths);
            }
            Self::StageAll => {
                let mut moved = std::mem::take(&mut status.unstaged);
                moved.extend(std::mem::take(&mut status.conflicted));
                let paths = moved
                    .iter()
                    .flat_map(|entry| {
                        std::iter::once(entry.path.clone()).chain(entry.original_path.clone())
                    })
                    .collect::<HashSet<_>>();
                remove_matching(&mut status.staged, &paths);
                for entry in &mut moved {
                    entry.staged = true;
                    if entry.kind == FileChangeKind::Conflicted {
                        entry.kind = FileChangeKind::Modified;
                    }
                }
                status.staged.extend(moved);
            }
            Self::UnstageAll => {
                let mut moved = std::mem::take(&mut status.staged);
                for entry in &mut moved {
                    entry.staged = false;
                }
                status.unstaged.extend(moved);
            }
            Self::DiscardAll => status.unstaged.clear(),
        }
        status
            .staged
            .sort_by(|left, right| left.path.cmp(&right.path));
        status
            .unstaged
            .sort_by(|left, right| left.path.cmp(&right.path));
        status
            .conflicted
            .sort_by(|left, right| left.path.cmp(&right.path));
    }
}

fn entry_matches(entry: &StatusEntry, paths: &HashSet<String>) -> bool {
    paths.contains(entry.path.as_str())
        || entry
            .original_path
            .as_deref()
            .is_some_and(|path| paths.contains(path))
}

fn take_matching(entries: &mut Vec<StatusEntry>, paths: &HashSet<String>) -> Vec<StatusEntry> {
    let mut moved = Vec::new();
    let mut index = 0;
    while index < entries.len() {
        if entry_matches(&entries[index], paths) {
            moved.push(entries.remove(index));
        } else {
            index += 1;
        }
    }
    moved
}

fn remove_matching(entries: &mut Vec<StatusEntry>, paths: &HashSet<String>) {
    entries.retain(|entry| !entry_matches(entry, paths));
}

/// What a repository reports to the views that show it.
#[derive(Debug, Clone)]
pub enum RepositoryEvent {
    Refreshed,
    StatusRefreshed,
    OperationStarted,
    OperationFinished(RepositoryNotification),
}

impl EventEmitter<RepositoryEvent> for RepositoryState {}

#[derive(Debug, Clone, Copy)]
enum OperationRefresh {
    All,
    Status,
}

struct OperationPlan {
    route: Option<String>,
    hide_checkpoints: bool,
    send_notifications: bool,
    refresh: OperationRefresh,
    previous_status: Option<RepoStatus>,
}

const HISTORY_LIMIT: usize = 1000;
const STATS_LIMIT: usize = 400;

struct LoadedData {
    commits: Vec<Commit>,
    branches: Vec<Branch>,
    remotes: Vec<Remote>,
    tags: Vec<Tag>,
    stashes: Vec<Stash>,
    worktrees: Vec<Worktree>,
    checkpoints: Vec<Checkpoint>,
    current_branch: String,
    unpushed_commits: HashSet<String>,
    ahead: u32,
    behind: u32,
    error: Option<String>,
}

impl RepositoryState {
    /// Creates an empty snapshot of the repository at `path`; `refresh` loads it.
    pub fn new(path: PathBuf) -> Self {
        Self {
            repo: GitRepo::new(path),
            commits: Vec::new(),
            graph_rows: Vec::new(),
            status: RepoStatus::default(),
            branches: Vec::new(),
            remotes: Vec::new(),
            tags: Vec::new(),
            stashes: Vec::new(),
            worktrees: Vec::new(),
            checkpoints: Vec::new(),
            lfs: None,
            current_branch: String::new(),
            unpushed_commits: HashSet::new(),
            ahead: 0,
            behind: 0,
            loading: false,
            busy: None,
            active_operation: None,
            last_error: None,
            notifications: Vec::new(),
            generation: 0,
            status_generation: 0,
            lfs_loading: false,
            commit_stats: HashMap::new(),
        }
    }

    /// Returns the repository's folder name.
    pub fn name(&self) -> String {
        self.repo.name()
    }

    /// Reload all repository data on a background thread.
    pub fn refresh(&mut self, hide_checkpoints: bool, cx: &mut Context<Self>) {
        self.refresh_with_error(hide_checkpoints, None, cx);
    }

    fn refresh_with_error(
        &mut self,
        hide_checkpoints: bool,
        operation_error: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.loading = true;
        self.generation += 1;
        let generation = self.generation;
        let repo = self.repo.clone();
        self.refresh_status(cx);
        cx.notify();

        cx.spawn(async move |this, cx| {
            let task = cx
                .background_executor()
                .spawn(async move { load_all(&repo, hide_checkpoints) });
            let data = task.await;
            this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.graph_rows = graph::layout(&data.commits);
                this.commits = data.commits;
                this.apply_commit_stats();
                this.branches = data.branches;
                this.remotes = data.remotes;
                this.tags = data.tags;
                this.stashes = data.stashes;
                this.worktrees = data.worktrees;
                this.checkpoints = data.checkpoints;
                this.current_branch = data.current_branch;
                this.unpushed_commits = data.unpushed_commits;
                this.ahead = data.ahead;
                this.behind = data.behind;
                this.last_error = operation_error.or(data.error);
                this.loading = false;
                cx.emit(RepositoryEvent::Refreshed);
                cx.notify();
                this.load_missing_commit_stats(generation, cx);
            })
            .ok();
        })
        .detach();
    }

    /// Copies the known line stats onto the newest commits.
    fn apply_commit_stats(&mut self) {
        for commit in self.commits.iter_mut().take(STATS_LIMIT) {
            if let Some((additions, deletions)) = self.commit_stats.get(&commit.hash) {
                commit.additions = Some(*additions);
                commit.deletions = Some(*deletions);
            }
        }
    }

    /// Loads line stats for the newest commits that do not have them yet.
    /// The history shows first, and the stats follow when they are ready.
    fn load_missing_commit_stats(&mut self, generation: u64, cx: &mut Context<Self>) {
        let missing: Vec<String> = self
            .commits
            .iter()
            .take(STATS_LIMIT)
            .filter(|commit| !self.commit_stats.contains_key(&commit.hash))
            .map(|commit| commit.hash.clone())
            .collect();
        if missing.is_empty() {
            return;
        }
        let repo = self.repo.clone();

        cx.spawn(async move |this, cx| {
            let stats = cx
                .background_executor()
                .spawn(async move { repo.commit_stats(&missing) })
                .await;
            let Ok(stats) = stats else {
                return;
            };
            this.update(cx, |this, cx| {
                this.commit_stats.extend(
                    stats
                        .into_iter()
                        .map(|(hash, additions, deletions)| (hash, (additions, deletions))),
                );
                if this.generation != generation {
                    return;
                }
                this.apply_commit_stats();
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Reloads the working tree status: a fast pass first, then line counts and details.
    pub fn refresh_status(&mut self, cx: &mut Context<Self>) {
        self.status_generation = self.status_generation.wrapping_add(1);
        let generation = self.status_generation;
        let repo = self.repo.clone();
        cx.notify();

        cx.spawn(async move |this, cx| {
            let fast_repo = repo.clone();
            let fast = cx
                .background_executor()
                .spawn(async move { fast_repo.status_fast() })
                .await;
            let Ok(fast) = this.update(cx, |this, cx| {
                if this.status_generation != generation {
                    return None;
                }
                let fast = fast.ok();
                if let Some(status) = &fast {
                    this.status = status.clone();
                    cx.emit(RepositoryEvent::StatusRefreshed);
                    cx.notify();
                }
                Some(fast)
            }) else {
                return;
            };
            let Some(fast) = fast else {
                return;
            };

            let detailed = cx
                .background_executor()
                .spawn(async move {
                    match fast {
                        Some(status) => Ok(repo.status_details(status)),
                        None => repo.status(),
                    }
                })
                .await;
            this.update(cx, |this, cx| {
                if this.status_generation != generation {
                    return;
                }
                match detailed {
                    Ok(status) => this.status = status,
                    Err(error) => {
                        this.last_error = Some(format!("Changes could not refresh: {error}"));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Run one git write operation, then refresh.
    pub fn run_operation(
        &mut self,
        label: &str,
        hide_checkpoints: bool,
        op: impl FnOnce(&GitRepo) -> anyhow::Result<()> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        self.run_operation_with_notifications(
            label,
            OperationPlan {
                route: None,
                hide_checkpoints,
                send_notifications: true,
                refresh: OperationRefresh::All,
                previous_status: None,
            },
            op,
            cx,
        );
    }

    /// Runs a staging operation, showing `mutation` at once and restoring the previous status
    /// when Git fails.
    pub fn run_status_operation(
        &mut self,
        label: &str,
        hide_checkpoints: bool,
        mutation: StatusMutation,
        op: impl FnOnce(&GitRepo) -> anyhow::Result<()> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.busy.is_some() {
            return;
        }
        let previous_status = self.status.clone();
        mutation.apply(&mut self.status);
        self.run_operation_with_notifications(
            label,
            OperationPlan {
                route: None,
                hide_checkpoints,
                send_notifications: true,
                refresh: OperationRefresh::Status,
                previous_status: Some(previous_status),
            },
            op,
            cx,
        );
    }

    /// Runs one Git write operation whose notification names `route`, then refreshes.
    pub fn run_operation_with_route(
        &mut self,
        label: &str,
        route: Option<String>,
        hide_checkpoints: bool,
        op: impl FnOnce(&GitRepo) -> anyhow::Result<()> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        self.run_operation_with_notifications(
            label,
            OperationPlan {
                route,
                hide_checkpoints,
                send_notifications: true,
                refresh: OperationRefresh::All,
                previous_status: None,
            },
            op,
            cx,
        );
    }

    /// Runs one Git operation without a notification, such as an automatic fetch, then refreshes.
    pub fn run_silent_operation(
        &mut self,
        label: &str,
        hide_checkpoints: bool,
        op: impl FnOnce(&GitRepo) -> anyhow::Result<()> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        self.run_operation_with_notifications(
            label,
            OperationPlan {
                route: None,
                hide_checkpoints,
                send_notifications: false,
                refresh: OperationRefresh::All,
                previous_status: None,
            },
            op,
            cx,
        );
    }

    fn run_operation_with_notifications(
        &mut self,
        label: &str,
        plan: OperationPlan,
        op: impl FnOnce(&GitRepo) -> anyhow::Result<()> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.busy.is_some() {
            return;
        }
        let route = if plan.send_notifications {
            plan.route.or_else(|| self.default_operation_route())
        } else {
            None
        };
        self.busy = Some(label.to_string());
        self.active_operation = plan.send_notifications.then(|| RepositoryOperation {
            label: label.to_string(),
            route,
        });
        self.last_error = None;
        let repo = self.repo.clone();
        let label = label.to_string();
        let branch = self.current_branch.clone();
        let body = if branch.is_empty() {
            self.name()
        } else {
            branch
        };
        if plan.send_notifications {
            cx.emit(RepositoryEvent::OperationStarted);
        }
        cx.notify();

        cx.spawn(async move |this, cx| {
            let task = cx.background_executor().spawn(async move { op(&repo) });
            let result = task.await;
            this.update(cx, |this, cx| {
                this.busy = None;
                this.active_operation = None;
                if let Err(error) = result {
                    if let Some(previous_status) = plan.previous_status {
                        this.status = previous_status;
                    }
                    let error = error.to_string();
                    this.last_error = Some(error.clone());
                    if plan.send_notifications {
                        let notification = RepositoryNotification {
                            title: operation_result_title(&label, false),
                            body: error,
                            status: RepositoryNotificationStatus::Failure,
                            created_at: Local::now(),
                            is_read: false,
                        };
                        this.add_notification(notification.clone());
                        cx.emit(RepositoryEvent::OperationFinished(notification));
                    }
                    match plan.refresh {
                        OperationRefresh::All => {
                            this.refresh_with_error(
                                plan.hide_checkpoints,
                                this.last_error.clone(),
                                cx,
                            );
                        }
                        OperationRefresh::Status => this.refresh_status(cx),
                    }
                    cx.notify();
                } else {
                    if plan.send_notifications {
                        let notification = RepositoryNotification {
                            title: operation_result_title(&label, true),
                            body,
                            status: RepositoryNotificationStatus::Success,
                            created_at: Local::now(),
                            is_read: false,
                        };
                        this.add_notification(notification.clone());
                        cx.emit(RepositoryEvent::OperationFinished(notification));
                    }
                    match plan.refresh {
                        OperationRefresh::All => this.refresh(plan.hide_checkpoints, cx),
                        OperationRefresh::Status => this.refresh_status(cx),
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    fn add_notification(&mut self, notification: RepositoryNotification) {
        self.notifications.insert(0, notification);
        self.notifications.truncate(20);
    }

    /// Returns how many notifications the user has not seen.
    pub fn unread_notification_count(&self) -> usize {
        self.notifications
            .iter()
            .filter(|item| !item.is_read)
            .count()
    }

    /// Marks every notification as seen.
    pub fn mark_notifications_read(&mut self, cx: &mut Context<Self>) {
        for notification in &mut self.notifications {
            notification.is_read = true;
        }
        cx.notify();
    }

    /// Removes every notification.
    pub fn clear_notifications(&mut self, cx: &mut Context<Self>) {
        self.notifications.clear();
        cx.notify();
    }

    /// Returns the direction of a fetch for its notification, such as `origin -> main`.
    pub fn fetch_operation_route(&self, all_remotes: bool) -> Option<String> {
        let branch = self.operation_branch()?;
        let source = if all_remotes {
            if self.remotes.len() == 1 {
                self.remotes[0].name.as_str()
            } else {
                "all remotes"
            }
        } else {
            self.preferred_remote_name().unwrap_or("remote")
        };
        Some(format!("{source} -> {branch}"))
    }

    /// Returns the direction of a pull for its notification, such as `origin/main -> main`.
    pub fn pull_operation_route(&self) -> Option<String> {
        let branch = self.operation_branch()?;
        let source = self
            .current_upstream()
            .map(str::to_string)
            .or_else(|| {
                self.preferred_remote_name()
                    .map(|remote| format!("{remote}/{branch}"))
            })
            .unwrap_or_else(|| "upstream".to_string());
        Some(format!("{source} -> {branch}"))
    }

    /// Returns the direction of a push for its notification, such as `main -> origin/main`.
    pub fn push_operation_route(&self) -> Option<String> {
        let branch = self.operation_branch()?;
        let target = self
            .current_upstream()
            .map(str::to_string)
            .or_else(|| {
                self.preferred_remote_name()
                    .map(|remote| format!("{remote}/{branch}"))
            })
            .unwrap_or_else(|| "remote".to_string());
        Some(format!("{branch} -> {target}"))
    }

    fn default_operation_route(&self) -> Option<String> {
        self.operation_branch().map(str::to_string)
    }

    fn operation_branch(&self) -> Option<&str> {
        let branch = self.current_branch.trim();
        (!branch.is_empty()).then_some(branch)
    }

    fn current_upstream(&self) -> Option<&str> {
        self.branches
            .iter()
            .find(|branch| branch.is_current || branch.name == self.current_branch)
            .and_then(|branch| branch.upstream.as_deref())
            .filter(|upstream| !upstream.is_empty())
    }

    fn preferred_remote_name(&self) -> Option<&str> {
        self.current_upstream()
            .and_then(|upstream| upstream.split_once('/').map(|(remote, _)| remote))
            .or_else(|| (self.remotes.len() == 1).then(|| self.remotes[0].name.as_str()))
    }

    /// Returns the number of staged, unstaged and conflicted files.
    pub fn changes_count(&self) -> usize {
        self.status.staged.len() + self.status.unstaged.len() + self.status.conflicted.len()
    }

    /// Load LFS data on a background thread. Runs at most one load at
    /// a time.
    pub fn refresh_lfs(&mut self, cx: &mut Context<Self>) {
        if self.lfs_loading {
            return;
        }
        self.lfs_loading = true;
        let repo = self.repo.clone();
        cx.spawn(async move |this, cx| {
            let task = cx
                .background_executor()
                .spawn(async move { repo.lfs_info() });
            let info = task.await;
            this.update(cx, |this, cx| {
                this.lfs = Some(info);
                this.lfs_loading = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

fn operation_result_title(label: &str, success: bool) -> String {
    let action = match label {
        "Fetching" => "Fetch",
        "Pulling" => "Pull",
        "Pushing" => "Push",
        "Committing" => "Commit",
        "Amending" => "Amend",
        "Merging" => "Merge",
        "Rebasing" => "Rebase",
        "Staging" | "Staging all" => "Stage",
        "Unstaging" | "Unstaging all" => "Unstage",
        "Checking out" => "Checkout",
        "Stashing" => "Stash",
        _ => "Operation",
    };
    if success {
        format!("{action} Complete")
    } else {
        format!("{action} Failed")
    }
}

/// Returns the result of a scoped thread, or the default value when the
/// thread panicked.
fn joined<T: Default>(handle: ScopedJoinHandle<'_, T>) -> T {
    handle.join().unwrap_or_default()
}

/// Loads the repository data. Each git command runs on its own thread,
/// so the total time is close to the time of the slowest command.
fn load_all(repo: &GitRepo, hide_checkpoints: bool) -> LoadedData {
    thread::scope(|scope| {
        let branches = scope.spawn(|| repo.branches().unwrap_or_default());
        let remotes = scope.spawn(|| repo.remotes().unwrap_or_default());
        let tags = scope.spawn(|| repo.tags().unwrap_or_default());
        let stashes = scope.spawn(|| repo.stashes().unwrap_or_default());
        let worktrees = scope.spawn(|| repo.worktrees().unwrap_or_default());
        let checkpoints = scope.spawn(|| repo.checkpoints().unwrap_or_default());
        let current_branch = scope.spawn(|| repo.current_branch().unwrap_or_default());
        let unpushed_commits = scope.spawn(|| repo.unpushed_commit_hashes().unwrap_or_default());

        let mut error = None;
        let mut commits = match repo.log(HISTORY_LIMIT) {
            Ok(commits) => commits,
            Err(e) => {
                error = Some(e.to_string());
                Vec::new()
            }
        };
        if hide_checkpoints {
            commits.retain(|c| !is_checkpoint(c));
        }

        let branches = joined(branches);
        let (ahead, behind) = branches
            .iter()
            .find(|b| b.is_current)
            .map(|b| (b.ahead, b.behind))
            .unwrap_or((0, 0));

        LoadedData {
            commits,
            branches,
            remotes: joined(remotes),
            tags: joined(tags),
            stashes: joined(stashes),
            worktrees: joined(worktrees),
            checkpoints: joined(checkpoints),
            current_branch: joined(current_branch),
            unpushed_commits: joined(unpushed_commits),
            ahead,
            behind,
            error,
        }
    })
}

/// Match the checkpoint commits that the original app hides.
fn is_checkpoint(commit: &Commit) -> bool {
    let message = commit.subject.trim().to_lowercase();
    message.starts_with("checkpoint:")
        || message.starts_with("t3 checkpoint ref=refs/t3/checkpoints/")
        || message.starts_with("synara checkpoint ref=refs/synara/checkpoints/")
        || message == "waku worktree snapshot"
        || message == "waku turn start snapshot"
}
