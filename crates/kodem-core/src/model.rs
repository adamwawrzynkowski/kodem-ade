//! Persisted domain values and the versioned local IPC contract.
pub use crate::project_icons::ProjectIcon;
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

/// A command-line program that Kodem runs and that the setup dialog can install.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tool {
    Claude,
    Codex,
    Git,
    GitLfs,
    GitHub,
}
impl Tool {
    /// The tools besides the providers' CLIs, in the order that the setup dialog lists them.
    pub const SUPPORT: [Tool; 3] = [Tool::Git, Tool::GitLfs, Tool::GitHub];
    /// Returns the name that the interface shows.
    pub fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
            Self::Git => "Git",
            Self::GitLfs => "Git LFS",
            Self::GitHub => "GitHub CLI",
        }
    }
    /// Returns the CLI that runs a provider.
    pub fn of(provider: Provider) -> Self {
        match provider {
            Provider::Claude => Self::Claude,
            Provider::Codex => Self::Codex,
        }
    }
}
/// What a support tool reported when Kodem ran it. `version` is empty when it did not run.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolVersion {
    pub tool: Tool,
    pub version: Option<String>,
}

/// Integration that owns a conversation and its remote session identity.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Provider {
    #[default]
    Codex,
    Claude,
}
impl Provider {
    pub const ALL: [Self; 2] = [Self::Claude, Self::Codex];
    /// Returns the integration's display name.
    pub fn label(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::Claude => "Claude Code",
        }
    }
    /// Returns the bundled provider logo asset.
    pub fn icon(self) -> &'static str {
        match self {
            Self::Codex => "openai",
            Self::Claude => "claude",
        }
    }
    /// Returns the CLI command used to authenticate this integration.
    pub fn login_command(self) -> &'static str {
        match self {
            Self::Codex => "codex login",
            Self::Claude => "claude auth login",
        }
    }
    /// Returns the CLI command that removes this integration's saved sign-in.
    pub fn logout_command(self) -> &'static str {
        match self {
            Self::Codex => "codex logout",
            Self::Claude => "claude auth logout",
        }
    }
    /// Returns the environment variable that moves the CLI to another configuration directory.
    pub fn home_variable(self) -> Option<&'static str> {
        match self {
            Self::Codex => Some("CODEX_HOME"),
            Self::Claude => Some("CLAUDE_CONFIG_DIR"),
        }
    }
}

/// Identity of the account a provider CLI uses when no configuration directory is set.
pub const DEFAULT_ACCOUNT: &str = "default";

/// One signed-in identity of a provider, isolated by its own CLI configuration directory.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Account {
    pub id: String,
    pub provider: Provider,
    pub name: String,
    /// Directory passed through the provider's home variable; absent for the CLI's own directory.
    #[serde(default)]
    pub home: Option<PathBuf>,
    #[serde(default)]
    pub color: Option<AccentColor>,
    pub created: u64,
}
impl Account {
    /// Reports whether this is the account the CLI uses outside Kodem.
    pub fn is_default(&self) -> bool {
        self.id == DEFAULT_ACCOUNT
    }
}

/// Sign-in and quota state observed from the provider CLI. It never holds credentials.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AccountStatus {
    /// Unknown until the CLI has reported its sign-in state.
    pub signed_in: Option<bool>,
    pub email: Option<String>,
    pub plan: Option<String>,
    pub organization: Option<String>,
    pub usage: Option<UsageLimits>,
    pub error: Option<String>,
    /// A sign-in window was opened and its result is not yet known.
    #[serde(default)]
    pub signing_in: bool,
    /// Wall-clock milliseconds of the last completed check; zero before the first one.
    #[serde(default)]
    pub checked: u64,
}

/// An account together with its last observed status.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AccountState {
    pub account: Account,
    #[serde(default)]
    pub status: AccountStatus,
}

