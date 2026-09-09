//! Durable workspace operations shared by the desktop and local host.
pub mod git;
pub mod ipc;
pub mod model;
pub mod runtime;
pub mod storage;
pub use model::*;
