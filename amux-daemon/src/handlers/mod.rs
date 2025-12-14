//! gRPC request handlers organized by domain
//!
//! Each module contains handlers for a specific domain of functionality.
//! The main CcmDaemonService delegates to these handlers.

pub mod attach;
pub mod comments;
pub mod diff;
pub mod events;
pub mod git_status;
pub mod provider;
pub mod repo;
pub mod session;
pub mod todo;
pub mod worktree;

use crate::error::{DaemonError, RepoError};
use crate::git::GitOps;
use crate::repo as repo_mod;
use crate::state::SharedState;
use tonic::Status;
use tracing::warn;

/// Helper to get a repo and open its git repository.
/// If the repo path no longer exists, removes the repo from state and returns an error.
pub async fn get_repo_and_open_git(
    state: &SharedState,
    repo_id: &str,
) -> Result<(repo_mod::Repo, git2::Repository), Status> {
    // First, read the state to get the repo
    let repo = {
        let state_guard = state.read().await;
        state_guard
            .repos
            .get(repo_id)
            .cloned()
            .ok_or_else(|| Status::from(DaemonError::Repo(RepoError::NotFound(repo_id.to_string()))))?
    };

    // Check if the path exists
    if !repo.path.exists() {
        warn!(
            "Repo {} path no longer exists: {:?}, removing from state",
            repo_id, repo.path
        );
        // Remove from state and save
        let mut state_guard = state.write().await;
        state_guard.repos.remove(repo_id);
        let repos: Vec<_> = state_guard.repos.values().cloned().collect();
        drop(state_guard);
        let _ = repo_mod::save_repos(&repos);

        return Err(Status::not_found(format!(
            "Repository path no longer exists: {:?}",
            repo.path
        )));
    }

    // Open the git repository
    let git_repo = GitOps::open(&repo.path).map_err(|e| Status::from(DaemonError::from(e)))?;

    Ok((repo, git_repo))
}