/// A permission mode advertised by a provider, preserved without remapping its semantics.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProviderAccess {
    pub id: String,
    pub name: String,
    pub description: String,
}
impl ProviderAccess {
    /// Reports a provider mode in which the agent only plans.
    pub fn plans(id: &str) -> bool {
        id.to_ascii_lowercase().contains("plan")
    }
}

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
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
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
    /// Provider, account, model and access used for new tasks in this project.
    #[serde(default)]
    pub defaults: Option<AgentPreferences>,
    /// Scripts and apps that a task of this project runs in its terminal, in list order.
    #[serde(default)]
    pub commands: Vec<ProjectCommand>,
}
/// A named sequence of scripts or apps that runs in a task's terminal. Each step starts when the
/// step before it succeeds.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProjectCommand {
    pub id: String,
    /// The name that the command list shows. An empty name shows the first step's file name.
    #[serde(default)]
    pub name: String,
    /// Programs in run order. A program inside the project is relative to the project's root,
    /// so that it runs from the task's own checkout.
    pub steps: Vec<PathBuf>,
}
impl ProjectCommand {
    /// The most steps that one command holds.
    pub const MAX_STEPS: usize = 32;
    /// Returns the name that the command list shows.
    pub fn label(&self) -> String {
        let name = self.name.trim();
        if !name.is_empty() {
            return name.to_owned();
        }
        self.steps
            .first()
            .map_or_else(|| "Untitled command".to_owned(), |step| step_label(step))
    }
    /// Returns each step as an absolute program path. A relative step resolves in the task's
    /// `checkout`, or in the project's root when the checkout does not hold it, such as a file
    /// that Git does not track.
    pub fn programs(&self, checkout: &Path, project: &Path) -> Vec<PathBuf> {
        self.steps
            .iter()
            .map(|step| {
                if step.is_absolute() {
                    return step.clone();
                }
                let own = checkout.join(step);
                if own.exists() {
                    own
                } else {
                    project.join(step)
                }
            })
            .collect()
    }
}
/// Returns the file name of a command step, or the whole step when it has none.
pub fn step_label(step: &Path) -> String {
    step.file_name()
        .unwrap_or(step.as_os_str())
        .to_string_lossy()
        .into_owned()
}
/// Named color themes of the workspace. The window maps each one to its palette.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ThemeId {
    #[default]
    Graphite,
    Adys,
    Monokai,
    Dracula,
    OneDark,
    Nord,
    TokyoNight,
    GruvboxDark,
    Mocha,
    SolarizedDark,
    Paper,
    Latte,
    SolarizedLight,
    GruvboxLight,
    OneLight,
}
impl ThemeId {
    /// Every theme in picker order: the dark themes, then the light ones.
    pub const ALL: [Self; 15] = [
        Self::Graphite,
        Self::Adys,
        Self::Monokai,
        Self::Dracula,
        Self::OneDark,
        Self::Nord,
        Self::TokyoNight,
        Self::GruvboxDark,
        Self::Mocha,
        Self::SolarizedDark,
        Self::Paper,
        Self::Latte,
        Self::SolarizedLight,
        Self::GruvboxLight,
        Self::OneLight,
    ];
    /// Returns the name shown in the theme picker.
    pub fn label(self) -> &'static str {
        match self {
            Self::Graphite => "Graphite",
            Self::Adys => "Adys",
            Self::Monokai => "Monokai",
            Self::Dracula => "Dracula",
            Self::OneDark => "One Dark",
            Self::Nord => "Nord",
            Self::TokyoNight => "Tokyo Night",
            Self::GruvboxDark => "Gruvbox Dark",
            Self::Mocha => "Catppuccin Mocha",
            Self::SolarizedDark => "Solarized Dark",
            Self::Paper => "Paper",
            Self::Latte => "Catppuccin Latte",
            Self::SolarizedLight => "Solarized Light",
            Self::GruvboxLight => "Gruvbox Light",
            Self::OneLight => "One Light",
        }
    }
    /// Reports whether the theme has light surfaces and dark text.
    pub fn light(self) -> bool {
        matches!(
            self,
            Self::Paper | Self::Latte | Self::SolarizedLight | Self::GruvboxLight | Self::OneLight
        )
    }
}
/// Typefaces for the interface and prose. Fira Sans and Inter are bundled; the others are macOS
/// fonts or fonts that the user installed.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum TextFont {
    #[default]
    FiraSans,
    System,
    Inter,
    IbmPlexSans,
    Geist,
    HelveticaNeue,
    AvenirNext,
    Verdana,
    Charter,
    Georgia,
}
impl TextFont {
    pub const ALL: [Self; 10] = [
        Self::FiraSans,
        Self::System,
        Self::Inter,
        Self::IbmPlexSans,
        Self::Geist,
        Self::HelveticaNeue,
        Self::AvenirNext,
        Self::Verdana,
        Self::Charter,
        Self::Georgia,
    ];
    /// Returns the name shown in the font picker.
    pub fn label(self) -> &'static str {
        match self {
            Self::System => "SF Pro",
            other => other.family(),
        }
    }
    /// Returns the family name that the text system loads.
    pub fn family(self) -> &'static str {
        match self {
            Self::FiraSans => "Fira Sans",
            Self::System => ".SystemUIFont",
            Self::Inter => "Inter",
            Self::IbmPlexSans => "IBM Plex Sans",
            Self::Geist => "Geist",
            Self::HelveticaNeue => "Helvetica Neue",
            Self::AvenirNext => "Avenir Next",
            Self::Verdana => "Verdana",
            Self::Charter => "Charter",
            Self::Georgia => "Georgia",
        }
    }
    /// Reports whether the font is always present: bundled with Kodem or the system font.
    pub fn built_in(self) -> bool {
        matches!(self, Self::FiraSans | Self::Inter | Self::System)
    }
}
/// Monospace typefaces for code, diffs, terminal output and instrument labels. IBM Plex Mono,
/// JetBrains Mono and Fira Code are bundled; the others are macOS fonts or fonts that the user
/// installed.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum CodeFont {
    #[default]
    IbmPlexMono,
    JetBrainsMono,
    FiraCode,
    CascadiaCode,
    SourceCodePro,
    GeistMono,
    Hack,
    SfMono,
    Menlo,
    Monaco,
    AndaleMono,
    CourierNew,
}
impl CodeFont {
    pub const ALL: [Self; 12] = [
        Self::IbmPlexMono,
        Self::JetBrainsMono,
        Self::FiraCode,
        Self::CascadiaCode,
        Self::SourceCodePro,
        Self::GeistMono,
        Self::Hack,
        Self::SfMono,
        Self::Menlo,
        Self::Monaco,
        Self::AndaleMono,
        Self::CourierNew,
    ];
    /// Returns the name shown in the font picker, which is also the family name that the text
    /// system loads.
    pub fn family(self) -> &'static str {
        match self {
            Self::IbmPlexMono => "IBM Plex Mono",
            Self::JetBrainsMono => "JetBrains Mono",
            Self::FiraCode => "Fira Code",
            Self::CascadiaCode => "Cascadia Code",
            Self::SourceCodePro => "Source Code Pro",
            Self::GeistMono => "Geist Mono",
            Self::Hack => "Hack",
            Self::SfMono => "SF Mono",
            Self::Menlo => "Menlo",
            Self::Monaco => "Monaco",
            Self::AndaleMono => "Andale Mono",
            Self::CourierNew => "Courier New",
        }
    }
    /// Reports whether the font is bundled with Kodem.
    pub fn built_in(self) -> bool {
        matches!(
            self,
            Self::IbmPlexMono | Self::JetBrainsMono | Self::FiraCode
        )
    }
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
    #[serde(default)]
    pub cost: Option<UsageCost>,
}
/// Session cost reported by the provider in its own currency or credit unit.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UsageCost {
    pub amount: f64,
    pub currency: String,
}
/// A provider account's measured usage window. Reset timestamps are Unix seconds.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageWindow {
    pub used_percent: i32,
    pub window_duration_mins: Option<u64>,
    pub resets_at: Option<u64>,
}
/// Provider-reported credits. The decimal balance is preserved without currency assumptions.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageCredits {
    pub has_credits: bool,
    pub unlimited: bool,
    pub balance: Option<String>,
}
/// Usage credits spent in the current billing period against the cap the provider set.
/// Amounts are in the smallest unit of `currency`, such as cents.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSpend {
    pub used: Option<f64>,
    /// Absent when the account has no cap.
    pub limit: Option<f64>,
    pub used_percent: Option<i32>,
    pub currency: Option<String>,
}
/// Banked resets available to the account; the count may exceed the returned detail rows.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BankedResets {
    pub available_count: i64,
}
/// Result of one idempotent attempt to redeem a banked usage reset.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UsageResetOutcome {
    Reset,
    AlreadyRedeemed,
    NothingToReset,
    NoCredit,
}
/// Provider-reported account limits; missing windows are unavailable, not zero usage.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageLimits {
    pub limit_id: Option<String>,
    pub plan_type: Option<String>,
    pub primary: Option<UsageWindow>,
    pub secondary: Option<UsageWindow>,
    pub credits: Option<UsageCredits>,
    pub banked_resets: Option<BankedResets>,
    pub rate_limit_reached_type: Option<String>,
    pub spend_control_reached: Option<bool>,
    /// Spend of an account that pays for usage with credits, such as a Claude Enterprise seat.
    #[serde(default)]
    pub spend: Option<UsageSpend>,
    #[serde(default)]
    pub reported: u64,
}
impl UsageLimits {
    /// Reports that the provider refuses further use of the account until a reset.
    pub fn limit_reached(&self) -> bool {
        self.spend_control_reached == Some(true) || self.rate_limit_reached_type.is_some()
    }
}
/// What a task is for. The kind selects the workspace that lists the task and the rules of its
/// provider session.
#[derive(
    Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub enum TaskKind {
    /// Agentic work in a project checkout.
    #[default]
    Coding,
    /// A conversation without a project.
    Chat,
    /// A local, read-only review of a pull request or a branch.
    Review,
}
impl TaskKind {
    pub const ALL: [Self; 3] = [Self::Coding, Self::Chat, Self::Review];
    /// Returns the name of the workspace that lists tasks of this kind.
    pub fn label(self) -> &'static str {
        match self {
            Self::Coding => "Agentic Coding",
            Self::Chat => "Chat",
            Self::Review => "Review",
        }
    }
    /// Returns the bundled icon of the workspace.
    pub fn icon(self) -> &'static str {
        match self {
            Self::Coding => "code",
            Self::Chat => "chat",
            Self::Review => "scan-eye",
        }
    }
    /// Returns what one task of this kind is called in the interface.
    pub fn noun(self) -> &'static str {
        match self {
            Self::Coding => "task",
            Self::Chat => "chat",
            Self::Review => "review",
        }
    }
    /// Reports whether tasks of this kind belong to a project and work in a Git checkout.
    pub fn has_project(self) -> bool {
        self != Self::Chat
    }
}
/// A pull request or a branch chosen for review.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum ReviewTarget {
    PullRequest {
        number: u64,
        title: String,
        /// Head branch; a pull request from a fork names its owner first, as `owner:branch`.
        head: String,
        base: String,
        url: String,
    },
    Branch {
        /// Name shown to the user, such as `feature/x` or `origin/feature/x`.
        name: String,
        /// Full reference name, such as `refs/heads/feature/x`.
        reference: String,
    },
}
impl ReviewTarget {
    /// Returns a short name of the target for titles and rows.
    pub fn label(&self) -> String {
        match self {
            Self::PullRequest { number, .. } => format!("PR #{number}"),
            Self::Branch { name, .. } => name.clone(),
        }
    }
    /// Returns the name of the branch whose commits are reviewed.
    pub fn head(&self) -> &str {
        match self {
            Self::PullRequest { head, .. } => head,
            Self::Branch { name, .. } => name,
        }
    }
}
/// A review target with what it resolved to when the review started.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ReviewScope {
    pub target: ReviewTarget,
    /// Reviewed commit. The task's baseline is its merge base with `base`.
    pub head: String,
    /// Name of the branch that the target is compared with.
    pub base: String,
}
/// An open pull request of a project, as GitHub CLI reports it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PullRequestSummary {
    pub number: u64,
    pub title: String,
    pub head: String,
    pub base: String,
    pub author: String,
    pub draft: bool,
    /// Wall-clock milliseconds of the last update; zero when unknown.
    pub updated: u64,
    pub additions: u64,
    pub deletions: u64,
    pub url: String,
}
/// A local or remote-tracking branch of a project.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BranchSummary {
    pub name: String,
    pub reference: String,
    pub remote: bool,
    /// The branch that the project's checkout is on.
    pub current: bool,
    pub commit: String,
    pub subject: String,
    pub author: String,
    /// Wall-clock milliseconds of the last commit.
    pub updated: u64,
}
/// What can be reviewed in one project. Pull requests need GitHub CLI; branches do not.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ReviewTargets {
    pub project_id: String,
    /// Branch that reviewed branches are compared with; absent when the repository has none.
    pub default_branch: Option<String>,
    pub branches: Vec<BranchSummary>,
    pub pull_requests: Vec<PullRequestSummary>,
    /// Why pull requests could not be listed, such as a missing or signed-out GitHub CLI.
    pub pull_request_error: Option<String>,
    /// The checkout has uncommitted changes, which a review of its branch does not include.
    pub dirty: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Task {
    #[serde(default)]
    pub subagents: Vec<Subagent>,
    /// Main turn outcome held until its descendants have stopped working.
    #[serde(default)]
    pub pending_completion: Option<TurnOutcome>,
    #[serde(default)]
    pub provider: Provider,
    /// Account that runs this task's turns; absent selects the provider's default account.
    #[serde(default)]
    pub account: Option<String>,
    /// The agent's current step list, replaced whenever the provider reports a new one.
    #[serde(default)]
    pub plan: Vec<PlanStep>,
    #[serde(default)]
    pub provider_access: Option<String>,
    #[serde(default)]
    pub kind: TaskKind,
    /// What a review task examines; absent for the other kinds.
    #[serde(default)]
    pub review_scope: Option<ReviewScope>,
    pub id: String,
    /// Owning project; empty for a chat, which has none.
    pub project_id: String,
    pub title: String,
    #[serde(default)]
    pub title_pending: bool,
    pub cwd: PathBuf,
    pub owns_worktree: bool,
    pub baseline: Option<String>,
    /// Linked checkout commits observed when the task was created.
    #[serde(default)]
    pub repository_baselines: Vec<RepositoryBaseline>,
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
    /// Set while the next run has to start with the earlier conversation as context.
    #[serde(default)]
    pub handoff: Option<Handoff>,
    /// The task that this one was forked from.
    #[serde(default)]
    pub lineage: Option<Lineage>,
}
impl Task {
    /// Reports a delegated task that may write only to a worktree of its own or not at all.
    ///
    /// Such a task reads the checkout of its parent while the parent works, so it neither
    /// blocks another task of that checkout nor is blocked by one.
    pub fn shares_checkout(&self) -> bool {
        self.lineage
            .as_ref()
            .is_some_and(|lineage| lineage.delegated)
            && !self.owns_worktree
    }
    /// Returns the identity of the account that runs this task.
    pub fn account_id(&self) -> &str {
        self.account.as_deref().unwrap_or(DEFAULT_ACCOUNT)
    }
    /// Reports pending blocking or asynchronous requests for user input.
    pub fn needs_input(&self) -> bool {
        self.state == RunState::Waiting || !self.questions.is_empty()
    }
    /// Reports a failure or uncertain delivery that the user has to resolve.
    pub fn is_blocked(&self) -> bool {
        matches!(self.state, RunState::Failed | RunState::DeliveryUnknown)
    }
    /// Reports a finished turn whose result arrived after the task was last open.
    pub fn is_unread(&self, viewed: Option<u64>) -> bool {
        matches!(self.state, RunState::Completed | RunState::Interrupted)
            && self.last_started.is_some()
            && viewed.is_none_or(|viewed| viewed < self.updated)
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
    Subagent,
}

/// A provider child thread, owned by a task but excluded from task navigation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Subagent {
    #[serde(default)]
    pub history_loaded: bool,
    pub thread_id: String,
    pub parent_thread_id: String,
    pub name: String,
    pub state: RunState,
    pub active_turn: Option<String>,
}
impl Subagent {
    /// Returns the transcript storage identity without creating a sidebar task.
    pub fn transcript_id(&self, task_id: &str) -> String {
        format!("{task_id}/agent/{}", self.thread_id)
    }
}

