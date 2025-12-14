//! Worktree management handlers

use crate::error::{DaemonError, RepoError};
use crate::events::EventBroadcaster;
use crate::git::GitOps;
use crate::state::SharedState;
use ccm_proto::daemon::*;
use std::collections::HashSet;
use tonic::{Response, Status};

/// List all worktrees for a repository
pub async fn list_worktrees(
    state: &SharedState,
    req: ListWorktreesRequest,
) -> Result<Response<ListWorktreesResponse>, Status> {
    let state = state.read().await;

    let repo = state
        .repos
        .get(&req.repo_id)
        .ok_or_else(|| Status::from(DaemonError::Repo(RepoError::NotFound(req.repo_id.clone()))))?;

    let git_repo = GitOps::open(&repo.path).map_err(|e| Status::from(DaemonError::from(e)))?;

    // Get worktrees from git
    let git_worktrees =
        GitOps::list_worktrees(&git_repo).map_err(|e| Status::from(DaemonError::from(e)))?;

    // Get all branches
    let branches =
        GitOps::list_branches(&git_repo).map_err(|e| Status::from(DaemonError::from(e)))?;

    // Build response: first include all worktrees (including main), then other branches
    let mut worktrees: Vec<WorktreeInfo> = Vec::new();
    let mut seen_branches: HashSet<String> = HashSet::new();

    // First: add all branches that have worktrees (this ensures main worktree is always included)
    for wt in &git_worktrees {
        let session_count = state
            .sessions
            .values()
            .filter(|s| s.repo_id == req.repo_id && s.branch == wt.branch)
            .count() as i32;

        worktrees.push(WorktreeInfo {
            repo_id: req.repo_id.clone(),
            branch: wt.branch.clone(),
            path: wt.path.to_string_lossy().to_string(),
            is_main: wt.is_main,
            session_count,
        });
        seen_branches.insert(wt.branch.clone());
    }

    // Second: add branches that don't have worktrees yet
    for branch in branches {
        if !seen_branches.contains(&branch) {
            let session_count = state
                .sessions
                .values()
                .filter(|s| s.repo_id == req.repo_id && s.branch == branch)
                .count() as i32;

            worktrees.push(WorktreeInfo {
                repo_id: req.repo_id.clone(),
                branch,
                path: String::new(), // No worktree path
                is_main: false,
                session_count,
            });
        }
    }

    Ok(Response::new(ListWorktreesResponse { worktrees }))
}

/// Create a new worktree
pub async fn create_worktree(
    state: &SharedState,
    events: &EventBroadcaster,
    req: CreateWorktreeRequest,
) -> Result<Response<WorktreeInfo>, Status> {
    let state = state.read().await;

    let repo = state
        .repos
        .get(&req.repo_id)
        .ok_or_else(|| Status::from(DaemonError::Repo(RepoError::NotFound(req.repo_id.clone()))))?;

    let git_repo = GitOps::open(&repo.path).map_err(|e| Status::from(DaemonError::from(e)))?;

    let wt_path = GitOps::create_worktree(
        &git_repo,
        &req.branch,
        &repo.path,
        req.base_branch.as_deref(),
    )
    .map_err(|e| Status::from(DaemonError::from(e)))?;

    let info = WorktreeInfo {
        repo_id: req.repo_id,
        branch: req.branch,
        path: wt_path.to_string_lossy().to_string(),
        is_main: false,
        session_count: 0,
    };

    // Emit worktree added event for multi-instance sync
    events.emit_worktree_added(info.clone());

    Ok(Response::new(info))
}

/// Remove a worktree
pub async fn remove_worktree(
    state: &SharedState,
    events: &EventBroadcaster,
    req: RemoveWorktreeRequest,
) -> Result<Response<Empty>, Status> {
    let state = state.read().await;

    // Check if any sessions exist for this worktree
    let has_sessions = state
        .sessions
        .values()
        .any(|s| s.repo_id == req.repo_id && s.branch == req.branch);

    if has_sessions {
        return Err(Status::failed_precondition(
            "Cannot remove worktree with active sessions",
        ));
    }

    let repo = state
        .repos
        .get(&req.repo_id)
        .ok_or_else(|| Status::from(DaemonError::Repo(RepoError::NotFound(req.repo_id.clone()))))?;

    let git_repo = GitOps::open(&repo.path).map_err(|e| Status::from(DaemonError::from(e)))?;

    GitOps::remove_worktree(&git_repo, &req.branch)
        .map_err(|e| Status::from(DaemonError::from(e)))?;

    // Emit worktree removed event for multi-instance sync
    events.emit_worktree_removed(req.repo_id.clone(), req.branch.clone());

    Ok(Response::new(Empty {}))
}

/// Delete a branch
pub async fn delete_branch(
    state: &SharedState,
    req: DeleteBranchRequest,
) -> Result<Response<Empty>, Status> {
    let state = state.read().await;

    let repo = state
        .repos
        .get(&req.repo_id)
        .ok_or_else(|| Status::from(DaemonError::Repo(RepoError::NotFound(req.repo_id.clone()))))?;

    let git_repo = GitOps::open(&repo.path).map_err(|e| Status::from(DaemonError::from(e)))?;

    GitOps::delete_branch(&git_repo, &req.branch)
        .map_err(|e| Status::from(DaemonError::from(e)))?;

    Ok(Response::new(Empty {}))
}
