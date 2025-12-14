//! gRPC server implementation
//!
//! This module defines the CcmDaemonService and implements the CcmDaemon trait.
//! All handler implementations are delegated to the handlers module.

use crate::events::EventBroadcaster;
use crate::file_watcher::WatcherManager;
use crate::handlers;
use crate::state::SharedState;
use amux_proto::daemon::ccm_daemon_server::CcmDaemon;
use amux_proto::daemon::*;
use tonic::{Request, Response, Status, Streaming};

/// Amux Daemon gRPC service
pub struct CcmDaemonService {
    state: SharedState,
    events: EventBroadcaster,
    pub watcher_manager: WatcherManager,
}

impl CcmDaemonService {
    pub fn new(state: SharedState, events: EventBroadcaster) -> Self {
        let watcher_manager = WatcherManager::new(events.clone());
        Self {
            state,
            events,
            watcher_manager,
        }
    }

    /// Initialize watchers for all existing worktrees
    ///
    /// This should be called after the daemon starts to set up file watching
    /// for any worktrees that already exist.
    pub async fn initialize_watchers(&self) -> anyhow::Result<()> {
        use crate::git::GitOps;
        use tracing::{info, warn};

        let state = self.state.read().await;

        info!("Initializing file watchers for {} repos", state.repos.len());

        for repo in state.repos.values() {
            let git_repo = match GitOps::open(&repo.path) {
                Ok(r) => r,
                Err(e) => {
                    warn!("Failed to open repo {}: {}", repo.id, e);
                    continue;
                }
            };

            // List all worktrees (includes main worktree with is_main: true)
            let worktrees = match GitOps::list_worktrees(&git_repo) {
                Ok(wts) => wts,
                Err(e) => {
                    warn!("Failed to list worktrees for {}: {}", repo.id, e);
                    continue;
                }
            };

            info!("Found {} worktrees for repo {}", worktrees.len(), repo.id);

            for wt in worktrees {
                // Watch both main and additional worktrees
                if let Err(e) = self
                    .watcher_manager
                    .watch_worktree(repo.id.clone(), wt.branch.clone(), wt.path.clone())
                    .await
                {
                    warn!(
                        "Failed to start watcher for {}/{}: {}",
                        repo.id, wt.branch, e
                    );
                }
            }
        }

        Ok(())
    }
}

#[tonic::async_trait]
impl CcmDaemon for CcmDaemonService {
    // ============ Repo Management ============

    async fn add_repo(
        &self,
        request: Request<AddRepoRequest>,
    ) -> Result<Response<RepoInfo>, Status> {
        handlers::repo::add_repo(&self.state, request.into_inner()).await
    }

