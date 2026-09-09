//! Persisted domain values and the versioned local IPC contract.
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

/// Returns wall-clock milliseconds for persisted observations.
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
/// Returns a new opaque local identity.
pub fn new_id() -> String {
    Uuid::new_v4().to_string()
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub path: PathBuf,
    pub name: String,
    pub pinned: bool,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub enum RunState {
    #[default]
    Idle,
    Starting,
    Running,
    Waiting,
    Completed,
    Interrupted,
    Failed,
    DeliveryUnknown,
}
impl RunState {
    /// Returns whether the host still owns work or input for this task.
    pub fn active(&self) -> bool {
        matches!(self, Self::Starting | Self::Running | Self::Waiting)
    }
    /// Returns a concise status for the task row.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Idle => "Ready",
            Self::Starting => "Starting",
            Self::Running => "Running",
            Self::Waiting => "Needs input",
            Self::Completed => "Review",
            Self::Interrupted => "Interrupted",
            Self::Failed => "Failed",
            Self::DeliveryUnknown => "Delivery unknown",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub cwd: PathBuf,
    pub owns_worktree: bool,
    pub baseline: Option<String>,
    pub branch: String,
    pub provider_thread: Option<String>,
    pub active_turn: Option<String>,
    pub state: RunState,
    pub draft: String,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub pinned: bool,
    pub archived: bool,
    pub created: u64,
    pub updated: u64,
    pub feedback: Vec<Feedback>,
    pub error: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum MessageKind {
    User,
    Assistant,
    Tool,
    Error,
    Check,
    Notice,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub seq: u64,
    pub id: String,
    pub task_id: String,
    pub kind: MessageKind,
    pub text: String,
    pub complete: bool,
    pub created: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InputRequest {
    pub id: serde_json::Value,
    pub task_id: String,
    pub method: String,
    pub params: serde_json::Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskView {
    pub task: Task,
    pub messages: Vec<Message>,
    pub before: Option<u64>,
    pub requests: Vec<InputRequest>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub name: String,
    pub efforts: Vec<String>,
    pub default_effort: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RuntimeInfo {
    pub version: Option<String>,
    pub authenticated: Option<bool>,
    pub models: Vec<Model>,
    pub error: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub sidebar_width: f32,
    pub review_width: f32,
    pub collapsed: bool,
    pub comfortable: bool,
    pub font_size: f32,
    pub light: bool,
    pub sort: Sort,
    pub selected_task: Option<String>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            sidebar_width: 260.,
            review_width: 520.,
            collapsed: false,
            comfortable: false,
            font_size: 14.,
            light: false,
            sort: Sort::Recent,
            selected_task: None,
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub enum Sort {
    Recent,
    Name,
    Attention,
    Created,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Workspace {
    pub protocol: u32,
    pub projects: Vec<Project>,
    pub tasks: Vec<Task>,
    pub settings: Settings,
    pub runtime: RuntimeInfo,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Comparison {
    WorkingTree,
    Staged,
    Unstaged,
    Baseline,
}
impl Comparison {
    /// Returns the comparison label displayed above the diff.
    pub fn label(&self) -> &'static str {
        match self {
            Self::WorkingTree => "Working tree",
            Self::Staged => "Staged",
            Self::Unstaged => "Unstaged",
            Self::Baseline => "Task baseline",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Side {
    Old,
    New,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum LineKind {
    Context,
    Added,
    Removed,
    Header,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiffLine {
    pub old: Option<u32>,
    pub new: Option<u32>,
    pub kind: LineKind,
    pub text: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileDelta {
    pub path: Vec<u8>,
    pub old_path: Option<Vec<u8>>,
    pub label: String,
    pub status: String,
    pub lines: Vec<DiffLine>,
    pub notice: Option<String>,
    pub digest: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiffSnapshot {
    pub id: String,
    pub task_id: String,
    pub comparison: Comparison,
    pub baseline: Option<String>,
    pub captured: u64,
    pub index_digest: String,
    pub files: Vec<FileDelta>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Feedback {
    pub id: String,
    pub snapshot_id: String,
    pub path: Vec<u8>,
    pub label: String,
    pub side: Side,
    pub start: u32,
    pub end: u32,
    pub excerpt: String,
    pub comment: String,
    pub digest: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Resource {
    pub name: String,
    pub source: String,
    pub scope: String,
    pub status: String,
    pub detail: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TerminalView {
    pub text: String,
    pub running: bool,
    pub exit_code: Option<u32>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TaskEdit {
    Rename(String),
    Pin(bool),
    Archive(bool),
    Draft(String),
    Model {
        model: Option<String>,
        effort: Option<String>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Request {
    Hello,
    ConnectRuntime,
    Reconcile {
        task_id: String,
    },
    OpenProject {
        path: PathBuf,
    },
    CreateTask {
        project_id: String,
        worktree: bool,
    },
    Task {
        task_id: String,
        before: Option<u64>,
    },
    EditTask {
        task_id: String,
        edit: TaskEdit,
    },
    SaveSettings(Settings),
    Submit {
        task_id: String,
        submission_id: String,
        text: String,
        feedback_ids: Vec<String>,
    },
    Interrupt {
        task_id: String,
    },
    Answer {
        task_id: String,
        id: serde_json::Value,
        response: serde_json::Value,
    },
    CaptureDiff {
        task_id: String,
        comparison: Comparison,
    },
    LoadSnapshot {
        id: String,
    },
    AddFeedback {
        task_id: String,
        snapshot_id: String,
        file: usize,
        side: Side,
        start: u32,
        end: u32,
        comment: String,
    },
    RemoveFeedback {
        task_id: String,
        id: String,
    },
    StageFile {
        task_id: String,
        snapshot_id: String,
        file: usize,
    },
    Resources {
        task_id: Option<String>,
    },
    Files {
        task_id: String,
    },
    ReadFile {
        task_id: String,
        path: Vec<u8>,
    },
    Terminal {
        task_id: String,
    },
    TerminalInput {
        task_id: String,
        data: Vec<u8>,
    },
    TerminalResize {
        task_id: String,
        rows: u16,
        cols: u16,
    },
    RunCheck {
        task_id: String,
        command: String,
    },
    Shutdown,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Response {
    Workspace(Workspace),
    Task(TaskView),
    Project(Project),
    Created(Task),
    Diff(DiffSnapshot),
    Resources(Vec<Resource>),
    Files(Vec<Vec<u8>>),
    Text(String),
    Terminal(TerminalView),
    Ok,
    Error(String),
}

#[derive(Clone, Debug)]
pub struct SubmissionRecord {
    pub id: String,
    pub prompt: String,
    pub state: String,
    pub receipt: Option<String>,
    pub feedback: Vec<String>,
}
