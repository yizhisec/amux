//! Repository management handlers

use crate::error::{DaemonError, RepoError};
use crate::repo::{self, Repo};
use crate::state::SharedState;
use amux_proto::daemon::*;
use tonic::{Response, Status};

/// Add a new repository
pub async fn add_repo(
    state: &SharedState,
    req: AddRepoRequest,
) -> Result<Response<RepoInfo>, Status> {
    let path = std::path::PathBuf::from(&req.path);

    // If the path is a worktree, resolve to main repository
    let path = crate::git::GitOps::find_main_repo_path(&path).unwrap_or(path);

    // Create repo - convert RepoError to DaemonError to Status
    let repo = Repo::new(path).map_err(|e| Status::from(DaemonError::from(e)))?;

    // Add to state
    let mut state = state.write().await;
    if state.repos.contains_key(&repo.id) {
        return Err(Status::from(DaemonError::Repo(RepoError::AlreadyExists(
            repo.id.clone(),
        ))));
    }

    let info = RepoInfo {
        id: repo.id.clone(),
        name: repo.name.clone(),
        path: repo.path.to_string_lossy().to_string(),
        session_count: 0,
    };

    state.repos.insert(repo.id.clone(), repo);

    // Save to disk
    let repos: Vec<_> = state.repos.values().cloned().collect();
    drop(state);
    repo::save_repos(&repos).map_err(|e| Status::from(DaemonError::from(e)))?;

    Ok(Response::new(info))
}

/// List all repositories
pub async fn list_repos(state: &SharedState) -> Result<Response<ListReposResponse>, Status> {
    let state = state.read().await;

    let repos: Vec<RepoInfo> = state
        .repos
        .values()
        .map(|r| {
            let session_count = state
                .sessions
                .values()
                .filter(|s| s.repo_id == r.id)
                .count() as i32;

            RepoInfo {
                id: r.id.clone(),
                name: r.name.clone(),
                path: r.path.to_string_lossy().to_string(),
                session_count,
            }
        })
        .collect();

    Ok(Response::new(ListReposResponse { repos }))
}

/// Remove a repository
pub async fn remove_repo(
    state: &SharedState,
    req: RemoveRepoRequest,
) -> Result<Response<Empty>, Status> {
    let mut state = state.write().await;

    // Check if repo has active sessions
    let has_sessions = state.sessions.values().any(|s| s.repo_id == req.id);
    if has_sessions {
        return Err(Status::failed_precondition(
            "Cannot remove repo with active sessions",
        ));
    }

    state
        .repos
        .remove(&req.id)
        .ok_or_else(|| Status::from(DaemonError::Repo(RepoError::NotFound(req.id.clone()))))?;

    // Save to disk
    let repos: Vec<_> = state.repos.values().cloned().collect();
    drop(state);
    repo::save_repos(&repos).map_err(|e| Status::from(DaemonError::from(e)))?;

    Ok(Response::new(Empty {}))
}
