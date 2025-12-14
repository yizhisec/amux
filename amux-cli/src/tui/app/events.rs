//! Event handling and async action execution

use super::super::state::AsyncAction;
use super::super::App;
use crate::error::TuiError;
use amux_proto::daemon::{event as daemon_event, Event as DaemonEvent};
use tracing::debug;

type Result<T> = std::result::Result<T, TuiError>;

impl App {
    /// Check if event subscription needs to be restored
    pub fn needs_resubscribe(&self) -> bool {
        self.event_rx.is_none()
    }

    /// Try to resubscribe to events
    pub async fn try_resubscribe(&mut self) {
        self.subscribe_events().await;
    }

    /// Handle daemon event and return true if UI needs redraw
    pub fn handle_daemon_event(&mut self, event: DaemonEvent) -> Option<AsyncAction> {
        match event.event {
            Some(daemon_event::Event::SessionCreated(e)) => {
                debug!(
                    "Event: SessionCreated {:?}",
                    e.session.as_ref().map(|s| &s.id)
                );
                if let Some(session) = e.session {
                    // Only add if it matches current repo/branch filter
                    if let (Some(repo), Some(branch)) =
                        (self.current_repo(), self.current_worktree())
                    {
                        if session.repo_id == repo.info.id && session.branch == branch.branch {
                            if let Some(repo) = self.current_repo_mut() {
                                repo.sessions.push(session);
                            }
                            self.dirty.sidebar = true;
                            return None; // Session list changed, will redraw
                        }
                    }
                }
                None
            }
            Some(daemon_event::Event::SessionDestroyed(e)) => {
                debug!("Event: SessionDestroyed {}", e.session_id);
                if let Some(repo) = self.current_repo_mut() {
                    let old_len = repo.sessions.len();
                    // Remove session from list
                    repo.sessions.retain(|s| s.id != e.session_id);
                    // Clamp session index
                    if !repo.sessions.is_empty() && repo.session_idx >= repo.sessions.len() {
                        repo.session_idx = repo.sessions.len() - 1;
                    }
                    // Only redraw if session was actually removed
                    if repo.sessions.len() != old_len {
                        self.dirty.sidebar = true;
                    }
                }
                None
            }
            Some(daemon_event::Event::SessionNameUpdated(e)) => {
                debug!(
                    "Event: SessionNameUpdated {} -> {}",
                    e.session_id, e.new_name
                );
                let mut changed = false;

                // Update session name in main sessions list
                if let Some(repo) = self.current_repo_mut() {
                    if let Some(session) = repo.sessions.iter_mut().find(|s| s.id == e.session_id) {
                        if session.name != e.new_name {
                            session.name = e.new_name.clone();
                            changed = true;
                        }
                    }
                }

                // Also update in sidebar sessions_by_worktree (for tree view)
                if let Some(repo) = self.current_repo_mut() {
                    for sessions in repo.sessions_by_worktree.values_mut() {
                        if let Some(session) = sessions.iter_mut().find(|s| s.id == e.session_id) {
                            if session.name != e.new_name {
                                session.name = e.new_name.clone();
                                changed = true;
                            }
                        }
                    }
                }

                if changed {
                    self.dirty.sidebar = true;
                }
                None
            }
            Some(daemon_event::Event::SessionStatusChanged(e)) => {
                debug!(
                    "Event: SessionStatusChanged {} {} -> {}",
                    e.session_id, e.old_status, e.new_status
                );
                let mut changed = false;

                // Update session status in main sessions list
                if let Some(repo) = self.current_repo_mut() {
                    if let Some(session) = repo.sessions.iter_mut().find(|s| s.id == e.session_id) {
                        debug!(
                            "Found session in sessions list, current status: {}, new status: {}",
                            session.status, e.new_status
                        );
                        if session.status != e.new_status {
                            debug!(
                                "Status changed! Updating from {} to {}",
                                session.status, e.new_status
                            );
                            session.status = e.new_status;
                            changed = true;
                        }
                    } else {
                        debug!("Session {} not found in sessions list", e.session_id);
                    }
                }

                // Also update in sidebar sessions_by_worktree (for tree view)
                if let Some(repo) = self.current_repo_mut() {
                    for sessions in repo.sessions_by_worktree.values_mut() {
                        if let Some(session) = sessions.iter_mut().find(|s| s.id == e.session_id) {
                            debug!(
                                "Found session in sidebar, updating status from {} to {}",
                                session.status, e.new_status
                            );
                            if session.status != e.new_status {
                                session.status = e.new_status;
                                changed = true;
                            }
                        }
                    }
                }

                if changed {
                    self.dirty.sidebar = true;
                }
                None
            }
            Some(daemon_event::Event::WorktreeAdded(e)) => {
                debug!(
                    "Event: WorktreeAdded {:?}",
                    e.worktree.as_ref().map(|w| &w.branch)
                );
                if let Some(worktree) = e.worktree {
                    // Only add if it matches current repo
                    if let Some(repo) = self.current_repo() {
                        if worktree.repo_id == repo.info.id {
                            // Check if worktree already exists to avoid duplicates
                            if let Some(repo) = self.current_repo_mut() {
                                if !repo.worktrees.iter().any(|w| w.branch == worktree.branch) {
                                    repo.worktrees.push(worktree);
                                    self.dirty.sidebar = true;
                                    return None;
                                }
                            }
                        }
                    }
                }
                None
            }
            Some(daemon_event::Event::WorktreeRemoved(e)) => {
                debug!("Event: WorktreeRemoved {} {}", e.repo_id, e.branch);
                // Remove worktree from list if it matches current repo
                let repo_id_matches = self
                    .current_repo()
                    .map(|r| r.info.id == e.repo_id)
                    .unwrap_or(false);
                if repo_id_matches {
                    if let Some(repo) = self.current_repo_mut() {
                        let old_len = repo.worktrees.len();
                        repo.worktrees.retain(|w| w.branch != e.branch);

                        if repo.worktrees.len() != old_len {
                            // Worktree was removed, update state
                            if repo.worktrees.is_empty() {
                                // All worktrees removed
                                repo.branch_idx = 0;
                                repo.sidebar_cursor = 0;
                                repo.expanded_worktrees.clear();
                                repo.sessions_by_worktree.clear();
                            } else {
                                // Clamp branch index
                                if repo.branch_idx >= repo.worktrees.len() {
                                    repo.branch_idx = repo.worktrees.len() - 1;
                                }
                                // Clear session caches (indices may have shifted)
                                repo.sessions_by_worktree.clear();
                                repo.expanded_worktrees.clear();
                            }

                            // Recalculate sidebar items and clamp cursor
                            let max_cursor = repo.calculate_sidebar_total().saturating_sub(1);
                            if repo.sidebar_cursor > max_cursor {
                                repo.sidebar_cursor = max_cursor;
                            }

                            self.dirty.sidebar = true;
                        }
                    }
                }
                self.update_sidebar_total_items();
                None
            }
            Some(daemon_event::Event::GitStatusChanged(e)) => {
                debug!("Event: GitStatusChanged {}/{}", e.repo_id, e.branch);

                // Only refresh if event is for current worktree
                if let (Some(repo), Some(worktree)) = (self.current_repo(), self.current_worktree())
                {
                    if e.repo_id == repo.info.id && e.branch == worktree.branch {
                        debug!("Auto-refreshing git status for {}/{}", e.repo_id, e.branch);

                        // Client-side debounce: avoid refreshing too frequently
                        if let Some(last) = self.last_git_refresh {
                            if last.elapsed() < std::time::Duration::from_millis(500) {
                                debug!(
                                    "Skipping refresh: debounced (last refresh was {}ms ago)",
                                    last.elapsed().as_millis()
                                );
                                return None; // Skip if refreshed <500ms ago
                            }
                        }

                        self.last_git_refresh = Some(std::time::Instant::now());
                        return Some(AsyncAction::LoadGitStatus);
                    }
                }
                None
            }
            None => None,
        }
    }

