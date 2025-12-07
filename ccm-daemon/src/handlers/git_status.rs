//! Git status operations handlers

use crate::error::{DaemonError, RepoError};
use crate::git::GitOps;
use crate::state::SharedState;
use ccm_proto::daemon::*;
use tonic::{Response, Status};

/// Get git status for a worktree
pub async fn get_git_status(
    state: &SharedState,
    req: GetGitStatusRequest,
) -> Result<Response<GetGitStatusResponse>, Status> {
    let state = state.read().await;

    let repo = state
        .repos
        .get(&req.repo_id)
        .ok_or_else(|| Status::from(DaemonError::Repo(RepoError::NotFound(req.repo_id.clone()))))?;

    let git_repo = GitOps::open(&repo.path).map_err(|e| Status::from(DaemonError::from(e)))?;

    // Find worktree path for the branch
    let worktree_path = GitOps::find_worktree_path(&git_repo, &req.branch).ok_or_else(|| {
        Status::not_found(format!("Worktree not found for branch: {}", req.branch))
    })?;

    // Open the worktree repository
    let wt_repo = GitOps::open(&worktree_path).map_err(|e| Status::from(DaemonError::from(e)))?;

    // Get git status
    let status_result =
        GitOps::get_status(&wt_repo).map_err(|e| Status::from(DaemonError::from(e)))?;

    // Convert to proto types
    let to_proto_file = |f: crate::git::GitStatusFile| -> ccm_proto::daemon::GitStatusFile {
        ccm_proto::daemon::GitStatusFile {
            path: f.path,
            status: match f.status {
                crate::git::GitFileStatus::Modified => FileStatus::Modified as i32,
                crate::git::GitFileStatus::Added => FileStatus::Added as i32,
                crate::git::GitFileStatus::Deleted => FileStatus::Deleted as i32,
                crate::git::GitFileStatus::Renamed => FileStatus::Renamed as i32,
                crate::git::GitFileStatus::Untracked => FileStatus::Untracked as i32,
            },
        }
    };

    Ok(Response::new(GetGitStatusResponse {
        staged: status_result
            .staged
            .into_iter()
            .map(to_proto_file)
            .collect(),
        unstaged: status_result
            .unstaged
            .into_iter()
            .map(to_proto_file)
            .collect(),
        untracked: status_result
            .untracked
            .into_iter()
            .map(to_proto_file)
            .collect(),
    }))
}

/// Stage a file
pub async fn stage_file(
    state: &SharedState,
    req: StageFileRequest,
) -> Result<Response<Empty>, Status> {
    let state = state.read().await;

    let repo = state
        .repos
        .get(&req.repo_id)
        .ok_or_else(|| Status::from(DaemonError::Repo(RepoError::NotFound(req.repo_id.clone()))))?;

    let git_repo = GitOps::open(&repo.path).map_err(|e| Status::from(DaemonError::from(e)))?;

    // Find worktree path for the branch
    let worktree_path = GitOps::find_worktree_path(&git_repo, &req.branch).ok_or_else(|| {
        Status::not_found(format!("Worktree not found for branch: {}", req.branch))
    })?;

    // Open the worktree repository
    let wt_repo = GitOps::open(&worktree_path).map_err(|e| Status::from(DaemonError::from(e)))?;

    // Stage the file
    GitOps::stage_file(&wt_repo, &req.file_path).map_err(|e| Status::from(DaemonError::from(e)))?;

    Ok(Response::new(Empty {}))
}

/// Unstage a file
pub async fn unstage_file(
    state: &SharedState,
    req: UnstageFileRequest,
) -> Result<Response<Empty>, Status> {
    let state = state.read().await;

    let repo = state
        .repos
        .get(&req.repo_id)
        .ok_or_else(|| Status::from(DaemonError::Repo(RepoError::NotFound(req.repo_id.clone()))))?;

    let git_repo = GitOps::open(&repo.path).map_err(|e| Status::from(DaemonError::from(e)))?;

    // Find worktree path for the branch
    let worktree_path = GitOps::find_worktree_path(&git_repo, &req.branch).ok_or_else(|| {
        Status::not_found(format!("Worktree not found for branch: {}", req.branch))
    })?;

    // Open the worktree repository
    let wt_repo = GitOps::open(&worktree_path).map_err(|e| Status::from(DaemonError::from(e)))?;

    // Unstage the file
    GitOps::unstage_file(&wt_repo, &req.file_path)
        .map_err(|e| Status::from(DaemonError::from(e)))?;

    Ok(Response::new(Empty {}))
}

/// Stage all files
pub async fn stage_all(
    state: &SharedState,
    req: StageAllRequest,
) -> Result<Response<Empty>, Status> {
    let state = state.read().await;

    let repo = state
        .repos
        .get(&req.repo_id)
        .ok_or_else(|| Status::from(DaemonError::Repo(RepoError::NotFound(req.repo_id.clone()))))?;

    let git_repo = GitOps::open(&repo.path).map_err(|e| Status::from(DaemonError::from(e)))?;

    // Find worktree path for the branch
    let worktree_path = GitOps::find_worktree_path(&git_repo, &req.branch).ok_or_else(|| {
        Status::not_found(format!("Worktree not found for branch: {}", req.branch))
    })?;

    // Open the worktree repository
    let wt_repo = GitOps::open(&worktree_path).map_err(|e| Status::from(DaemonError::from(e)))?;

    // Stage all files
    GitOps::stage_all(&wt_repo).map_err(|e| Status::from(DaemonError::from(e)))?;

    Ok(Response::new(Empty {}))
}

/// Unstage all files
pub async fn unstage_all(
    state: &SharedState,
    req: UnstageAllRequest,
) -> Result<Response<Empty>, Status> {
    let state = state.read().await;

    let repo = state
        .repos
        .get(&req.repo_id)
        .ok_or_else(|| Status::from(DaemonError::Repo(RepoError::NotFound(req.repo_id.clone()))))?;

    let git_repo = GitOps::open(&repo.path).map_err(|e| Status::from(DaemonError::from(e)))?;

    // Find worktree path for the branch
    let worktree_path = GitOps::find_worktree_path(&git_repo, &req.branch).ok_or_else(|| {
        Status::not_found(format!("Worktree not found for branch: {}", req.branch))
    })?;

    // Open the worktree repository
    let wt_repo = GitOps::open(&worktree_path).map_err(|e| Status::from(DaemonError::from(e)))?;

    // Unstage all files
    GitOps::unstage_all(&wt_repo).map_err(|e| Status::from(DaemonError::from(e)))?;

    Ok(Response::new(Empty {}))
}
