//! The Git workspace: a full Git client in the window's fourth workspace. It opens repositories
//! side by side, shows their history, changes, branches and pull requests, and runs Git with
//! AI help for commit messages and reviews. It began as the standalone Liquid app; its views
//! draw with Kodem's theme, surfaces and motion, and its repositories are Kodem's projects.
pub mod account_link;
pub mod ai;
pub mod changes;
pub mod clone_repository;
pub mod components;
pub mod contextual_actions;
pub mod diff;
pub mod history;
pub mod icons;
pub mod kit;
pub mod motion;
pub mod onboarding;
pub mod pull_requests;
pub mod ref_drag;
pub mod remote_actions;
pub mod settings;
pub mod settings_view;
pub mod sidebar;
pub mod state;
pub mod syntax;
pub mod theme;
pub mod workspace;

pub use workspace::{GitEvent, GitWorkspace};

use gpui::App;

/// Loads the Git workspace's settings.
pub fn init(cx: &mut App) {
    settings::GitSettings::init(cx);
}
