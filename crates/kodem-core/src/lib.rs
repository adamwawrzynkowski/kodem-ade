//! Durable workspace operations shared by the desktop and local host.
pub mod git;
pub mod ipc;
pub mod model;
#[cfg(target_os = "macos")]
pub mod notification_presentation;
pub mod providers;
pub mod storage;
pub use model::*;
