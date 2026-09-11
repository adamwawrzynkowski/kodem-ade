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
    #[serde(default)]
    pub color: Option<u32>,
    #[serde(default)]
    pub icon: ProjectIcon,
    #[serde(default)]
    pub removed: bool,
}
/// Named accents with coordinated foreground colors for both appearances.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum AccentColor {
    #[default]
    Orange,
    Blue,
    Green,
    Purple,
    Pink,
    Teal,
    Amber,
    Gold,
    Lemon,
    Lime,
    Mint,
    Cyan,
    Sky,
    Indigo,
    Lavender,
    Magenta,
    Coral,
    Red,
}
impl AccentColor {
    pub const ALL: [Self; 18] = [
        Self::Orange,
        Self::Blue,
        Self::Green,
        Self::Purple,
        Self::Pink,
        Self::Teal,
        Self::Amber,
        Self::Gold,
        Self::Lemon,
        Self::Lime,
        Self::Mint,
        Self::Cyan,
        Self::Sky,
        Self::Indigo,
        Self::Lavender,
        Self::Magenta,
        Self::Coral,
        Self::Red,
    ];
    /// Returns the label shown in color pickers.
    pub fn label(self) -> &'static str {
        match self {
            Self::Orange => "Orange",
            Self::Blue => "Blue",
            Self::Green => "Green",
            Self::Purple => "Purple",
            Self::Pink => "Pink",
            Self::Teal => "Teal",
            Self::Amber => "Amber",
            Self::Gold => "Gold",
            Self::Lemon => "Lemon",
            Self::Lime => "Lime",
            Self::Mint => "Mint",
            Self::Cyan => "Cyan",
            Self::Sky => "Sky",
            Self::Indigo => "Indigo",
            Self::Lavender => "Lavender",
            Self::Magenta => "Magenta",
            Self::Coral => "Coral",
            Self::Red => "Red",
        }
    }
    /// Returns a readable RGB foreground for the selected appearance.
    pub fn rgb(self, light: bool) -> u32 {
        match (self, light) {
            (Self::Orange, false) => 0xcb864c,
            (Self::Orange, true) => 0x956324,
            (Self::Blue, false) => 0x78aaf0,
            (Self::Blue, true) => 0x2965aa,
            (Self::Green, false) => 0x82bd87,
            (Self::Green, true) => 0x34743b,
            (Self::Purple, false) => 0xb59be6,
            (Self::Purple, true) => 0x7852ac,
            (Self::Pink, false) => 0xdc93b2,
            (Self::Pink, true) => 0xa4416c,
            (Self::Teal, false) => 0x78bebc,
            (Self::Teal, true) => 0x277977,
            (Self::Amber, false) => 0xe5a54f,
            (Self::Amber, true) => 0x966014,
            (Self::Gold, false) => 0xd8ba60,
            (Self::Gold, true) => 0x80651a,
            (Self::Lemon, false) => 0xd4ce66,
            (Self::Lemon, true) => 0x74701a,
            (Self::Lime, false) => 0xa9ca69,
            (Self::Lime, true) => 0x587824,
            (Self::Mint, false) => 0x73cda2,
            (Self::Mint, true) => 0x26764e,
            (Self::Cyan, false) => 0x68c6d8,
            (Self::Cyan, true) => 0x197589,
            (Self::Sky, false) => 0x7ebfea,
            (Self::Sky, true) => 0x326f9e,
            (Self::Indigo, false) => 0x939bea,
            (Self::Indigo, true) => 0x565bad,
            (Self::Lavender, false) => 0xc4a7ec,
            (Self::Lavender, true) => 0x8058a5,
            (Self::Magenta, false) => 0xd983d7,
            (Self::Magenta, true) => 0xa03a9e,
            (Self::Coral, false) => 0xe79983,
            (Self::Coral, true) => 0xa54d37,
            (Self::Red, false) => 0xe58387,
            (Self::Red, true) => 0xac3e48,
        }
    }
}
/// Project glyphs or an imported image stored by the local host.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProjectIcon {
    #[default]
    Auto,
    Folder,
    Code,
    Cube,
    Book,
    Branch,
    Terminal,
    File,
    Image(PathBuf),
}
impl ProjectIcon {
    pub const ALL: [Self; 8] = [
        Self::Auto,
        Self::Folder,
        Self::Code,
        Self::Cube,
        Self::Book,
        Self::Branch,
        Self::Terminal,
        Self::File,
    ];
    /// Returns the picker label for this icon.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Auto => "Automatic",
            Self::Folder => "Folder",
            Self::Code => "Code",
            Self::Cube => "Cube",
            Self::Book => "Book",
            Self::Branch => "Branch",
            Self::Terminal => "Terminal",
            Self::File => "File",
            Self::Image(_) => "Custom image",
        }
    }
    /// Returns the bundled asset, or lets the desktop choose its existing automatic glyph.
    pub fn glyph(&self) -> Option<&'static str> {
        match self {
            Self::Auto | Self::Image(_) => None,
            Self::Folder => Some("folder"),
            Self::Code => Some("code"),
            Self::Cube => Some("cube"),
            Self::Book => Some("book"),
            Self::Branch => Some("branch"),
            Self::Terminal => Some("square-terminal"),
            Self::File => Some("file-text"),
        }
    }
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
/// Execution permissions selected explicitly for this task.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum AccessMode {
    ReadOnly,
    #[default]
    Workspace,
    AutoReview,
    FullAccess,
}
impl AccessMode {
    /// Returns the compact label shown beside the model and effort controls.
    pub fn label(self) -> &'static str {
        match self {
            Self::ReadOnly => "Read only",
            Self::Workspace => "Workspace",
            Self::AutoReview => "AI auto-approve",
            Self::FullAccess => "Full access",
        }
    }
    /// Returns the provider's thread sandbox mode.
    pub fn sandbox(self) -> &'static str {
        match self {
            Self::ReadOnly => "read-only",
            Self::Workspace | Self::AutoReview => "workspace-write",
            Self::FullAccess => "danger-full-access",
        }
    }
    /// Returns the approval policy paired with the selected sandbox.
    pub fn approval_policy(self) -> &'static str {
        match self {
            Self::Workspace | Self::AutoReview => "on-request",
            Self::ReadOnly | Self::FullAccess => "never",
        }
    }
    /// Routes permission escalation to the user or Codex's risk-based AI reviewer.
    pub fn approvals_reviewer(self) -> &'static str {
        match self {
            Self::AutoReview => "auto_review",
            _ => "user",
        }
    }
    /// Returns the explicit per-turn policy, including restricted network defaults.
    pub fn turn_policy(self, cwd: &std::path::Path) -> serde_json::Value {
        match self {
            Self::ReadOnly => serde_json::json!({"type":"readOnly","networkAccess":false}),
            Self::Workspace | Self::AutoReview => {
                serde_json::json!({"type":"workspaceWrite","networkAccess":false,"writableRoots":[cwd]})
            }
            Self::FullAccess => serde_json::json!({"type":"dangerFullAccess"}),
        }
    }
}
/// Diff captured when the provider finished a turn, never refreshed from sidebar polling.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskReview {
    pub snapshot_id: Option<String>,
    pub added: usize,
    pub removed: usize,
    pub omitted_files: usize,
    pub captured: u64,
    pub error: Option<String>,
}
/// Last provider-reported context occupancy, separate from cumulative billed tokens.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContextUsage {
    pub used: u64,
    pub window: Option<u64>,
    pub reported: u64,
}
/// A provider account's measured usage window. Reset timestamps are Unix seconds.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageWindow {
    pub used_percent: i32,
    pub window_duration_mins: Option<u64>,
    pub resets_at: Option<u64>,
}
/// Provider-reported account limits; missing windows are unavailable, not zero usage.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageLimits {
    pub limit_id: Option<String>,
    pub plan_type: Option<String>,
    pub primary: Option<UsageWindow>,
    pub secondary: Option<UsageWindow>,
    pub rate_limit_reached_type: Option<String>,
    pub spend_control_reached: Option<bool>,
    #[serde(default)]
    pub reported: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub project_id: String,
    pub title: String,
    #[serde(default)]
    pub title_pending: bool,
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
    #[serde(default)]
    pub access: AccessMode,
    #[serde(default)]
    pub review: Option<TaskReview>,
    #[serde(default)]
    pub context_usage: Option<ContextUsage>,
    pub pinned: bool,
    #[serde(default)]
    pub settled: Option<bool>,
    /// Last background observation that this task's branch is merged into the default branch.
    #[serde(default)]
    pub branch_merged: bool,
    pub archived: bool,
    pub created: u64,
    pub updated: u64,
    /// Last submitted or started run; viewing and completing a task do not change its recent order.
    #[serde(default)]
    pub last_started: Option<u64>,
    pub feedback: Vec<Feedback>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    pub error: Option<String>,
    /// Async questions remain answerable after the producing turn or host exits.
    #[serde(default)]
    pub questions: Vec<InputRequest>,
}
impl Task {
    /// Reports pending blocking or asynchronous requests for user input.
    pub fn needs_input(&self) -> bool {
        self.state == RunState::Waiting || !self.questions.is_empty()
    }
    /// Resolves manual or merge-based settlement while keeping active or uncertain work visible.
    pub fn is_settled(&self) -> bool {
        !self.state.active()
            && !self.needs_input()
            && self.state != RunState::DeliveryUnknown
            && self.settled.unwrap_or(self.branch_merged)
    }
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FileEdit {
    pub path: String,
    pub lines: Option<EditedLines>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EditedLines {
    pub added: usize,
    pub removed: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub seq: u64,
    pub id: String,
    pub task_id: String,
    pub kind: MessageKind,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    #[serde(default)]
    pub feedback: Vec<Feedback>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub file_edits: Vec<FileEdit>,
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
impl InputRequest {
    /// Identifies questions answered as user messages rather than JSON-RPC responses.
    pub fn is_async(&self) -> bool {
        self.method == "item/agentMessage"
    }

    /// Gives async answers a stable outbox identity across retries and repeated events.
    pub fn answer_submission_id(&self) -> String {
        use sha2::{Digest, Sha256};
        format!(
            "answer-{:x}",
            Sha256::digest(format!("{}:{}", self.task_id, self.id))
        )
    }

    /// Normalizes the runtime's structured async message metadata for the answer form.
    pub fn from_async_message(
        task_id: &str,
        turn_id: &str,
        item: &serde_json::Value,
    ) -> anyhow::Result<Option<Self>> {
        #[derive(Deserialize)]
        struct Question {
            title: String,
            options: Option<Vec<String>>,
        }
        if item["type"] != "agentMessage" || item["questions"].is_null() {
            return Ok(None);
        }
        let questions: Vec<Question> = serde_json::from_value(item["questions"].clone())?;
        if questions.is_empty() {
            return Ok(None);
        }
        let id = item["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Async question has no item ID"))?;
        let questions: Vec<_> = questions
            .into_iter()
            .enumerate()
            .map(|(index, question)| {
                serde_json::json!({"id":index.to_string(),"question":question.title,
                "options":question.options.map(|options| options.into_iter().map(|label|
                    serde_json::json!({"label":label,"description":""})).collect::<Vec<_>>())})
            })
            .collect();
        Ok(Some(Self {
            id: serde_json::json!(format!("{turn_id}:{id}")),
            task_id: task_id.into(),
            method: "item/agentMessage".into(),
            params: serde_json::json!({"questions":questions}),
        }))
    }
}
/// Provider-reported work currently in progress, separate from the saved transcript.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Activity {
    pub id: String,
    pub kind: ActivityKind,
    pub detail: String,
    pub started: u64,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub enum ActivityKind {
    Reconnecting,
    ReadingFiles,
    SearchingFiles,
    Delegating,
    Reviewing,
    ViewingImage,
    GeneratingImage,
    WaitingTimer,
    Reasoning,
    Responding,
    Command,
    EditingFiles,
    Searching,
    Tool,
    Planning,
    Compacting,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskView {
    #[serde(default)]
    pub activities: Vec<Activity>,
    pub task: Task,
    #[serde(default)]
    pub outgoing: Vec<OutgoingMessage>,
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
    #[serde(default)]
    pub usage: Option<UsageLimits>,
    pub models: Vec<Model>,
    pub error: Option<String>,
}
/// Last explicitly chosen agent options, inherited by newly created tasks.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct AgentPreferences {
    pub model: Option<String>,
    pub effort: Option<String>,
    pub access: AccessMode,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub notifications: NotificationSettings,
    pub sidebar_width: f32,
    pub review_width: f32,
    pub collapsed: bool,
    pub comfortable: bool,
    pub font_size: f32,
    pub light: bool,
    pub accent: AccentColor,
    pub sort: Sort,
    pub group_tasks_by_project: bool,
    pub collapsed_projects: std::collections::HashSet<String>,
    pub selected_task: Option<String>,
    #[serde(default)]
    pub diff_placement: DiffPlacement,
    #[serde(default)]
    pub send_mode: SendMode,
    pub agent_defaults: AgentPreferences,
    pub default_worktree: bool,
    pub show_review: bool,
    pub show_terminal: bool,
    pub split_diff: bool,
    pub wrap_diff: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            notifications: NotificationSettings::default(),
            sidebar_width: 260.,
            review_width: 520.,
            collapsed: false,
            comfortable: false,
            font_size: 14.,
            light: false,
            accent: AccentColor::default(),
            sort: Sort::Recent,
            group_tasks_by_project: false,
            collapsed_projects: Default::default(),
            selected_task: None,
            diff_placement: DiffPlacement::default(),
            send_mode: SendMode::default(),
            agent_defaults: AgentPreferences::default(),
            default_worktree: false,
            show_review: true,
            show_terminal: false,
            split_diff: false,
            wrap_diff: true,
        }
    }
}
/// Controls task alerts independently of macOS notification permissions.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct NotificationSettings {
    pub enabled: bool,
    pub attention: bool,
    pub completed: bool,
    pub failed: bool,
    pub interrupted: bool,
    pub play_sound: bool,
    pub sound: NotificationSound,
}
impl Default for NotificationSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            attention: true,
            completed: true,
            failed: true,
            interrupted: true,
            play_sound: true,
            sound: NotificationSound::default(),
        }
    }
}
/// Bundled notification sounds, with stable persisted names.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum NotificationSound {
    #[default]
    Chime,
    Bell,
    Glass,
    Pulse,
    Ascend,
}
impl NotificationSound {
    pub const ALL: [Self; 5] = [
        Self::Chime,
        Self::Bell,
        Self::Glass,
        Self::Pulse,
        Self::Ascend,
    ];