    /// Execute a queued async action
    pub async fn execute_async_action(&mut self, action: AsyncAction) -> Result<()> {
        match action {
            AsyncAction::RefreshAll => {
                self.refresh_all().await?;
            }
            AsyncAction::RefreshSessions => {
                let _ = self.refresh_sessions().await;
            }
            AsyncAction::RefreshBranches => {
                let _ = self.refresh_branches().await;
            }
            AsyncAction::CreateSession => {
                self.create_new().await?;
            }
            AsyncAction::SubmitInput => {
                self.submit_input().await?;
            }
            AsyncAction::SubmitRenameSession => {
                self.submit_rename_session().await?;
            }
            AsyncAction::SubmitAddWorktree => {
                self.submit_add_worktree().await?;
            }
            AsyncAction::ConfirmDelete { target, action } => {
                self.confirm_delete(target, action).await?;
            }
            AsyncAction::ConfirmDeleteBranch => {
                self.confirm_delete_branch().await?;
            }
            AsyncAction::ConfirmDeleteWorktreeSessions => {
                self.confirm_delete_worktree_sessions().await?;
            }
            AsyncAction::DestroySession { session_id } => {
                self.client.destroy_session(&session_id).await?;
                let _ = self.refresh_sessions().await;
                // Also refresh worktree sessions for tree view
                let wt_idx = self.branch_idx();
                let _ = self.load_worktree_sessions(wt_idx).await;
                // Adjust sidebar cursor if it's now out of bounds
                if let Some(repo) = self.current_repo_mut() {
                    let max_cursor = repo.calculate_sidebar_total().saturating_sub(1);
                    if repo.sidebar_cursor > max_cursor {
                        repo.sidebar_cursor = max_cursor;
                    }
                }
                // Sync selection from updated cursor position
                self.update_selection_from_sidebar();
                self.dirty.sidebar = true;
            }
            AsyncAction::RenameSession {
                session_id,
                new_name,
            } => {
                self.client.rename_session(&session_id, &new_name).await?;
            }
            AsyncAction::ConnectStream => {
                self.enter_terminal().await?;
            }
            AsyncAction::ResizeTerminal { rows, cols } => {
                self.resize_terminal(rows, cols).await?;
            }
            AsyncAction::SendToTerminal { data } => {
                self.send_to_terminal(data).await?;
            }
            AsyncAction::SwitchToDiffView => {
                self.switch_to_diff_view().await?;
            }
            AsyncAction::LoadDiffFiles => {
                self.load_diff_files().await?;
            }
            AsyncAction::LoadFileDiff => {
                self.load_file_diff().await?;
            }
            AsyncAction::LoadComments => {
                self.load_comments().await?;
            }
            AsyncAction::SubmitLineComment => {
                self.submit_line_comment().await?;
            }
            AsyncAction::UpdateLineComment => {
                self.update_line_comment().await?;
            }
            AsyncAction::DeleteLineComment => {
                self.delete_current_line_comment().await?;
            }
            AsyncAction::SubmitReviewToClaude => {
                self.submit_review_to_claude().await?;
            }
            AsyncAction::LoadWorktreeSessions { wt_idx } => {
                self.load_worktree_sessions(wt_idx).await?;
            }
            AsyncAction::LoadGitStatus => {
                self.load_git_status().await?;
            }
            AsyncAction::StageFile { file_path } => {
                self.stage_file(&file_path).await?;
            }
            AsyncAction::UnstageFile { file_path } => {
                self.unstage_file(&file_path).await?;
            }
            AsyncAction::StageAll => {
                self.stage_all().await?;
            }
            AsyncAction::UnstageAll => {
                self.unstage_all().await?;
            }
            AsyncAction::GitPush => {
                self.git_push().await?;
            }
            AsyncAction::GitPull => {
                self.git_pull().await?;
            }
            AsyncAction::SwitchToShell => {
                self.switch_to_shell_session().await?;
            }
            AsyncAction::LoadTodos => {
                self.load_todos().await?;
            }
            AsyncAction::CreateTodo {
                title,
                description,
                parent_id,
            } => {
                self.create_todo(title, description, parent_id).await?;
            }
            AsyncAction::ToggleTodo { todo_id } => {
                self.toggle_todo(&todo_id).await?;
            }
            AsyncAction::DeleteTodo { todo_id } => {
                self.delete_todo(&todo_id).await?;
            }
            AsyncAction::UpdateTodo {
                todo_id,
                title,
                description,
            } => {
                self.update_todo(&todo_id, title, description).await?;
            }
            AsyncAction::ReorderTodo {
                todo_id,
                new_order,
                new_parent_id,
            } => {
                self.reorder_todo(&todo_id, new_order, new_parent_id)
                    .await?;
            }
            AsyncAction::FetchProviders { repo_id, branch } => {
                self.fetch_providers(&repo_id, &branch).await?;
            }
            AsyncAction::SubmitProviderSelection => {
                self.submit_provider_selection().await?;
            }
            AsyncAction::SubmitCreateSessionInput => {
                self.submit_create_session_input().await?;
            }
        }
        Ok(())
    }
}