/// Why a run exists.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum RunOrigin {
    /// A message of the user started it.
    #[default]
    User,
    /// The provider continued by itself, for example after a background agent reported.
    Continuation,
    /// Another task handed it over.
    Delegated,
}

/// One counted turn of a task, from the claim of its first message to its published outcome.
///
/// A run is the unit that the provider turn, the checkout capture and the outcome belong to. A
/// message that steers a running turn joins its run. A compaction that the host asked for is a
/// provider turn but not a run. A run is open while `ended` is absent.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Run {
    pub id: String,
    pub task_id: String,
    /// Position among the runs of the task, starting at 1.
    pub ordinal: u32,
    #[serde(default)]
    pub origin: RunOrigin,
    /// The submission that started the run, then those that steered it, in order.
    #[serde(default)]
    pub submissions: Vec<String>,
    pub state: RunState,
    pub provider: Provider,
    /// Provider conversation that ran it, once the provider named one.
    #[serde(default)]
    pub provider_thread: Option<String>,
    /// Provider turn that ran it, once the provider acknowledged or reported one.
    #[serde(default)]
    pub turn: Option<String>,
    /// `seq` of the first transcript row of the run. Rows of the task's own transcript from
    /// this one up to the first row of the next run belong to it. Zero until the store saved it.
    #[serde(default)]
    pub first_seq: u64,
    pub started: u64,
    #[serde(default)]
    pub ended: Option<u64>,
    /// Name of the checkpoint that holds the files of the checkout as the run left them.
    #[serde(default)]
    pub checkpoint: Option<String>,
    /// When the user undid the run. Its rows stay in the transcript but are no longer part of
    /// the conversation that the provider continues.
    #[serde(default)]
    pub undone: Option<u64>,
}

