//! Saved settings of the Git workspace. Kodem's own settings carry the theme, the accent, the
//! typefaces, the text sizes and the window, so this file keeps only what the Git workspace
//! adds: Git and remote options, diff display, history display, AI, linked Git host accounts,
//! the open repositories and the sizes of the workspace's panes.
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use kodem_core::{Provider, platform};
use serde::{Deserialize, Serialize};

/// Narrowest width of the diff panel.
pub const MIN_DIFF_PANEL_WIDTH: f32 = 280.0;
/// Widest width of the diff panel.
pub const MAX_DIFF_PANEL_WIDTH: f32 = 1_600.0;
/// Default width of the diff panel.
pub const DEFAULT_DIFF_PANEL_WIDTH: f32 = 560.0;
/// Lowest height of the diff panel and the commit details.
pub const MIN_DIFF_PANEL_HEIGHT: f32 = 180.0;
/// Highest height of the diff panel and the commit details.
pub const MAX_DIFF_PANEL_HEIGHT: f32 = 1_000.0;
/// Default height of the diff panel and the commit details.
pub const DEFAULT_DIFF_PANEL_HEIGHT: f32 = 360.0;
/// Narrowest width of the commit file list.
pub const MIN_HISTORY_FILE_PANEL_WIDTH: f32 = 220.0;
/// Widest width of the commit file list.
pub const MAX_HISTORY_FILE_PANEL_WIDTH: f32 = 600.0;
/// Default width of the commit file list.
pub const DEFAULT_HISTORY_FILE_PANEL_WIDTH: f32 = 300.0;

/// File name of the Git workspace's settings in Kodem's data folder.
const SETTINGS_FILE: &str = "git-settings.json";

/// The layout of the changes list.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangesRenderMode {
    #[default]
    Extensions,
    Tree,
}

impl ChangesRenderMode {
    /// All layouts, in the order the interface lists them.
    pub const ALL: [Self; 2] = [Self::Extensions, Self::Tree];

    /// Returns the name of the layout as the interface shows it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Extensions => "Extensions",
            Self::Tree => "Tree",
        }
    }
}

/// All settings of the Git workspace. The file lives in Kodem's data folder. Unknown fields are
/// ignored, so a settings file of the standalone Liquid app loads as well.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    // General
    pub auto_fetch_interval: u32,
    pub default_branch_name: String,
    pub restore_tabs_on_launch: bool,
    pub confirm_destructive_actions: bool,
    pub show_notifications: bool,

    // Git
    pub pull_strategy: PullStrategy,
    pub prune_on_fetch: bool,
    pub fetch_all_remotes: bool,
    pub fetch_submodules: bool,
    pub autostash_before_rebase: bool,
    pub autostash_before_checkout: bool,
    pub autostash_before_merge: bool,
    pub stash_before_worktree_apply: bool,
    pub pull_no_commit: bool,
    pub pull_lfs: bool,
    pub pull_submodules: bool,
    pub set_upstream_on_push: bool,
    pub push_tags: bool,
    pub push_submodules: bool,
    pub merge_no_ff: bool,
    pub signing_method: SigningMethod,
    pub gpg_key_id: String,

    // Diffs
    pub tab_width: u32,
    pub diff_context_lines: u32,
    pub show_line_numbers: bool,
    pub diff_word_wrap: bool,
    pub show_whitespace: bool,
    pub diff_panel_bottom: bool,
    pub commit_title_limit: u32,
    pub changes_render_mode: ChangesRenderMode,

    // History display
    pub show_avatars: bool,
    pub show_branch_badges: bool,
    pub show_commit_stats_badges: bool,
    pub hide_checkpoint_commits: bool,
    pub date_format: DateFormat,

    // Accounts
    pub accounts: Vec<Account>,

    // AI
    /// The provider, account, model and effort that run every AI action, chosen in the Git
    /// workspace. `None` follows Kodem's own choices until the user picks one here.
    pub ai_agent: Option<AiAgent>,
    pub ai_commit_messages: bool,
    pub ai_generate_description: bool,
    pub ai_include_whole_file: bool,
    pub ai_include_unstaged: bool,
    pub ai_commit_prompt: String,
    pub ai_code_review: bool,
    pub ai_review_prompt: String,
    pub ai_explain_diffs: bool,
    pub ai_explain_prompt: String,
    pub ai_pull_request_review: bool,
    pub ai_pull_request_review_prompt: String,

    // Workspace
    pub recent_repositories: Vec<PathBuf>,
    pub open_repositories: Vec<PathBuf>,
    pub active_repository: Option<PathBuf>,
    pub diff_panel_width: f32,
    pub diff_panel_height: f32,
    pub history_detail_height: f32,
    pub history_file_panel_width: f32,

    // Local ignore, per repository path
    pub local_ignored: HashMap<String, Vec<String>>,

    // Onboarding
    /// A settings file from before onboarding existed belongs to a user
    /// who already set Liquid up, so a missing value counts as done.
    #[serde(default = "onboarding_completed_by_default")]
    pub onboarding_completed: bool,
    /// The folder that the clone dialog suggests.
    pub clone_directory: Option<PathBuf>,
}

