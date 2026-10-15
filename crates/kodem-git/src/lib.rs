//! The Git workspace's engine: Git commands and their parsed models, repository cloning, GitHub
//! requests, the AI service for commit messages and reviews, and the workspace's saved settings.
//! Nothing here depends on GPUI; the desktop runs these blocking calls on background threads.
pub mod ai;
pub mod git;
pub mod github;
pub mod process;
pub mod settings;
pub mod util;
