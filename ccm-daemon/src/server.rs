//! gRPC server implementation

use crate::git::GitOps;
use crate::repo::{self, Repo};
use crate::session::{self, Session, SessionStatus};
use crate::state::SharedState;
use anyhow::Result;
use ccm_proto::daemon::ccm_daemon_server::CcmDaemon;
use ccm_proto::daemon::*;
use std::pin::Pin;
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream, Stream, StreamExt};
use tonic::{Request, Response, Status, Streaming};

/// CCM Daemon gRPC service
pub struct CcmDaemonService {
    state: SharedState,
}

impl CcmDaemonService {
    pub fn new(state: SharedState) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl CcmDaemon for CcmDaemonService {
    // ============ Repo Management ============

    async fn add_repo(&self, request: Request<AddRepoRequest>) -> Result<Response<RepoInfo>, Status> {
        let req = request.into_inner();
        let path = std::path::PathBuf::from(&req.path);

        // Create repo
        let repo = Repo::new(path).map_err(|e| Status::invalid_argument(e.to_string()))?;

        // Add to state
        let mut state = self.state.write().await;
        if state.repos.contains_key(&repo.id) {
            return Err(Status::already_exists("Repo already added"));
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
        repo::save_repos(&repos).map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(info))
    }

    async fn list_repos(&self, _request: Request<Empty>) -> Result<Response<ListReposResponse>, Status> {
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

    async fn remove_repo(&self, request: Request<RemoveRepoRequest>) -> Result<Response<Empty>, Status> {
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
            .ok_or_else(|| Status::not_found("Repo not found"))?;

        // Save to disk
        let repos: Vec<_> = state.repos.values().cloned().collect();
        drop(state);
        repo::save_repos(&repos).map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(Empty {}))
    }

    // ============ Worktree Management ============

    async fn list_worktrees(
        &self,
        request: Request<ListWorktreesRequest>,
    ) -> Result<Response<ListWorktreesResponse>, Status> {
        let req = request.into_inner();
        let state = self.state.read().await;

        let repo = state
            .repos
            .get(&req.repo_id)
            .ok_or_else(|| Status::not_found("Repo not found"))?;

        let git_repo =
            GitOps::open(&repo.path).map_err(|e| Status::internal(e.to_string()))?;

        // Get worktrees from git
        let git_worktrees =
            GitOps::list_worktrees(&git_repo).map_err(|e| Status::internal(e.to_string()))?;

        // Get all branches
        let branches =
            GitOps::list_branches(&git_repo).map_err(|e| Status::internal(e.to_string()))?;

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
                path: wt.map(|w| w.path.to_string_lossy().to_string()).unwrap_or_default(),
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

        let repo = state
            .repos
            .get(&req.repo_id)
            .ok_or_else(|| Status::not_found("Repo not found"))?;

        let git_repo =
            GitOps::open(&repo.path).map_err(|e| Status::internal(e.to_string()))?;

        let wt_path = GitOps::create_worktree(&git_repo, &req.branch, &repo.path)
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(WorktreeInfo {
            repo_id: req.repo_id,
            branch: req.branch,
            path: wt_path.to_string_lossy().to_string(),
            is_main: false,
            session_count: 0,
        }))
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

        let repo = state
            .repos
            .get(&req.repo_id)
            .ok_or_else(|| Status::not_found("Repo not found"))?;

        let git_repo =
            GitOps::open(&repo.path).map_err(|e| Status::internal(e.to_string()))?;

        GitOps::remove_worktree(&git_repo, &req.branch)
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(Empty {}))
    }

    // ============ Session Management ============

    async fn list_sessions(
        &self,
        request: Request<ListSessionsRequest>,
    ) -> Result<Response<ListSessionsResponse>, Status> {
        let req = request.into_inner();
        let state = self.state.read().await;

        let sessions: Vec<SessionInfo> = state
            .sessions
            .values()
            .filter(|s| {
                req.repo_id.as_ref().map_or(true, |id| &s.repo_id == id)
                    && req.branch.as_ref().map_or(true, |b| &s.branch == b)
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
            .ok_or_else(|| Status::not_found("Repo not found"))?
            .clone();

        let git_repo =
            GitOps::open(&repo.path).map_err(|e| Status::internal(e.to_string()))?;

        // Find or create worktree
        let worktree_path = match GitOps::find_worktree_path(&git_repo, &req.branch) {
            Some(path) => path,
            None => {
                // Auto-create worktree
                GitOps::create_worktree(&git_repo, &req.branch, &repo.path)
                    .map_err(|e| Status::internal(format!("Failed to create worktree: {}", e)))?
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

        // Create session
        let id = session::generate_session_id();
        let mut session = Session::new(
            id.clone(),
            name.clone(),
            req.repo_id.clone(),
            req.branch.clone(),
            worktree_path.clone(),
        );

        // Start session
        session
            .start()
            .map_err(|e| Status::internal(format!("Failed to start session: {}", e)))?;

        let info = SessionInfo {
            id: session.id.clone(),
            name: session.name.clone(),
            repo_id: session.repo_id.clone(),
            branch: session.branch.clone(),
            worktree_path: session.worktree_path.to_string_lossy().to_string(),
            status: session_status::SessionStatus::Running as i32,
        };

        state.sessions.insert(id, session);

        Ok(Response::new(info))
    }

    async fn destroy_session(
        &self,
        request: Request<DestroySessionRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let mut state = self.state.write().await;

        let mut session = state
            .sessions
            .remove(&req.session_id)
            .ok_or_else(|| Status::not_found("Session not found"))?;

        session.stop().ok();

        Ok(Response::new(Empty {}))
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

        // Verify session exists
        {
            let state = state.read().await;
            if !state.sessions.contains_key(&session_id) {
                return Err(Status::not_found("Session not found"));
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
            loop {
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

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
                                break;
                            }
                        }
                        Ok(_) => {}
                        Err(_) => break,
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
}

mod session_status {
    pub enum SessionStatus {
        Running = 1,
        Stopped = 2,
    }
}