/// Returns the run that a row of a task's own transcript belongs to, by the row's `seq`.
///
/// `runs` are in order. A row that is older than the first recorded run belongs to none.
pub fn run_at(runs: &[Run], seq: u64) -> Option<&Run> {
    runs.iter()
        .rev()
        .find(|run| run.first_seq != 0 && run.first_seq <= seq)
}

/// The agent on one side of a transfer: a provider, its model, and the task when it is another.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AgentRef {
    pub provider: Provider,
    /// Identity of the model in the provider's catalog; absent before the catalog was read.
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub task_id: Option<String>,
}

/// What kind of transfer a transcript row marks.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum TransferKind {
    /// The task continues with another provider.
    Handoff,
    /// A new task starts from the conversation of another task.
    Fork,
    /// An agent gave a piece of work to another task.
    Delegated,
    /// A delegated task returned its result.
    Result,
}
impl TransferKind {
    /// Returns the words that name the transfer in the conversation.
    pub fn label(self) -> &'static str {
        match self {
            Self::Handoff => "Context handoff",
            Self::Fork => "Forked context",
            Self::Delegated => "Delegated task",
            Self::Result => "Subagent result",
        }
    }
}

/// A transfer of conversation context from one agent to another.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Transfer {
    pub kind: TransferKind,
    pub from: AgentRef,
    pub to: AgentRef,
}

/// Why the next run of a task starts in a provider conversation that has not seen the task.
///
/// The host then gives that conversation the earlier messages as context, with the reason.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Handoff {
    /// The files were restored to the checkpoint after this turn; zero is before the first.
    Restored { turn: u32 },
    /// Another provider ran the earlier turns of this task.
    Switched { from: AgentRef },
    /// The task is a fork: its conversation is that of the task in `from`, up to this turn.
    Forked { from: AgentRef, turn: u32 },
}

/// Where a task came from when another task made it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Lineage {
    pub parent: String,
    /// Last turn of the parent that the task continues from.
    pub turn: u32,
    /// An agent of the parent gave this task a piece of work, as opposed to a fork by the user.
    #[serde(default)]
    pub delegated: bool,
}

/// A model that an agent can give work to, as the provider's catalog lists it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AgentOption {
    pub provider: Provider,
    pub model: String,
    pub name: String,
    /// The model that the provider's tasks use when none is chosen.
    pub default: bool,
}

/// The state of a delegated task, as its parent's agent reads it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Delegation {
    pub task_id: String,
    /// Display name of the agent that does the work.
    pub agent: String,
    pub state: RunState,
    /// The task stopped working and no message waits: `result` is final.
    pub done: bool,
    /// The task waits for an approval or an answer of the user.
    pub needs_input: bool,
    /// The replies of the task's last turn.
    pub result: Option<String>,
    pub error: Option<String>,
    /// Branch and directory of the task's own worktree, when it may write.
    pub worktree: Option<(String, PathBuf)>,
}

/// Provider outcome awaiting completion of the task's whole agent tree.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TurnOutcome {
    pub state: RunState,
    pub error: Option<String>,
}

