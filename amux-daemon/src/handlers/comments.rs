//! Review/Comment operations handlers

use crate::git::GitOps;
use crate::review::{CommentLineType, ReviewOps};
use crate::state::SharedState;
use amux_proto::daemon::*;
use tonic::{Response, Status};

/// Create a line comment
pub async fn create_line_comment(
    req: CreateLineCommentRequest,
) -> Result<Response<LineCommentInfo>, Status> {
    let comment = ReviewOps::create_comment(
        &req.repo_id,
        &req.branch,
        &req.file_path,
        req.line_number,
        CommentLineType::from(req.line_type),
        &req.comment,
    )
    .map_err(|e| Status::internal(e.to_string()))?;

    Ok(Response::new(LineCommentInfo {
        id: comment.id,
        repo_id: req.repo_id,
        branch: req.branch,
        file_path: comment.file_path,
        line_number: comment.line_number,
        line_type: i32::from(comment.line_type),
        comment: comment.comment,
        created_at: comment.created_at.timestamp(),
    }))
}

/// Update a line comment
pub async fn update_line_comment(
    state: &SharedState,
    req: UpdateLineCommentRequest,
) -> Result<Response<LineCommentInfo>, Status> {
    // We need to find the comment first to get repo_id and branch
    // For now, we'll iterate through all repos/branches to find it
    // This is not optimal but works for the prototype
    let state = state.read().await;

    for repo in state.repos.values() {
        let git_repo = match GitOps::open(&repo.path) {
            Ok(r) => r,
            Err(_) => continue,
        };

        let worktrees = match GitOps::list_worktrees(&git_repo) {
            Ok(w) => w,
            Err(_) => continue,
        };

        for wt in worktrees {
            if let Ok(updated) =
                ReviewOps::update_comment(&repo.id, &wt.branch, &req.comment_id, &req.comment)
            {
                return Ok(Response::new(LineCommentInfo {
                    id: updated.id,
                    repo_id: repo.id.clone(),
                    branch: wt.branch,
                    file_path: updated.file_path,
                    line_number: updated.line_number,
                    line_type: i32::from(updated.line_type),
                    comment: updated.comment,
                    created_at: updated.created_at.timestamp(),
                }));
            }
        }
    }

    Err(Status::not_found("Comment not found"))
}

/// Delete a line comment
pub async fn delete_line_comment(
    state: &SharedState,
    req: DeleteLineCommentRequest,
) -> Result<Response<Empty>, Status> {
    // Similar search pattern as update
    let state = state.read().await;

    for repo in state.repos.values() {
        let git_repo = match GitOps::open(&repo.path) {
            Ok(r) => r,
            Err(_) => continue,
        };

        let worktrees = match GitOps::list_worktrees(&git_repo) {
            Ok(w) => w,
            Err(_) => continue,
        };

        for wt in worktrees {
            if ReviewOps::delete_comment(&repo.id, &wt.branch, &req.comment_id).is_ok() {
                return Ok(Response::new(Empty {}));
            }
        }
    }

    Err(Status::not_found("Comment not found"))
}

/// List line comments
pub async fn list_line_comments(
    req: ListLineCommentsRequest,
) -> Result<Response<ListLineCommentsResponse>, Status> {
    let comments = ReviewOps::list_comments(&req.repo_id, &req.branch, req.file_path.as_deref())
        .map_err(|e| Status::internal(e.to_string()))?;

    let comment_infos: Vec<LineCommentInfo> = comments
        .into_iter()
        .map(|c| LineCommentInfo {
            id: c.id,
            repo_id: req.repo_id.clone(),
            branch: req.branch.clone(),
            file_path: c.file_path,
            line_number: c.line_number,
            line_type: i32::from(c.line_type),
            comment: c.comment,
            created_at: c.created_at.timestamp(),
        })
        .collect();

    Ok(Response::new(ListLineCommentsResponse {
        comments: comment_infos,
    }))
}
