//! Diff operations handlers

use super::get_repo_and_open_git;
use crate::diff::DiffOps;
use crate::error::DaemonError;
use crate::git::GitOps;
use crate::state::SharedState;
use amux_proto::daemon::*;
use tonic::{Response, Status};

/// Get list of changed files for a branch
pub async fn get_diff_files(
    state: &SharedState,
    req: GetDiffFilesRequest,
) -> Result<Response<GetDiffFilesResponse>, Status> {
    let (_repo, git_repo) = get_repo_and_open_git(state, &req.repo_id).await?;

    // Find worktree path for the branch
    let worktree_path = GitOps::find_worktree_path(&git_repo, &req.branch).ok_or_else(|| {
        Status::not_found(format!("Worktree not found for branch: {}", req.branch))
    })?;

    // Get diff files
    let diff_files =
        DiffOps::get_diff_files(&worktree_path).map_err(|e| Status::from(DaemonError::from(e)))?;

    let files = diff_files
        .into_iter()
        .map(|f| DiffFileInfo {
            path: f.path,
            status: match f.status {
                crate::diff::FileStatus::Modified => FileStatus::Modified as i32,
                crate::diff::FileStatus::Added => FileStatus::Added as i32,
                crate::diff::FileStatus::Deleted => FileStatus::Deleted as i32,
                crate::diff::FileStatus::Renamed => FileStatus::Renamed as i32,
                crate::diff::FileStatus::Untracked => FileStatus::Untracked as i32,
            },
            additions: f.additions,
            deletions: f.deletions,
        })
        .collect();

    Ok(Response::new(GetDiffFilesResponse { files }))
}

/// Get diff lines for a specific file
pub async fn get_file_diff(
    state: &SharedState,
    req: GetFileDiffRequest,
) -> Result<Response<GetFileDiffResponse>, Status> {
    let (_repo, git_repo) = get_repo_and_open_git(state, &req.repo_id).await?;

    // Find worktree path for the branch
    let worktree_path = GitOps::find_worktree_path(&git_repo, &req.branch).ok_or_else(|| {
        Status::not_found(format!("Worktree not found for branch: {}", req.branch))
    })?;

    // Get diff for file
    let diff_lines = DiffOps::get_file_diff(&worktree_path, &req.file_path)
        .map_err(|e| Status::from(DaemonError::from(e)))?;

    let lines = diff_lines
        .into_iter()
        .map(|l| DiffLine {
            line_type: match l.line_type {
                crate::diff::LineType::Header => LineType::Header as i32,
                crate::diff::LineType::Context => LineType::Context as i32,
                crate::diff::LineType::Addition => LineType::Addition as i32,
                crate::diff::LineType::Deletion => LineType::Deletion as i32,
            },
            content: l.content,
            old_lineno: l.old_lineno,
            new_lineno: l.new_lineno,
        })
        .collect();

    Ok(Response::new(GetFileDiffResponse {
        file_path: req.file_path,
        lines,
    }))
}