/// One bounded page of an independently stored child transcript.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubagentView {
    pub task_id: String,
    pub agent: Subagent,
    pub messages: Vec<Message>,
    pub before: Option<u64>,
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
    /// Structured form of a tool message. When present, `text` holds only the tool's output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<ToolCall>,
    /// Set on the row that marks a transfer of the conversation from one agent to another.
    /// `text` then holds the same statement in words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transfer: Option<Transfer>,
    pub text: String,
    pub complete: bool,
    pub created: u64,
}
/// Selects a tool call's icon, summary wording and detail layout.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum ToolCategory {
    Command,
    Read,
    Edit,
    Search,
    Fetch,
    Agent,
    Plan,
    Thinking,
    Mcp,
    #[default]
    Other,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum ToolStatus {
    #[default]
    Running,
    Completed,
    Failed,
    Declined,
}
/// One tool invocation in a form the transcript can summarize without parsing text.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ToolCall {
    /// The provider's own tool name, such as `Bash` or `mcp__server__tool`.
    pub name: String,
    pub category: ToolCategory,
    /// The subject of the call: a command, a path, a query or a short description.
    pub title: String,
    /// Secondary context, such as a search scope or the tool's own description of the call.
    #[serde(default)]
    pub detail: String,
    pub status: ToolStatus,
    #[serde(default)]
    pub exit: Option<i64>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    /// Changed lines of an edit, in display order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diff: Vec<DiffLine>,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum PlanStatus {
    #[default]
    Pending,
    Active,
    Done,
}
/// One step of the agent's current plan.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PlanStep {
    pub text: String,
    #[serde(default)]
    pub status: PlanStatus,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InputRequest {
    pub id: serde_json::Value,
    pub task_id: String,
    pub method: String,
    pub params: serde_json::Value,
}
impl InputRequest {
    /// Reports a request to approve an action or a plan, as opposed to questions to answer.
    pub fn is_approval(&self) -> bool {
        self.method.ends_with("requestApproval")
    }
    /// Returns the provider conversation that asked, which is a subagent's when it is not the
    /// main one. A request that names none belongs to the main conversation.
    pub fn conversation(&self) -> Option<&str> {
        self.params["threadId"].as_str()
    }
    /// Returns the decisions that the provider listed for an approval, in its order.
    pub fn offered_decisions(&self) -> Option<Vec<String>> {
        self.params["availableDecisions"]
            .as_array()
            .map(|decisions| {
                decisions
                    .iter()
                    .filter_map(|decision| decision.as_str().map(str::to_owned))
                    .collect()
            })
    }
    /// Returns the decisions to show for an approval: those the provider listed, or the three
    /// that every provider accepts.
    pub fn decisions(&self) -> Vec<String> {
        self.offered_decisions()
            .unwrap_or_else(|| vec!["accept".into(), "decline".into(), "cancel".into()])
    }
    /// Returns the provider's own wording of a decision, when it gave one.
    pub fn decision_label(&self, decision: &str) -> Option<&str> {
        self.params["decisionLabels"][decision].as_str()
    }
    /// Returns why the provider asks.
    pub fn reason(&self) -> Option<&str> {
        self.params["reason"].as_str()
    }
    /// Returns the command that an approval is about.
    pub fn command(&self) -> Option<&str> {
        self.params["command"].as_str()
    }
    /// Returns the name of the tool that an approval is about.
    pub fn tool(&self) -> Option<&str> {
        self.params["tool"].as_str()
    }
    /// Returns the text of a plan that is up for review. It can be empty.
    pub fn plan(&self) -> Option<&str> {
        self.params["plan"].as_str()
    }
    /// Returns the file that the provider wrote a reviewed plan to.
    pub fn plan_path(&self) -> Option<&str> {
        self.params["planFilePath"].as_str()
    }
    /// Returns the questions to answer. Each holds an `id`, a `question` and optional `options`.
    pub fn questions(&self) -> Option<&Vec<serde_json::Value>> {
        self.params["questions"].as_array()
    }
    /// Returns everything the provider sent with the request, as text for inspection.
    pub fn details(&self) -> String {
        serde_json::to_string_pretty(&self.params).unwrap_or_default()
    }
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
    /// The runs of the task, oldest first.
    #[serde(default)]
    pub runs: Vec<Run>,
    /// Identity of the host process that produced this view.
    #[serde(default)]
    pub epoch: String,
    /// Order in which that host produced its views. A view with a smaller number is older, so
    /// replies that arrive on separate connections can be put in order.
    #[serde(default)]
    pub serial: u64,
}

/// What a watcher of one task last received from one host process.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Cursor {
    /// Identity of the host process; a cursor means nothing to another one.
    pub epoch: String,
    /// Last transcript revision of that host which the watcher received.
    pub rev: u64,
    /// Digest of the task state other than its transcript rows.
    pub state: u64,
}