    async fn list_repos(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<ListReposResponse>, Status> {
        handlers::repo::list_repos(&self.state).await
    }

    async fn remove_repo(
        &self,
        request: Request<RemoveRepoRequest>,
    ) -> Result<Response<Empty>, Status> {
        handlers::repo::remove_repo(&self.state, request.into_inner()).await
    }

    // ============ Worktree Management ============

    async fn list_worktrees(
        &self,
        request: Request<ListWorktreesRequest>,
    ) -> Result<Response<ListWorktreesResponse>, Status> {
        handlers::worktree::list_worktrees(&self.state, request.into_inner()).await
    }

    async fn create_worktree(
        &self,
        request: Request<CreateWorktreeRequest>,
    ) -> Result<Response<WorktreeInfo>, Status> {
        let result =
            handlers::worktree::create_worktree(&self.state, &self.events, request.into_inner())
                .await?;

        // Start watching the newly created worktree
        let info = result.get_ref();
        if let Err(e) = self
            .watcher_manager
            .watch_worktree(
                info.repo_id.clone(),
                info.branch.clone(),
                std::path::PathBuf::from(&info.path),
            )
            .await
        {
            tracing::warn!(
                "Failed to start watcher for {}/{}: {}",
                info.repo_id,
                info.branch,
                e
            );
        }

        Ok(result)
    }

    async fn remove_worktree(
        &self,
        request: Request<RemoveWorktreeRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();

        // Stop watching the worktree before removing it
        self.watcher_manager
            .unwatch_worktree(&req.repo_id, &req.branch)
            .await;

        handlers::worktree::remove_worktree(&self.state, &self.events, req).await
    }

    async fn delete_branch(
        &self,
        request: Request<DeleteBranchRequest>,
    ) -> Result<Response<Empty>, Status> {
        handlers::worktree::delete_branch(&self.state, request.into_inner()).await
    }

    // ============ Session Management ============

    async fn list_sessions(
        &self,
        request: Request<ListSessionsRequest>,
    ) -> Result<Response<ListSessionsResponse>, Status> {
        handlers::session::list_sessions(&self.state, request.into_inner()).await
    }

    async fn create_session(
        &self,
        request: Request<CreateSessionRequest>,
    ) -> Result<Response<SessionInfo>, Status> {
        handlers::session::create_session(&self.state, &self.events, request.into_inner()).await
    }

    async fn rename_session(
        &self,
        request: Request<RenameSessionRequest>,
    ) -> Result<Response<SessionInfo>, Status> {
        handlers::session::rename_session(&self.state, &self.events, request.into_inner()).await
    }

    async fn destroy_session(
        &self,
        request: Request<DestroySessionRequest>,
    ) -> Result<Response<Empty>, Status> {
        handlers::session::destroy_session(&self.state, &self.events, request.into_inner()).await
    }

    async fn stop_session(
        &self,
        request: Request<StopSessionRequest>,
    ) -> Result<Response<Empty>, Status> {
        handlers::session::stop_session(&self.state, &self.events, request.into_inner()).await
    }

    // ============ Events ============

    type SubscribeEventsStream = handlers::events::SubscribeEventsStream;

    async fn subscribe_events(
        &self,
        request: Request<SubscribeEventsRequest>,
    ) -> Result<Response<Self::SubscribeEventsStream>, Status> {
        handlers::events::subscribe_events(&self.events, request.into_inner()).await
    }

    // ============ Diff Operations ============

    async fn get_diff_files(
        &self,
        request: Request<GetDiffFilesRequest>,
    ) -> Result<Response<GetDiffFilesResponse>, Status> {
        handlers::diff::get_diff_files(&self.state, request.into_inner()).await
    }

    async fn get_file_diff(
        &self,
        request: Request<GetFileDiffRequest>,
    ) -> Result<Response<GetFileDiffResponse>, Status> {
        handlers::diff::get_file_diff(&self.state, request.into_inner()).await
    }

    // ============ Review/Comment Operations ============

    async fn create_line_comment(
        &self,
        request: Request<CreateLineCommentRequest>,
    ) -> Result<Response<LineCommentInfo>, Status> {
        handlers::comments::create_line_comment(request.into_inner()).await
    }

    async fn update_line_comment(
        &self,
        request: Request<UpdateLineCommentRequest>,
    ) -> Result<Response<LineCommentInfo>, Status> {
        handlers::comments::update_line_comment(&self.state, request.into_inner()).await
    }

    async fn delete_line_comment(
        &self,
        request: Request<DeleteLineCommentRequest>,
    ) -> Result<Response<Empty>, Status> {
        handlers::comments::delete_line_comment(&self.state, request.into_inner()).await
    }

    async fn list_line_comments(
        &self,
        request: Request<ListLineCommentsRequest>,
    ) -> Result<Response<ListLineCommentsResponse>, Status> {
        handlers::comments::list_line_comments(request.into_inner()).await
    }

    // ============ Attach/Detach ============

    type AttachSessionStream = handlers::attach::AttachSessionStream;

    async fn attach_session(
        &self,
        request: Request<Streaming<AttachInput>>,
    ) -> Result<Response<Self::AttachSessionStream>, Status> {
        handlers::attach::attach_session(
            self.state.clone(),
            self.events.clone(),
            request.into_inner(),
        )
        .await
    }

    // ============ Git Status Operations ============

    async fn get_git_status(
        &self,
        request: Request<GetGitStatusRequest>,
    ) -> Result<Response<GetGitStatusResponse>, Status> {
        handlers::git_status::get_git_status(&self.state, request.into_inner()).await
    }

    async fn stage_file(
        &self,
        request: Request<StageFileRequest>,
    ) -> Result<Response<Empty>, Status> {
        handlers::git_status::stage_file(&self.state, request.into_inner()).await
    }

    async fn unstage_file(
        &self,
        request: Request<UnstageFileRequest>,
    ) -> Result<Response<Empty>, Status> {
        handlers::git_status::unstage_file(&self.state, request.into_inner()).await
    }

    async fn stage_all(
        &self,
        request: Request<StageAllRequest>,
    ) -> Result<Response<Empty>, Status> {
        handlers::git_status::stage_all(&self.state, request.into_inner()).await
    }

    async fn unstage_all(
        &self,
        request: Request<UnstageAllRequest>,
    ) -> Result<Response<Empty>, Status> {
        handlers::git_status::unstage_all(&self.state, request.into_inner()).await
    }

    async fn git_push(
        &self,
        request: Request<GitPushRequest>,
    ) -> Result<Response<GitPushResponse>, Status> {
        handlers::git_status::git_push(&self.state, request.into_inner()).await
    }

    async fn git_pull(
        &self,
        request: Request<GitPullRequest>,
    ) -> Result<Response<GitPullResponse>, Status> {
        handlers::git_status::git_pull(&self.state, request.into_inner()).await
    }

    // ============ TODO Operations ============

    async fn create_todo(
        &self,
        request: Request<CreateTodoRequest>,
    ) -> Result<Response<TodoItem>, Status> {
        handlers::todo::create_todo(request.into_inner()).await
    }

    async fn update_todo(
        &self,
        request: Request<UpdateTodoRequest>,
    ) -> Result<Response<TodoItem>, Status> {
        handlers::todo::update_todo(&self.state, request.into_inner()).await
    }

    async fn delete_todo(
        &self,
        request: Request<DeleteTodoRequest>,
    ) -> Result<Response<Empty>, Status> {
        handlers::todo::delete_todo(&self.state, request.into_inner()).await
    }

    async fn list_todos(
        &self,
        request: Request<ListTodosRequest>,
    ) -> Result<Response<ListTodosResponse>, Status> {
        handlers::todo::list_todos(request.into_inner()).await
    }

    async fn toggle_todo(
        &self,
        request: Request<ToggleTodoRequest>,
    ) -> Result<Response<TodoItem>, Status> {
        handlers::todo::toggle_todo(&self.state, request.into_inner()).await
    }

    async fn reorder_todo(
        &self,
        request: Request<ReorderTodoRequest>,
    ) -> Result<Response<TodoItem>, Status> {
        handlers::todo::reorder_todo(&self.state, request.into_inner()).await
    }
}