fn onboarding_completed_by_default() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_fetch_interval: 0,
            default_branch_name: "main".into(),
            restore_tabs_on_launch: true,
            confirm_destructive_actions: true,
            show_notifications: true,

            pull_strategy: PullStrategy::FastForward,
            prune_on_fetch: true,
            fetch_all_remotes: true,
            fetch_submodules: true,
            autostash_before_rebase: false,
            autostash_before_checkout: false,
            autostash_before_merge: false,
            stash_before_worktree_apply: true,
            pull_no_commit: false,
            pull_lfs: true,
            pull_submodules: true,
            set_upstream_on_push: true,
            push_tags: false,
            push_submodules: true,
            merge_no_ff: true,
            signing_method: SigningMethod::None,
            gpg_key_id: String::new(),

            tab_width: 4,
            diff_context_lines: 3,
            show_line_numbers: true,
            diff_word_wrap: false,
            show_whitespace: false,
            diff_panel_bottom: false,
            commit_title_limit: 72,
            changes_render_mode: ChangesRenderMode::default(),

            show_avatars: true,
            show_branch_badges: true,
            show_commit_stats_badges: true,
            hide_checkpoint_commits: true,
            date_format: DateFormat::Merged,

            accounts: Vec::new(),

            ai_agent: None,
            ai_commit_messages: true,
            ai_generate_description: true,
            ai_include_whole_file: false,
            ai_include_unstaged: false,
            ai_commit_prompt: String::new(),
            ai_code_review: true,
            ai_review_prompt: String::new(),
            ai_explain_diffs: true,
            ai_explain_prompt: String::new(),
            ai_pull_request_review: true,
            ai_pull_request_review_prompt: String::new(),

            recent_repositories: Vec::new(),
            open_repositories: Vec::new(),
            active_repository: None,
            diff_panel_width: DEFAULT_DIFF_PANEL_WIDTH,
            diff_panel_height: DEFAULT_DIFF_PANEL_HEIGHT,
            history_detail_height: DEFAULT_DIFF_PANEL_HEIGHT,
            history_file_panel_width: DEFAULT_HISTORY_FILE_PANEL_WIDTH,

            local_ignored: HashMap::new(),

            onboarding_completed: false,
            clone_directory: None,
        }
    }
}

/// How a pull joins remote changes into the current branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PullStrategy {
    FastForward,
    Merge,
    Rebase,
}

impl PullStrategy {
    /// All strategies, in the order the interface lists them.
    pub const ALL: [PullStrategy; 3] = [
        PullStrategy::FastForward,
        PullStrategy::Merge,
        PullStrategy::Rebase,
    ];

    /// Returns the name of the strategy as the interface shows it.
    pub fn label(&self) -> &'static str {
        match self {
            PullStrategy::FastForward => "Fast-forward",
            PullStrategy::Merge => "Merge",
            PullStrategy::Rebase => "Rebase",
        }
    }
}

/// How commits are signed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SigningMethod {
    None,
    Gpg,
    Ssh,
}

impl SigningMethod {
    /// All methods, in the order the interface lists them.
    pub const ALL: [SigningMethod; 3] =
        [SigningMethod::None, SigningMethod::Gpg, SigningMethod::Ssh];

    /// Returns the name of the method as the interface shows it.
    pub fn label(&self) -> &'static str {
        match self {
            SigningMethod::None => "None",
            SigningMethod::Gpg => "GPG",
            SigningMethod::Ssh => "SSH",
        }
    }
}

/// How the history shows commit dates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DateFormat {
    Merged,
    Relative,
    Absolute,
    Iso8601,
}

impl DateFormat {
    /// All formats, in the order the interface lists them.
    pub const ALL: [DateFormat; 4] = [
        DateFormat::Merged,
        DateFormat::Relative,
        DateFormat::Absolute,
        DateFormat::Iso8601,
    ];