/// How the answer to a watch request relates to the cursor of the request.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SubscriptionStart {
    /// The request had no cursor of this host: `state` holds the newest transcript page.
    Snapshot { state: TaskView, through: Cursor },
    /// `state` holds only the transcript rows that changed after the cursor of the request.
    Replay { state: TaskView, through: Cursor },
    /// More changed than the host retains or one answer carries: `current` holds the newest
    /// transcript page in place of the changed rows.
    ReplayGap { current: TaskView, through: Cursor },
}
impl SubscriptionStart {
    /// Returns the view and the cursor to send with the next watch request.
    pub fn into_parts(self) -> (TaskView, Cursor) {
        match self {
            Self::Snapshot { state, through } | Self::Replay { state, through } => (state, through),
            Self::ReplayGap { current, through } => (current, through),
        }
    }
}
impl TaskView {
    /// Returns accepted input already in chat while its idle provider session starts.
    pub fn starting_submission(&self) -> Option<&OutgoingMessage> {
        if !matches!(self.task.state, RunState::Idle | RunState::Completed) {
            return None;
        }
        let outgoing = self
            .outgoing
            .iter()
            .find(|message| message.state == DeliveryState::Queued)?;
        let id = format!("submission-{}", outgoing.id);
        self.messages
            .iter()
            .any(|message| message.id == id)
            .then_some(outgoing)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub name: String,
    pub efforts: Vec<String>,
    pub default_effort: String,
    #[serde(default)]
    pub images: bool,
    /// Native model variants for providers that encode reasoning in the model identity.
    #[serde(default)]
    pub effort_models: std::collections::BTreeMap<String, String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RuntimeInfo {
    #[serde(default)]
    pub default_model: Option<String>,
    #[serde(default)]
    pub access_modes: Vec<ProviderAccess>,
    #[serde(default)]
    pub steering: bool,
    #[serde(default)]
    pub images: bool,
    #[serde(default)]
    pub files: bool,
    #[serde(default)]
    pub account_usage: bool,
    #[serde(default)]
    pub usage_resets: bool,
    pub version: Option<String>,
    pub models: Vec<Model>,
    pub error: Option<String>,
}
/// The model and effort last chosen for one account, in general or within one project.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ModelChoice {
    pub model: Option<String>,
    pub effort: Option<String>,
}
/// The approval rule last chosen for one model, shared by every workspace and project.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct AccessChoice {
    pub provider_access: Option<String>,
    pub access: AccessMode,
}
/// When a task's context is compacted without being asked.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct CompactRule {
    pub enabled: bool,
    /// Share of the context window, in percent, at which compaction starts.
    pub threshold: u8,
}
impl CompactRule {
    pub const MIN_THRESHOLD: u8 = 30;
    pub const MAX_THRESHOLD: u8 = 95;
    /// Returns the rule with its threshold inside the supported range.
    pub fn bounded(self) -> Self {
        Self {
            enabled: self.enabled,
            threshold: self
                .threshold
                .clamp(Self::MIN_THRESHOLD, Self::MAX_THRESHOLD),
        }
    }
}
impl Default for CompactRule {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold: 80,
        }
    }
}
/// Automatic compaction rules: one for every task, with overrides per provider and per model.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AutoCompact {
    pub default: CompactRule,
    pub providers: std::collections::BTreeMap<Provider, CompactRule>,
    /// Keyed by [`AutoCompact::model_key`].
    pub models: std::collections::BTreeMap<String, CompactRule>,
}
impl AutoCompact {
    /// Returns the key of one provider's model in `models`.
    pub fn model_key(provider: Provider, model: &str) -> String {
        format!("{}/{model}", provider.icon())
    }
    /// Returns the rule for a task: its model's override, then its provider's, then the default.
    pub fn rule(&self, provider: Provider, model: Option<&str>) -> CompactRule {
        model
            .and_then(|model| self.models.get(&Self::model_key(provider, model)))
            .or_else(|| self.providers.get(&provider))
            .copied()
            .unwrap_or(self.default)
            .bounded()
    }
}
/// Last explicitly chosen agent options, inherited by newly created tasks.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct AgentPreferences {
    #[serde(default)]
    pub provider: Provider,
    /// Account of `provider`; absent selects that provider's default account.
    #[serde(default)]
    pub account: Option<String>,
    #[serde(default)]
    pub provider_access: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub access: AccessMode,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Host-owned account registry. Saving window preferences never replaces it.
    pub accounts: Vec<Account>,
    pub provider_preferences: std::collections::BTreeMap<Provider, AgentPreferences>,
    /// Host-owned. Keyed by [`Settings::model_choice_key`].
    pub model_choices: std::collections::BTreeMap<String, ModelChoice>,
    /// Host-owned. Keyed by [`AutoCompact::model_key`].
    pub access_choices: std::collections::BTreeMap<String, AccessChoice>,
    pub auto_compact: AutoCompact,
    /// Turns on Artifacts in Claude Code sessions, which the CLI keeps off outside its own
    /// terminal. The `/design` and `/slides` commands need them.
    pub claude_artifacts: bool,
    /// The setup dialog opened once by itself. Later it opens only on request.
    pub onboarded: bool,
    pub notifications: NotificationSettings,
    /// Keeps the computer from sleeping on its own while an agent starts or runs a turn. The
    /// host holds the lock, so it works with the window closed. The display can still sleep.
    pub prevent_sleep: bool,
    pub sidebar_width: f32,
    pub review_width: f32,
    pub collapsed: bool,
    pub comfortable: bool,
    /// Size of interface and prose text, in points. Every text size follows it in proportion.
    pub font_size: f32,
    /// Size of code text, in points: code blocks, diffs, files and terminal output.
    pub code_font_size: f32,
    pub text_font: TextFont,
    pub code_font: CodeFont,
    /// Scale of the interface geometry: spacing, controls and icons. Text keeps its own sizes.
    pub ui_scale: f32,
    pub theme: ThemeId,
    pub accent: AccentColor,
    pub sort: Sort,
    pub group_tasks_by_project: bool,
    pub collapsed_projects: std::collections::HashSet<String>,
    pub selected_task: Option<String>,
    /// The project that a new task starts in, as it was chosen last.
    pub selected_project: Option<String>,
    /// The workspace that was open last.
    pub mode: TaskKind,
    /// The Git workspace was open last. It covers the workspace of `mode` while it is open.
    pub git_workspace: bool,
    /// When each task was last open in the window, in wall-clock milliseconds. A finished task
    /// whose result is newer than this is still unread.
    pub viewed: std::collections::BTreeMap<String, u64>,
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
            prevent_sleep: true,
            sidebar_width: 260.,
            review_width: 520.,
            collapsed: false,
            comfortable: false,
            font_size: Self::FONT_SIZE,
            code_font_size: Self::CODE_FONT_SIZE,
            text_font: TextFont::default(),
            code_font: CodeFont::default(),
            ui_scale: 1.,
            theme: ThemeId::default(),
            accent: AccentColor::default(),
            sort: Sort::Recent,
            group_tasks_by_project: false,
            collapsed_projects: Default::default(),
            selected_task: None,
            selected_project: None,
            mode: TaskKind::default(),
            git_workspace: false,
            viewed: Default::default(),
            diff_placement: DiffPlacement::default(),
            send_mode: SendMode::default(),
            agent_defaults: AgentPreferences::default(),
            provider_preferences: Default::default(),
            model_choices: Default::default(),
            access_choices: Default::default(),
            auto_compact: AutoCompact::default(),
            claude_artifacts: false,
            onboarded: false,
            accounts: Vec::new(),
            default_worktree: false,
            show_review: true,
            show_terminal: false,
            split_diff: false,
            wrap_diff: true,
        }
    }
}
impl Settings {
    /// Default size of interface and prose text, in points.
    pub const FONT_SIZE: f32 = 14.;
    /// Smallest and largest size of interface and prose text.
    pub const FONT_SIZE_RANGE: (f32, f32) = (12., 22.);
    /// Default size of code text, in points.
    pub const CODE_FONT_SIZE: f32 = 12.;
    /// Smallest and largest size of code text.
    pub const CODE_FONT_SIZE_RANGE: (f32, f32) = (10., 20.);
    /// Smallest and largest interface scale.
    pub const UI_SCALE_RANGE: (f32, f32) = (0.6, 1.4);
    /// Reads saved settings. Settings from before the named themes hold a light switch only;
    /// a light one selects Paper.
    pub fn from_json(json: &str) -> serde_json::Result<Self> {
        let mut value: serde_json::Value = serde_json::from_str(json)?;
        if let Some(fields) = value.as_object_mut()
            && !fields.contains_key("theme")
            && fields.get("light").and_then(serde_json::Value::as_bool) == Some(true)
        {
            fields.insert("theme".into(), serde_json::to_value(ThemeId::Paper)?);
        }
        serde_json::from_value(value)
    }
    /// Returns the key of an account's last model, in one project or in general.
    pub fn model_choice_key(
        provider: Provider,
        account: Option<&str>,
        project: Option<&str>,
    ) -> String {
        let account = account.unwrap_or(DEFAULT_ACCOUNT);
        match project {
            Some(project) => format!("{}/{account}/{project}", provider.icon()),
            None => format!("{}/{account}", provider.icon()),
        }
    }
    /// Returns the model last chosen for an account: in this project first, then anywhere.
    pub fn model_choice(
        &self,
        provider: Provider,
        account: Option<&str>,
        project: Option<&str>,
    ) -> Option<&ModelChoice> {
        project
            .and_then(|project| {
                self.model_choices
                    .get(&Self::model_choice_key(provider, account, Some(project)))
            })
            .or_else(|| {
                self.model_choices
                    .get(&Self::model_choice_key(provider, account, None))
            })
    }
    /// Records the model of `preferences` as the last choice of its account and of `project`.
    pub fn remember_model(&mut self, preferences: &AgentPreferences, project: Option<&str>) {
        let choice = ModelChoice {
            model: preferences.model.clone(),
            effort: preferences.effort.clone(),
        };
        let account = preferences.account.as_deref();
        if let Some(project) = project {
            self.model_choices.insert(
                Self::model_choice_key(preferences.provider, account, Some(project)),
                choice.clone(),
            );
        }
        self.model_choices.insert(
            Self::model_choice_key(preferences.provider, account, None),
            choice,
        );
    }
    /// Makes `defaults` the options of the next new task and of their provider.
    pub fn keep_agent_preferences(&mut self, defaults: AgentPreferences) {
        self.provider_preferences
            .entry(self.agent_defaults.provider)
            .or_insert_with(|| self.agent_defaults.clone());
        self.provider_preferences
            .insert(defaults.provider, defaults.clone());
        self.remember_model(&defaults, None);
        self.agent_defaults = defaults;
    }
    /// Returns the approval rule last chosen for a model, in any workspace or project.
    pub fn access_choice(&self, provider: Provider, model: Option<&str>) -> Option<&AccessChoice> {
        self.access_choices
            .get(&AutoCompact::model_key(provider, model?))
    }
    /// Records the approval rule of `preferences` as the last choice for its model.
    ///
    /// A plan mode is a mode of work and not an approval rule, so it is not recorded.
    pub fn remember_access(&mut self, preferences: &AgentPreferences) {
        let Some(model) = &preferences.model else {
            return;
        };
        if preferences
            .provider_access
            .as_deref()
            .is_some_and(ProviderAccess::plans)
        {
            return;
        }
        self.access_choices.insert(
            AutoCompact::model_key(preferences.provider, model),
            AccessChoice {
                provider_access: preferences.provider_access.clone(),
                access: preferences.access,
            },
        );
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
    /// Shows banners and plays sounds while Kodem is the active application.
    pub while_focused: bool,
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
            while_focused: true,
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
/// Preserves whether imported context is an image, ordinary file, referenced folder or explicit
/// skill invocation. The agent reads a folder in place: its `Attachment::path` is the folder,
/// not a copy.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum AttachmentKind {
    Image,
    File,
    Folder,
    Skill { source: PathBuf },
}
/// Distinguishes a file or folder upload from an explicit skill invocation before importing it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum AttachmentSource {
    File(PathBuf),
    Skill(PathBuf),
}
impl AttachmentSource {
    /// Returns the selected file or folder path for importing and draft display.
    pub fn path(&self) -> &std::path::Path {
        match self {
            Self::File(path) | Self::Skill(path) => path,
        }
    }
}
/// Host-owned immutable content retained with drafts and outgoing messages. A folder is the
/// exception: it stays where it is and is attached by reference.
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
    /// Identities of the projects whose folder has no Git repository of its own. The host checks
    /// the folders each time it sends the workspace, so a repository made later is noticed.
    #[serde(default)]
    pub without_git: std::collections::BTreeSet<String>,
    pub tasks: Vec<Task>,
    pub settings: Settings,
    // Older hosts must deserialize so connect_or_start can negotiate an idle upgrade.
    #[serde(default)]
    pub runtimes: std::collections::BTreeMap<Provider, RuntimeInfo>,
    /// Every provider account with its last observed sign-in and quota state.
    #[serde(default)]
    pub accounts: Vec<AccountState>,
}
impl Workspace {
    /// Finds an account and its status; an absent identity selects the provider's default account.
    pub fn account(&self, provider: Provider, id: Option<&str>) -> Option<&AccountState> {
        let id = id.unwrap_or(DEFAULT_ACCOUNT);
        self.accounts
            .iter()
            .find(|state| state.account.provider == provider && state.account.id == id)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Comparison {
    WorkingTree,
    Staged,
    Unstaged,
    Baseline,
    Branch,
}
impl Comparison {
    /// Returns the comparison label displayed above the diff.
    pub fn label(&self) -> &'static str {
        match self {
            Self::WorkingTree => "Working tree",
            Self::Staged => "Staged",
            Self::Unstaged => "Unstaged",
            Self::Baseline => "Task baseline",
            Self::Branch => "All branch changes",
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
    /// Owning checkout; absent only in snapshots written before multi-repository support.
    #[serde(default)]
    pub repository: Option<PathBuf>,
    pub path: Vec<u8>,
    pub old_path: Option<Vec<u8>>,
    pub label: String,
    pub status: String,
    pub lines: Vec<DiffLine>,
    pub notice: Option<String>,
    pub digest: String,
}
/// A task's starting commit in one linked checkout, including an unborn repository.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RepositoryBaseline {
    pub root: PathBuf,
    pub head: Option<String>,
}
/// Repository identity and index state captured alongside its file deltas.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RepositorySnapshot {
    pub root: PathBuf,
    pub label: String,
    pub baseline: Option<String>,
    /// Merge base with the default branch; absent on that branch or when unavailable.
    #[serde(default)]
    pub branch_base: Option<String>,
    pub index_digest: String,
    pub notice: Option<String>,
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
    #[serde(default)]
    pub repositories: Vec<RepositorySnapshot>,
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
            && self.repositories == other.repositories
            && self.files == other.files
    }
}
/// Both sides of one captured file, read so that its working-tree side can be edited.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditableFile {
    /// The text that the snapshot's comparison compares the file with; empty for a new file.
    pub before: String,
    /// The text of the file in the working tree.
    pub after: String,
    /// The SHA-256 digest of the working-tree bytes, which a save must still find on disk.
    pub digest: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Feedback {
    /// Owning checkout for repository-relative source paths.
    #[serde(default)]
    pub repository: Option<PathBuf>,
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
/// Composer actions backed by operations advertised by the selected provider.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ComposerCommand {
    Provider,
    New,
    Model,
    Permissions,
    Skills,
    Mcp,
    Status,
    Compact,
    /// Restores the files to the checkpoint from before the last turn.
    Undo,
    /// Starts a new task in its own worktree from the files and conversation of this one.
    Fork,
}