    /// Returns the sound's settings label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Chime => "Chime",
            Self::Bell => "Bell",
            Self::Glass => "Glass",
            Self::Pulse => "Pulse",
            Self::Ascend => "Ascend",
        }
    }

    /// Returns the audio resource name used by macOS Notification Center.
    pub fn filename(self) -> &'static str {
        match self {
            Self::Chime => "kodem-chime.wav",
            Self::Bell => "kodem-bell.wav",
            Self::Glass => "kodem-glass.wav",
            Self::Pulse => "kodem-pulse.wav",
            Self::Ascend => "kodem-ascend.wav",
        }
    }
}
/// Reports submission to Notification Center, without claiming a banner was displayed.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub enum NotificationDelivery {
    #[default]
    Idle,
    Pending,
    Submitted,
    Failed(String),
}
/// Chooses the surface used when opening a changed file.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
pub enum DiffPlacement {
    #[default]
    Conversation,
    RightPanel,
}
/// Chooses how a message submitted during active work is delivered.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
pub enum SendMode {
    #[default]
    Queue,
    Steer,
}
/// Preserves whether imported context is an image, ordinary file or explicit skill invocation.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum AttachmentKind {
    Image,
    File,
    Skill { source: PathBuf },
}
/// Distinguishes a file upload from an explicit skill invocation before importing it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum AttachmentSource {
    File(PathBuf),
    Skill(PathBuf),
}
impl AttachmentSource {
    /// Returns the selected file path for importing and draft display.
    pub fn path(&self) -> &std::path::Path {
        match self {
            Self::File(path) | Self::Skill(path) => path,
        }
    }
}
/// Host-owned immutable content retained with drafts and outgoing messages.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Attachment {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub bytes: u64,
    pub kind: AttachmentKind,
}
/// Delivery state shown in the composer outbox.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub enum DeliveryState {
    Queued,
    Pending,
    Failed,
    Unknown,
}
/// Frozen input and delivery status owned by the host rather than the window.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OutgoingMessage {
    pub id: String,
    pub text: String,
    pub attachments: Vec<Attachment>,
    pub feedback: Vec<Feedback>,
    pub created: u64,
    pub state: DeliveryState,
    pub error: Option<String>,
    pub mode: SendMode,
    pub expected_turn: Option<String>,
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
    #[serde(default)]
    pub notification_delivery: NotificationDelivery,
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
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DiffLine {
    pub old: Option<u32>,
    pub new: Option<u32>,
    pub kind: LineKind,
    pub text: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FileDelta {
    pub path: Vec<u8>,
    pub old_path: Option<Vec<u8>>,
    pub label: String,
    pub status: String,
    pub lines: Vec<DiffLine>,
    pub notice: Option<String>,
    pub digest: String,
}
/// Current task-baseline totals; omitted files have no inline line counts.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChangeSummary {
    pub files: usize,
    pub added: usize,
    pub removed: usize,
    pub omitted_files: usize,
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
impl DiffSnapshot {
    /// Counts changed files and available added/removed lines in this capture.
    pub fn summary(&self) -> ChangeSummary {
        ChangeSummary {
            files: self.files.len(),
            added: self
                .files
                .iter()
                .flat_map(|file| &file.lines)
                .filter(|line| line.kind == LineKind::Added)
                .count(),
            removed: self
                .files
                .iter()
                .flat_map(|file| &file.lines)
                .filter(|line| line.kind == LineKind::Removed)
                .count(),
            omitted_files: self
                .files
                .iter()
                .filter(|file| file.notice.is_some())
                .count(),
        }
    }

    /// Compares captured repository content without treating capture time or storage identity as a change.
    pub fn same_content(&self, other: &Self) -> bool {
        self.task_id == other.task_id
            && self.comparison == other.comparison
            && self.baseline == other.baseline
            && self.index_digest == other.index_digest
            && self.files == other.files
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Feedback {
    #[serde(default)]
    pub first_row: Option<usize>,
    #[serde(default)]
    pub old_range: Option<(u32, u32)>,
    #[serde(default)]
    pub new_range: Option<(u32, u32)>,
    #[serde(default)]
    pub last_row: Option<usize>,
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
/// Composer actions backed by operations advertised by the installed Codex runtime.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ComposerCommand {
    New,
    Model,
    Permissions,
    Skills,
    Mcp,
    Status,
}

/// An enabled skill returned by Codex for the current working directory.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComposerSkill {
    pub name: String,
    pub description: String,
    pub path: PathBuf,
}

/// Runtime-backed composer choices, with partial discovery failures kept visible.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ComposerCatalog {
    pub commands: Vec<ComposerCommand>,
    pub skills: Vec<ComposerSkill>,
    pub errors: Vec<String>,
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
    Access(AccessMode),
    Rename(String),
    Pin(bool),
    Settle(bool),
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
    CustomizeProject {
        project_id: String,
        name: String,
        color: Option<u32>,
        icon: ProjectIcon,
    },
    RemoveProject {
        project_id: String,
    },
    CreateTask {
        project_id: String,
        worktree: bool,
        preferences: AgentPreferences,
    },
    Task {
        task_id: String,
        before: Option<u64>,
    },
    EditTask {
        task_id: String,
        edit: TaskEdit,
    },
    DeleteTask {
        task_id: String,
    },
    TaskChanges {
        task_id: String,
    },
    SaveSettings(Settings),
    TestNotification {
        sound: Option<NotificationSound>,
    },
    Submit {
        task_id: String,
        submission_id: String,
        text: String,
        feedback_ids: Vec<String>,
        #[serde(default)]
        attachment_ids: Vec<String>,
        #[serde(default)]
        mode: SendMode,
    },
    ImportAttachments {
        task_id: String,
        sources: Vec<AttachmentSource>,
    },
    RemoveAttachment {
        task_id: String,
        id: String,
    },
    RestoreOutgoing {
        task_id: String,
        id: String,
    },
    CancelQueued {
        task_id: String,
        id: String,
    },
    SteerQueued {
        task_id: String,
        id: String,
    },
    ResumeQueue {
        task_id: String,
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
        #[serde(default)]
        previous: Option<String>,
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
    AddFeedbackRows {
        task_id: String,
        snapshot_id: String,
        file: usize,
        first_row: usize,
        last_row: usize,
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
    ComposerCatalog {
        request_id: String,
        task_id: Option<String>,
        project_id: Option<String>,
    },
    Resources {
        task_id: Option<String>,
        project_id: Option<String>,
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
    Accepted {
        task_id: String,
        submission_id: String,
    },
    Attachments {
        task_id: String,
        attachments: Vec<Attachment>,
    },
    Workspace(Workspace),
    Task(TaskView),
    Project(Project),
    Created(Task),
    Diff(DiffSnapshot),
    TaskChanges {
        task_id: String,
        summary: ChangeSummary,
    },
    ComposerCatalog(ComposerCatalog),
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
    pub outgoing: Option<OutgoingMessage>,
}
