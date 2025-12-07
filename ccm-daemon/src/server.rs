//! gRPC server implementation

use crate::diff::DiffOps;
use crate::error::{DaemonError, RepoError, SessionError};
use crate::events::EventBroadcaster;
use crate::git::GitOps;
use crate::persistence;
use crate::repo::{self, Repo};
use crate::review::{CommentLineType, ReviewOps};
use crate::session::{self, Session, SessionStatus};
use crate::state::SharedState;
use ccm_proto::daemon::ccm_daemon_server::CcmDaemon;
use ccm_proto::daemon::*;
use std::pin::Pin;
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream, Stream, StreamExt};
use tonic::{Request, Response, Status, Streaming};

/// CCM Daemon gRPC service
pub struct CcmDaemonService {
    state: SharedState,
    events: EventBroadcaster,
}

impl CcmDaemonService {
    pub fn new(state: SharedState, events: EventBroadcaster) -> Self {
        Self { state, events }
    }
}

#[tonic::async_trait]
impl CcmDaemon for CcmDaemonService {
    // ============ Repo Management ============

    async fn add_repo(
        &self,
        request: Request<AddRepoRequest>,
    ) -> Result<Response<RepoInfo>, Status> {
        let req = request.into_inner();
        let path = std::path::PathBuf::from(&req.path);

        // If the path is a worktree, resolve to main repository
        let path = crate::git::GitOps::find_main_repo_path(&path).unwrap_or(path);

        // Create repo - convert RepoError to DaemonError to Status
        let repo = Repo::new(path).map_err(|e| Status::from(DaemonError::from(e)))?;

        // Add to state
        let mut state = self.state.write().await;
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

    async fn list_repos(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<ListReposResponse>, Status> {
        let state = self.state.read().await;

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

    async fn remove_repo(
        &self,
        request: Request<RemoveRepoRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();

        let mut state = self.state.write().await;

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

    // ============ Worktree Management ============

    async fn list_worktrees(
        &self,
        request: Request<ListWorktreesRequest>,
    ) -> Result<Response<ListWorktreesResponse>, Status> {
        let req = request.into_inner();
        let state = self.state.read().await;

        let repo = state.repos.get(&req.repo_id).ok_or_else(|| {
            Status::from(DaemonError::Repo(RepoError::NotFound(req.repo_id.clone())))
        })?;

        let git_repo = GitOps::open(&repo.path).map_err(|e| Status::from(DaemonError::from(e)))?;

        // Get worktrees from git
        let git_worktrees =
            GitOps::list_worktrees(&git_repo).map_err(|e| Status::from(DaemonError::from(e)))?;

        // Get all branches
        let branches =
            GitOps::list_branches(&git_repo).map_err(|e| Status::from(DaemonError::from(e)))?;

        // Build response: include all branches, mark those with worktrees
        let mut worktrees: Vec<WorktreeInfo> = Vec::new();

        for branch in branches {
            let wt = git_worktrees.iter().find(|w| w.branch == branch);
            let session_count = state
                .sessions
                .values()
                .filter(|s| s.repo_id == req.repo_id && s.branch == branch)
                .count() as i32;

            worktrees.push(WorktreeInfo {
                repo_id: req.repo_id.clone(),
                branch: branch.clone(),
                path: wt
                    .map(|w| w.path.to_string_lossy().to_string())
                    .unwrap_or_default(),
                is_main: wt.map(|w| w.is_main).unwrap_or(false),
                session_count,
            });
        }

        Ok(Response::new(ListWorktreesResponse { worktrees }))
    }

    async fn create_worktree(
        &self,
        request: Request<CreateWorktreeRequest>,
    ) -> Result<Response<WorktreeInfo>, Status> {
        let req = request.into_inner();
        let state = self.state.read().await;

        let repo = state.repos.get(&req.repo_id).ok_or_else(|| {
            Status::from(DaemonError::Repo(RepoError::NotFound(req.repo_id.clone())))
        })?;

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
        self.events.emit_worktree_added(info.clone());

        Ok(Response::new(info))
    }

    async fn remove_worktree(
        &self,
        request: Request<RemoveWorktreeRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let state = self.state.read().await;

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

        let repo = state.repos.get(&req.repo_id).ok_or_else(|| {
            Status::from(DaemonError::Repo(RepoError::NotFound(req.repo_id.clone())))
        })?;

        let git_repo = GitOps::open(&repo.path).map_err(|e| Status::from(DaemonError::from(e)))?;

        GitOps::remove_worktree(&git_repo, &req.branch)
            .map_err(|e| Status::from(DaemonError::from(e)))?;

        // Emit worktree removed event for multi-instance sync
        self.events
            .emit_worktree_removed(req.repo_id.clone(), req.branch.clone());

        Ok(Response::new(Empty {}))
    }

    async fn delete_branch(
        &self,
        request: Request<DeleteBranchRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let state = self.state.read().await;

        let repo = state.repos.get(&req.repo_id).ok_or_else(|| {
            Status::from(DaemonError::Repo(RepoError::NotFound(req.repo_id.clone())))
        })?;

        let git_repo = GitOps::open(&repo.path).map_err(|e| Status::from(DaemonError::from(e)))?;

        GitOps::delete_branch(&git_repo, &req.branch)
            .map_err(|e| Status::from(DaemonError::from(e)))?;

        Ok(Response::new(Empty {}))
    }

    // ============ Session Management ============

    async fn list_sessions(
        &self,
        request: Request<ListSessionsRequest>,
    ) -> Result<Response<ListSessionsResponse>, Status> {
        let req = request.into_inner();

        // Try to update session names from Claude's first message
        {
            let mut state = self.state.write().await;
            for session in state.sessions.values_mut() {
                if !session.name_updated_from_claude {
                    session.update_name_from_claude();
                    if session.name_updated_from_claude {
                        let _ = persistence::save_session_meta(session);
                    }
                }
            }
        }

        let state = self.state.read().await;
        let sessions: Vec<SessionInfo> = state
            .sessions
            .values()
            .filter(|s| {
                req.repo_id.as_ref().is_none_or(|id| &s.repo_id == id)
                    && req.branch.as_ref().is_none_or(|b| &s.branch == b)
            })
            .map(|s| SessionInfo {
                id: s.id.clone(),
                name: s.name.clone(),
                repo_id: s.repo_id.clone(),
                branch: s.branch.clone(),
                worktree_path: s.worktree_path.to_string_lossy().to_string(),
                status: match s.status() {
                    SessionStatus::Running => session_status::SessionStatus::Running as i32,
                    SessionStatus::Stopped => session_status::SessionStatus::Stopped as i32,
                },
                claude_session_id: s.claude_session_id.clone(),
                is_shell: Some(s.is_shell),
            })
            .collect();

        Ok(Response::new(ListSessionsResponse { sessions }))
    }

    async fn create_session(
        &self,
        request: Request<CreateSessionRequest>,
    ) -> Result<Response<SessionInfo>, Status> {
        let req = request.into_inner();
        let mut state = self.state.write().await;

        let repo = state
            .repos
            .get(&req.repo_id)
            .ok_or_else(|| {
                Status::from(DaemonError::Repo(RepoError::NotFound(req.repo_id.clone())))
            })?
            .clone();

        let git_repo = GitOps::open(&repo.path).map_err(|e| Status::from(DaemonError::from(e)))?;

        // Find or create worktree
        let worktree_path = match GitOps::find_worktree_path(&git_repo, &req.branch) {
            Some(path) => path,
            None => {
                // Auto-create worktree (uses HEAD as base for new branch)
                GitOps::create_worktree(&git_repo, &req.branch, &repo.path, None)
                    .map_err(|e| Status::from(DaemonError::from(e)))?
            }
        };

        // Generate session name
        let existing_names: Vec<String> = state
            .sessions
            .values()
            .filter(|s| s.repo_id == req.repo_id && s.branch == req.branch)
            .map(|s| s.name.clone())
            .collect();

        let name = req
            .name
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| session::generate_session_name(&req.branch, &existing_names));

        // Create session with auto-generated Claude session ID
        let id = session::generate_session_id();
        let is_shell = req.is_shell.unwrap_or(false);

        // Shell sessions don't need Claude session ID
        let claude_session_id = if is_shell {
            None
        } else {
            Some(uuid::Uuid::new_v4().to_string())
        };

        let mut session = Session::new(
            id.clone(),
            name.clone(),
            req.repo_id.clone(),
            req.branch.clone(),
            worktree_path.clone(),
            claude_session_id.clone(),
            is_shell,
        );

        // Start session
        session
            .start()
            .map_err(|e| Status::from(DaemonError::Session(SessionError::Start(e.to_string()))))?;

        let info = SessionInfo {
            id: session.id.clone(),
            name: session.name.clone(),
            repo_id: session.repo_id.clone(),
            branch: session.branch.clone(),
            worktree_path: session.worktree_path.to_string_lossy().to_string(),
            status: session_status::SessionStatus::Running as i32,
            claude_session_id,
            is_shell: Some(session.is_shell),
        };

        // Save session metadata to disk
        if let Err(e) = persistence::save_session_meta(&session) {
            tracing::warn!("Failed to persist session metadata: {}", e);
        }

        state.sessions.insert(id, session);

        // Emit session created event
        self.events.emit_session_created(info.clone());

        Ok(Response::new(info))
    }

    async fn rename_session(
        &self,
        request: Request<RenameSessionRequest>,
    ) -> Result<Response<SessionInfo>, Status> {
        let req = request.into_inner();
        let mut state = self.state.write().await;

        let session = state.sessions.get_mut(&req.session_id).ok_or_else(|| {
            Status::from(DaemonError::Session(SessionError::NotFound(
                req.session_id.clone(),
            )))
        })?;

        let old_name = session.name.clone();
        session.name = req.new_name.clone();
        session.name_updated_from_claude = true; // Mark as manually updated

        // Save updated metadata
        if let Err(e) = persistence::save_session_meta(session) {
            tracing::warn!("Failed to persist session metadata after rename: {}", e);
        }

        let info = SessionInfo {
            id: session.id.clone(),
            name: session.name.clone(),
            repo_id: session.repo_id.clone(),
            branch: session.branch.clone(),
            worktree_path: session.worktree_path.to_string_lossy().to_string(),
            status: match session.status() {
                SessionStatus::Running => session_status::SessionStatus::Running as i32,
                SessionStatus::Stopped => session_status::SessionStatus::Stopped as i32,
            },
            claude_session_id: session.claude_session_id.clone(),
            is_shell: Some(session.is_shell),
        };

        // Emit session name updated event
        self.events
            .emit_session_name_updated(req.session_id, old_name, req.new_name);

        Ok(Response::new(info))
    }

    async fn destroy_session(
        &self,
        request: Request<DestroySessionRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let mut state = self.state.write().await;

        let mut session = state.sessions.remove(&req.session_id).ok_or_else(|| {
            Status::from(DaemonError::Session(SessionError::NotFound(
                req.session_id.clone(),
            )))
        })?;

        // Capture info for event before stopping
        let session_id = session.id.clone();
        let repo_id = session.repo_id.clone();
        let branch = session.branch.clone();

        if let Err(e) = session.stop() {
            tracing::warn!("Failed to stop session {}: {}", req.session_id, e);
        }

        // Delete persisted session data
        if let Err(e) = persistence::delete_session_data(&req.session_id) {
            tracing::warn!("Failed to delete session data: {}", e);
        }

        // Emit session destroyed event
        self.events
            .emit_session_destroyed(session_id, repo_id, branch);

        Ok(Response::new(Empty {}))
    }

    // ============ Events ============

    type SubscribeEventsStream =
        Pin<Box<dyn Stream<Item = Result<Event, Status>> + Send + 'static>>;

    async fn subscribe_events(
        &self,
        request: Request<SubscribeEventsRequest>,
    ) -> Result<Response<Self::SubscribeEventsStream>, Status> {
        let req = request.into_inner();
        let repo_filter = req.repo_id;

        // Subscribe to event broadcaster
        let mut event_rx = self.events.subscribe();

        // Create output channel for filtered events
        let (tx, rx) = mpsc::channel::<Result<Event, Status>>(32);

        // Spawn task to forward events with filtering
        tokio::spawn(async move {
            loop {
                match event_rx.recv().await {
                    Ok(event) => {
                        // Apply repo_id filter if specified
                        let should_send = match (&repo_filter, &event.event) {
                            (None, _) => true, // No filter, send all
                            (Some(filter_repo_id), Some(event::Event::SessionCreated(e))) => e
                                .session
                                .as_ref()
                                .map(|s| &s.repo_id == filter_repo_id)
                                .unwrap_or(false),
                            (Some(filter_repo_id), Some(event::Event::SessionDestroyed(e))) => {
                                &e.repo_id == filter_repo_id
                            }
                            // Name/status updates don't have repo_id, send all for now
                            // TUI can filter client-side if needed
                            (Some(_), Some(event::Event::SessionNameUpdated(_))) => true,
                            (Some(_), Some(event::Event::SessionStatusChanged(_))) => true,
                            // Worktree events
                            (Some(filter_repo_id), Some(event::Event::WorktreeAdded(e))) => e
                                .worktree
                                .as_ref()
                                .map(|w| &w.repo_id == filter_repo_id)
                                .unwrap_or(false),
                            (Some(filter_repo_id), Some(event::Event::WorktreeRemoved(e))) => {
                                &e.repo_id == filter_repo_id
                            }
                            (_, None) => false,
                        };

                        if should_send {
                            // Clone the Arc'd event
                            if tx.send(Ok((*event).clone())).await.is_err() {
                                // Client disconnected
                                break;
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Event subscriber lagged, missed {} events", n);
                        // Continue receiving
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        // Broadcaster closed, exit
                        break;
                    }
                }
            }
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }

    // ============ Diff Operations ============

    async fn get_diff_files(
        &self,
        request: Request<GetDiffFilesRequest>,
    ) -> Result<Response<GetDiffFilesResponse>, Status> {
        let req = request.into_inner();
        let state = self.state.read().await;

        let repo = state.repos.get(&req.repo_id).ok_or_else(|| {
            Status::from(DaemonError::Repo(RepoError::NotFound(req.repo_id.clone())))
        })?;

        let git_repo = GitOps::open(&repo.path).map_err(|e| Status::from(DaemonError::from(e)))?;

        // Find worktree path for the branch
        let worktree_path =
            GitOps::find_worktree_path(&git_repo, &req.branch).ok_or_else(|| {
                Status::not_found(format!("Worktree not found for branch: {}", req.branch))
            })?;

        // Get diff files
        let diff_files = DiffOps::get_diff_files(&worktree_path)
            .map_err(|e| Status::from(DaemonError::from(e)))?;

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

    async fn get_file_diff(
        &self,
        request: Request<GetFileDiffRequest>,
    ) -> Result<Response<GetFileDiffResponse>, Status> {
        let req = request.into_inner();
        let state = self.state.read().await;

        let repo = state.repos.get(&req.repo_id).ok_or_else(|| {
            Status::from(DaemonError::Repo(RepoError::NotFound(req.repo_id.clone())))
        })?;

        let git_repo = GitOps::open(&repo.path).map_err(|e| Status::from(DaemonError::from(e)))?;

        // Find worktree path for the branch
        let worktree_path =
            GitOps::find_worktree_path(&git_repo, &req.branch).ok_or_else(|| {
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

    // ============ Review/Comment Operations ============

    async fn create_line_comment(
        &self,
        request: Request<CreateLineCommentRequest>,
    ) -> Result<Response<LineCommentInfo>, Status> {
        let req = request.into_inner();

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

    async fn update_line_comment(
        &self,
        request: Request<UpdateLineCommentRequest>,
    ) -> Result<Response<LineCommentInfo>, Status> {
        let req = request.into_inner();

        // We need to find the comment first to get repo_id and branch
        // For now, we'll iterate through all repos/branches to find it
        // This is not optimal but works for the prototype
        let state = self.state.read().await;

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

    async fn delete_line_comment(
        &self,
        request: Request<DeleteLineCommentRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();

        // Similar search pattern as update
        let state = self.state.read().await;

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

    async fn list_line_comments(
        &self,
        request: Request<ListLineCommentsRequest>,
    ) -> Result<Response<ListLineCommentsResponse>, Status> {
        let req = request.into_inner();

        let comments =
            ReviewOps::list_comments(&req.repo_id, &req.branch, req.file_path.as_deref())
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

    // ============ Attach/Detach ============

    type AttachSessionStream =
        Pin<Box<dyn Stream<Item = Result<AttachOutput, Status>> + Send + 'static>>;

    async fn attach_session(
        &self,
        request: Request<Streaming<AttachInput>>,
    ) -> Result<Response<Self::AttachSessionStream>, Status> {
        let mut input_stream = request.into_inner();
        let state = self.state.clone();

        // Get session ID from first message
        let first_msg = input_stream
            .next()
            .await
            .ok_or_else(|| Status::invalid_argument("No input received"))?
            .map_err(|e| Status::internal(e.to_string()))?;

        let session_id = first_msg.session_id.clone();

        // Verify session exists and start if needed (handles restored sessions)
        {
            let mut state = state.write().await;
            let session = state.sessions.get_mut(&session_id).ok_or_else(|| {
                Status::from(DaemonError::Session(SessionError::NotFound(
                    session_id.clone(),
                )))
            })?;

            // Start session if not running
            if session.status() == SessionStatus::Stopped {
                session.start().map_err(|e| {
                    Status::from(DaemonError::Session(SessionError::Start(e.to_string())))
                })?;

                // Save updated metadata (in case claude_session_id was auto-generated)
                if let Err(e) = persistence::save_session_meta(session) {
                    tracing::warn!("Failed to persist session metadata: {}", e);
                }
            }
        }

        // Create output channel
        let (tx, rx) = mpsc::channel(32);
        let state_clone = state.clone();
        let session_id_clone = session_id.clone();

        // Send history buffer first
        {
            let state = state_clone.read().await;
            if let Some(session) = state.sessions.get(&session_id_clone) {
                let history = session.get_screen_state();
                if !history.is_empty() {
                    let output = AttachOutput { data: history };
                    let _ = tx.send(Ok(output)).await;
                }
            }
        }

        // Spawn task to read from PTY and send to client
        tokio::spawn(async move {
            let mut buf = [0u8; 4096];
            let mut save_counter = 0u32;
            let mut name_check_counter = 0u32;
            loop {
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

                // Periodically try to update session name from Claude's first message
                name_check_counter += 1;
                if name_check_counter >= 50 {
                    // Check every ~0.5 seconds
                    name_check_counter = 0;
                    let mut state = state_clone.write().await;
                    if let Some(session) = state.sessions.get_mut(&session_id_clone) {
                        if !session.name_updated_from_claude {
                            session.update_name_from_claude();
                            if session.name_updated_from_claude {
                                // Save updated metadata
                                let _ = persistence::save_session_meta(session);
                            }
                        }
                    }
                }

                let state = state_clone.read().await;
                if let Some(session) = state.sessions.get(&session_id_clone) {
                    match session.read(&mut buf) {
                        Ok(n) if n > 0 => {
                            // Store output in session buffer
                            session.process_output(&buf[..n]);

                            let output = AttachOutput {
                                data: buf[..n].to_vec(),
                            };
                            if tx.send(Ok(output)).await.is_err() {
                                // Client disconnected, save history before exit
                                let _ = persistence::save_session_history(session);
                                break;
                            }

                            // Periodically save history (every ~1 second of output)
                            save_counter += 1;
                            if save_counter >= 100 {
                                save_counter = 0;
                                let _ = persistence::save_session_history(session);
                            }
                        }
                        Ok(_) => {}
                        Err(_) => {
                            // PTY error, save history before exit
                            let _ = persistence::save_session_history(session);
                            break;
                        }
                    }
                } else {
                    break;
                }
            }
        });

        // Spawn task to read from client and write to PTY
        let state_clone = state.clone();
        tokio::spawn(async move {
            // Process first message data if any
            if !first_msg.data.is_empty() {
                let state = state_clone.read().await;
                if let Some(session) = state.sessions.get(&session_id) {
                    session.write(&first_msg.data).ok();
                }
            }

            // Handle resize from first message
            if let (Some(rows), Some(cols)) = (first_msg.rows, first_msg.cols) {
                let state = state_clone.read().await;
                if let Some(session) = state.sessions.get(&session_id) {
                    session.resize(rows as u16, cols as u16).ok();
                }
            }

            // Process remaining messages
            while let Some(Ok(msg)) = input_stream.next().await {
                let state = state_clone.read().await;
                if let Some(session) = state.sessions.get(&msg.session_id) {
                    // Write data
                    if !msg.data.is_empty() {
                        session.write(&msg.data).ok();
                    }

                    // Handle resize
                    if let (Some(rows), Some(cols)) = (msg.rows, msg.cols) {
                        session.resize(rows as u16, cols as u16).ok();
                    }
                }
            }
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }

    // ============ Git Status Operations ============

    async fn get_git_status(
        &self,
        request: Request<GetGitStatusRequest>,
    ) -> Result<Response<GetGitStatusResponse>, Status> {
        let req = request.into_inner();
        let state = self.state.read().await;

        let repo = state.repos.get(&req.repo_id).ok_or_else(|| {
            Status::from(DaemonError::Repo(RepoError::NotFound(req.repo_id.clone())))
        })?;

        let git_repo = GitOps::open(&repo.path).map_err(|e| Status::from(DaemonError::from(e)))?;

        // Find worktree path for the branch
        let worktree_path =
            GitOps::find_worktree_path(&git_repo, &req.branch).ok_or_else(|| {
                Status::not_found(format!("Worktree not found for branch: {}", req.branch))
            })?;

        // Open the worktree repository
        let wt_repo =
            GitOps::open(&worktree_path).map_err(|e| Status::from(DaemonError::from(e)))?;

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

    async fn stage_file(
        &self,
        request: Request<StageFileRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let state = self.state.read().await;

        let repo = state.repos.get(&req.repo_id).ok_or_else(|| {
            Status::from(DaemonError::Repo(RepoError::NotFound(req.repo_id.clone())))
        })?;

        let git_repo = GitOps::open(&repo.path).map_err(|e| Status::from(DaemonError::from(e)))?;

        // Find worktree path for the branch
        let worktree_path =
            GitOps::find_worktree_path(&git_repo, &req.branch).ok_or_else(|| {
                Status::not_found(format!("Worktree not found for branch: {}", req.branch))
            })?;

        // Open the worktree repository
        let wt_repo =
            GitOps::open(&worktree_path).map_err(|e| Status::from(DaemonError::from(e)))?;

        // Stage the file
        GitOps::stage_file(&wt_repo, &req.file_path)
            .map_err(|e| Status::from(DaemonError::from(e)))?;

        Ok(Response::new(Empty {}))
    }

    async fn unstage_file(
        &self,
        request: Request<UnstageFileRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let state = self.state.read().await;

        let repo = state.repos.get(&req.repo_id).ok_or_else(|| {
            Status::from(DaemonError::Repo(RepoError::NotFound(req.repo_id.clone())))
        })?;

        let git_repo = GitOps::open(&repo.path).map_err(|e| Status::from(DaemonError::from(e)))?;

        // Find worktree path for the branch
        let worktree_path =
            GitOps::find_worktree_path(&git_repo, &req.branch).ok_or_else(|| {
                Status::not_found(format!("Worktree not found for branch: {}", req.branch))
            })?;

        // Open the worktree repository
        let wt_repo =
            GitOps::open(&worktree_path).map_err(|e| Status::from(DaemonError::from(e)))?;

        // Unstage the file
        GitOps::unstage_file(&wt_repo, &req.file_path)
            .map_err(|e| Status::from(DaemonError::from(e)))?;

        Ok(Response::new(Empty {}))
    }

    async fn stage_all(
        &self,
        request: Request<StageAllRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let state = self.state.read().await;

        let repo = state.repos.get(&req.repo_id).ok_or_else(|| {
            Status::from(DaemonError::Repo(RepoError::NotFound(req.repo_id.clone())))
        })?;

        let git_repo = GitOps::open(&repo.path).map_err(|e| Status::from(DaemonError::from(e)))?;

        // Find worktree path for the branch
        let worktree_path =
            GitOps::find_worktree_path(&git_repo, &req.branch).ok_or_else(|| {
                Status::not_found(format!("Worktree not found for branch: {}", req.branch))
            })?;

        // Open the worktree repository
        let wt_repo =
            GitOps::open(&worktree_path).map_err(|e| Status::from(DaemonError::from(e)))?;

        // Stage all files
        GitOps::stage_all(&wt_repo).map_err(|e| Status::from(DaemonError::from(e)))?;

        Ok(Response::new(Empty {}))
    }

    async fn unstage_all(
        &self,
        request: Request<UnstageAllRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let state = self.state.read().await;

        let repo = state.repos.get(&req.repo_id).ok_or_else(|| {
            Status::from(DaemonError::Repo(RepoError::NotFound(req.repo_id.clone())))
        })?;

        let git_repo = GitOps::open(&repo.path).map_err(|e| Status::from(DaemonError::from(e)))?;

        // Find worktree path for the branch
        let worktree_path =
            GitOps::find_worktree_path(&git_repo, &req.branch).ok_or_else(|| {
                Status::not_found(format!("Worktree not found for branch: {}", req.branch))
            })?;

        // Open the worktree repository
        let wt_repo =
            GitOps::open(&worktree_path).map_err(|e| Status::from(DaemonError::from(e)))?;

        // Unstage all files
        GitOps::unstage_all(&wt_repo).map_err(|e| Status::from(DaemonError::from(e)))?;

        Ok(Response::new(Empty {}))
    }
}

mod session_status {
    pub enum SessionStatus {
        Running = 1,
        Stopped = 2,
    }
}