/// An enabled skill returned by the provider for the current working directory.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComposerSkill {
    pub name: String,
    pub description: String,
    pub path: PathBuf,
}

/// Runtime-backed composer choices, with partial discovery failures kept visible.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ComposerCatalog {
    #[serde(default)]
    pub provider_commands: Vec<ProviderCommand>,
    pub commands: Vec<ComposerCommand>,
    pub skills: Vec<ComposerSkill>,
    pub errors: Vec<String>,
}

/// A native slash command advertised by the current provider session.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderCommand {
    pub name: String,
    pub description: String,
    pub input_hint: Option<String>,
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
    Provider(AgentPreferences),
    /// Moves the task to another account of its provider; absent selects the default account.
    Account(Option<String>),
    ProviderAccess(String),
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
/// What opening a project does with a folder that is not the root of a Git repository.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum MissingRepository {
    /// Adds nothing and answers with `Response::NoRepository`, so the user can choose.
    Ask,
    /// Creates an empty repository in the folder, then adds it.
    Initialize,
    /// Adds the folder without Git. Its tasks have no worktrees, branches or tracked changes.
    OpenAnyway,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Request {
    SubagentThread {
        task_id: String,
        thread_id: String,
        before: Option<u64>,
    },
    Hello,
    RefreshUsage {
        provider: Provider,
        #[serde(default)]
        account: Option<String>,
    },
    ConsumeUsageReset {
        provider: Provider,
        #[serde(default)]
        account: Option<String>,
        idempotency_key: String,
    },
    ConnectRuntime {
        provider: Provider,
        #[serde(default)]
        account: Option<String>,
    },
    SetAgentPreferences(AgentPreferences),
    /// Saves options as `SetAgentPreferences` does, and records their approval rule as the
    /// choice for their model.
    SetAccessChoice(AgentPreferences),
    /// Registers another account of a provider with its own configuration directory.
    AddAccount {
        provider: Provider,
        name: String,
    },
    EditAccount {
        provider: Provider,
        id: String,
        name: String,
        color: Option<AccentColor>,
    },
    /// Forgets an added account and deletes its configuration directory.
    RemoveAccount {
        provider: Provider,
        id: String,
    },
    /// Runs each support tool to read its version. Answers with `Response::Tools`.
    InspectTools,
    /// Opens a terminal window that runs the official installer of a tool.
    InstallTool {
        tool: Tool,
    },
    /// Opens the provider CLI's own sign-in for this account.
    SignInAccount {
        provider: Provider,
        id: String,
    },
    SignOutAccount {
        provider: Provider,
        id: String,
    },
    /// Reads sign-in state and quotas again without starting a model turn.
    RefreshAccount {
        provider: Provider,
        id: String,
    },
    /// Sets or clears the options that new tasks in one project start with.
    SetProjectDefaults {
        project_id: String,
        defaults: Option<AgentPreferences>,
    },
    Reconcile {
        task_id: String,
    },
    /// Adds the folder as a project. `missing_repository` decides what happens when the folder
    /// is not the root of a Git repository.
    OpenProject {
        path: PathBuf,
        missing_repository: MissingRepository,
    },
    CustomizeProject {
        project_id: String,
        name: String,
        color: Option<u32>,
        icon: ProjectIcon,
        #[serde(default)]
        commands: Vec<ProjectCommand>,
    },
    RemoveProject {
        project_id: String,
    },
    CreateTask {
        project_id: String,
        worktree: bool,
        preferences: AgentPreferences,
        /// Commit that the new worktree starts from instead of the project's current commit.
        #[serde(default)]
        base: Option<String>,
    },
    /// Starts a conversation that has no project.
    CreateChat {
        preferences: AgentPreferences,
    },
    /// Lists the pull requests and branches of a project that can be reviewed.
    ReviewTargets {
        project_id: String,
    },
    /// Starts a local, read-only review in a detached worktree at the target's head.
    CreateReview {
        project_id: String,
        target: ReviewTarget,
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
    // Boxed: the settings are far larger than any other request.
    SaveSettings(Box<Settings>),
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
    /// Summarizes the task's conversation so far to free its context window.
    CompactTask {
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
    /// Reads the old side of one snapshot file and its working-tree text for editing.
    ReadEditableFile {
        task_id: String,
        snapshot_id: String,
        file: usize,
    },
    /// Writes `text` over a working-tree file of the task, in a repository of the capture
    /// `snapshot_id`, when the file still has the content digest `expected`.
    SaveFile {
        task_id: String,
        snapshot_id: String,
        repository: Option<PathBuf>,
        path: Vec<u8>,
        expected: String,
        text: String,
    },
    ComposerCatalog {
        provider: Provider,
        #[serde(default)]
        account: Option<String>,
        model: Option<String>,
        request_id: String,
        task_id: Option<String>,
        project_id: Option<String>,
    },
    Resources {
        provider: Provider,
        #[serde(default)]
        account: Option<String>,
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
    /// Types a project command into the task's terminal, which starts when it is not running,
    /// after sizing the terminal to `rows` and `cols`.
    RunProjectCommand {
        task_id: String,
        command_id: String,
        rows: u16,
        cols: u16,
    },
    Shutdown,
    /// Restores the files of a coding task to the checkpoint from before its last turn that is
    /// not undone yet, and starts the provider conversation again without that turn.
    UndoTurn {
        task_id: String,
    },
    /// Lists the models that an agent of the given task can give work to.
    Agents {
        parent: String,
    },
    /// Starts a task for an agent of `parent` on any provider and model, and sends it `prompt`.
    ///
    /// A task that may not write reads the checkout of its parent. One that may write gets a
    /// worktree of its own with the files as the parent has them now.
    Delegate {
        parent: String,
        prompt: String,
        provider: Option<Provider>,
        /// A model by identity or name; spaces and punctuation do not matter.
        model: Option<String>,
        write: bool,
        /// How long to wait for the result before answering with the task still working.
        wait_ms: u64,
    },
    /// Reads a delegated task, waiting up to `wait_ms` for it to finish.
    DelegateStatus {
        parent: String,
        task_id: String,
        wait_ms: u64,
    },
    /// Stops the running turn of a delegated task.
    DelegateCancel {
        parent: String,
        task_id: String,
    },
    /// Starts a new coding task in a worktree of its own, with the files that the last turn of
    /// the given task left and with its conversation as context for the first message.
    ForkTask {
        task_id: String,
    },
    /// Waits until the task differs from what `after` describes, then answers with the change.
    ///
    /// Without a cursor the answer is immediate. With one, an unchanged task is answered after
    /// a bounded wait, so the request never outlives the read timeout of its connection.
    Watch {
        task_id: String,
        after: Option<Cursor>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Response {
    Watched(SubscriptionStart),
    Agents(Vec<AgentOption>),
    Delegated(Delegation),
    Usage {
        provider: Provider,
        account: String,
        usage: UsageLimits,
    },
    UsageReset {
        provider: Provider,
        account: String,
        outcome: UsageResetOutcome,
        usage: Option<UsageLimits>,
        refresh_error: Option<String>,
    },
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
    SubagentThread(SubagentView),
    Project(Project),
    /// The canonical folder that `Request::OpenProject` did not add, because it has no Git
    /// repository of its own.
    NoRepository(PathBuf),
    Account(Account),
    Created(Task),
    ReviewTargets(ReviewTargets),
    Diff(DiffSnapshot),
    EditableFile(EditableFile),
    TaskChanges {
        task_id: String,
        summary: ChangeSummary,
    },
    ComposerCatalog(ComposerCatalog),
    Resources(Vec<Resource>),
    Files(Vec<Vec<u8>>),
    Text(String),
    Terminal(TerminalView),
    Tools(Vec<ToolVersion>),
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