    /// Returns the name of the format as the interface shows it.
    pub fn label(&self) -> &'static str {
        match self {
            DateFormat::Merged => "Merged",
            DateFormat::Relative => "Relative",
            DateFormat::Absolute => "Absolute",
            DateFormat::Iso8601 => "ISO 8601",
        }
    }
}

/// The Kodem provider, account, model and reasoning effort that run the Git workspace's AI
/// actions. Kodem keeps its own task options apart, so neither choice changes the other.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AiAgent {
    pub provider: Provider,
    /// Account of the provider; `None` is the provider's default account.
    pub account: Option<String>,
    /// Model by the provider's catalog identity; `None` is the provider's default model.
    pub model: Option<String>,
    /// Reasoning effort; `None` is the model's default effort.
    pub effort: Option<String>,
}

/// A Git host account whose token Git uses for HTTPS remotes on that host, and that the pull
/// request views use for the host's API.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Account {
    pub host: String,
    pub username: String,
    pub email: String,
    pub token: String,
}

// Returns where the Git workspace keeps its settings.
fn settings_path() -> Result<PathBuf> {
    Ok(kodem_core::ipc::data_dir()?.join(SETTINGS_FILE))
}

// Returns where the standalone Liquid app kept its settings, which the first start imports.
fn liquid_settings_path() -> Result<PathBuf> {
    Ok(platform::data_home()?.join("Liquid").join("settings.json"))
}

// Reads one settings file. A missing or unreadable file gives `None`.
fn read(path: &Path) -> Option<Settings> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

impl Settings {
    /// Loads the saved settings. The first start imports the settings of the standalone Liquid
    /// app when they exist, and a new installation starts from the defaults.
    pub fn load() -> Self {
        if let Some(settings) = settings_path().ok().and_then(|path| read(&path)) {
            return settings;
        }
        liquid_settings_path()
            .ok()
            .and_then(|path| read(&path))
            .unwrap_or_default()
    }

    /// Saves the settings. The file holds account tokens, so only its owner can read it, and a
    /// failed write leaves the previous file in place.
    pub fn save(&self) {
        if let Err(error) = self.write() {
            eprintln!("Kodem could not save the Git settings: {error:#}");
        }
    }

    // Writes the settings to a private file beside the saved one and moves it into place.
    fn write(&self) -> Result<()> {
        let path = settings_path()?;
        let directory = path.parent().context("The settings path has no folder")?;
        std::fs::create_dir_all(directory)?;
        let text = serde_json::to_string_pretty(self)?;
        let temporary = path.with_extension("json.tmp");
        let _ = std::fs::remove_file(&temporary);
        let mut file = platform::create_private(&temporary)?;
        std::io::Write::write_all(&mut file, text.as_bytes())?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, &path)?;
        platform::restrict(&path, 0o600)?;
        Ok(())
    }

    /// The folder that the clone dialog suggests: the last one used, or
    /// `~/Developer` when it exists, or the home folder.
    pub fn suggested_clone_directory(&self) -> PathBuf {
        if let Some(directory) = self.clone_directory.as_ref().filter(|path| path.is_dir()) {
            return directory.clone();
        }
        let home = platform::home_dir().unwrap_or_else(|_| PathBuf::from("/"));
        let developer = home.join("Developer");
        if developer.is_dir() { developer } else { home }
    }

    /// Returns the patterns that the user ignores locally in one repository.
    pub fn ignored_for(&self, repo_path: &str) -> &[String] {
        self.local_ignored
            .get(repo_path)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Returns the saved width of the diff panel within its supported range.
    pub fn effective_diff_panel_width(&self) -> f32 {
        self.diff_panel_width
            .clamp(MIN_DIFF_PANEL_WIDTH, MAX_DIFF_PANEL_WIDTH)
    }

    /// Returns the saved height of the diff panel within its supported range.
    pub fn effective_diff_panel_height(&self) -> f32 {
        self.diff_panel_height
            .clamp(MIN_DIFF_PANEL_HEIGHT, MAX_DIFF_PANEL_HEIGHT)
    }

    /// Returns the saved height of the commit details within its supported range.
    pub fn effective_history_detail_height(&self) -> f32 {
        self.history_detail_height
            .clamp(MIN_DIFF_PANEL_HEIGHT, MAX_DIFF_PANEL_HEIGHT)
    }

    /// Returns the saved width of the commit file list within its supported range.
    pub fn effective_history_file_panel_width(&self) -> f32 {
        self.history_file_panel_width
            .clamp(MIN_HISTORY_FILE_PANEL_WIDTH, MAX_HISTORY_FILE_PANEL_WIDTH)
    }
}
