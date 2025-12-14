//! gRPC request handlers organized by domain
//!
//! Each module contains handlers for a specific domain of functionality.
//! The main CcmDaemonService delegates to these handlers.

pub mod attach;
pub mod comments;
pub mod diff;
pub mod events;
pub mod git_status;
pub mod repo;
pub mod session;
pub mod todo;
pub mod worktree;
