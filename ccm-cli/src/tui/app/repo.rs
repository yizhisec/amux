//! Repository access and refresh operations

use super::super::state::{AsyncAction, RepoState};
use super::super::App;
use crate::error::TuiError;
use ccm_proto::daemon::{LineCommentInfo, RepoInfo, SessionInfo, WorktreeInfo};
use std::sync::{Arc, Mutex};
use tracing::debug;

type Result<T> = std::result::Result<T, TuiError>;

impl App {
    // ============ Repo Access Helpers ============

    /// Get current repo state (immutable)
    pub fn current_repo(&self) -> Option<&RepoState> {
        self.current_repo_id
            .as_ref()
            .and_then(|id| self.repo_states.get(id))
    }

    /// Get current repo state (mutable)
    pub fn current_repo_mut(&mut self) -> Option<&mut RepoState> {
        // Need to clone the id to avoid borrow issues
        let id = self.current_repo_id.clone()?;
        self.repo_states.get_mut(&id)
    }

    /// Get current repo index in repo_order
    pub fn repo_idx(&self) -> usize {
        self.current_repo_id
            .as_ref()
            .and_then(|id| self.repo_order.iter().position(|r| r == id))
            .unwrap_or(0)
    }

    /// Get repos in display order
    pub fn repos_ordered(&self) -> impl Iterator<Item = &RepoState> {
        self.repo_order
            .iter()
            .filter_map(|id| self.repo_states.get(id))
    }

    /// Get repos as RepoInfo list (for compatibility)
    pub fn repos(&self) -> Vec<RepoInfo> {
        self.repos_ordered().map(|r| r.info.clone()).collect()
    }

    /// Get current worktrees (convenience)
    pub fn worktrees(&self) -> &[WorktreeInfo] {
        self.current_repo()
            .map(|r| r.worktrees.as_slice())
            .unwrap_or(&[])
    }

    /// Get current available branches (convenience)
    pub fn available_branches(&self) -> &[WorktreeInfo] {
        self.current_repo()
            .map(|r| r.available_branches.as_slice())
            .unwrap_or(&[])
    }

    /// Get current sessions (convenience)
    pub fn sessions(&self) -> &[SessionInfo] {
        self.current_repo()
            .map(|r| r.sessions.as_slice())
            .unwrap_or(&[])
    }

    /// Get current branch_idx (convenience)
    pub fn branch_idx(&self) -> usize {
        self.current_repo().map(|r| r.branch_idx).unwrap_or(0)
    }

    /// Get current worktree (convenience)
    pub fn current_worktree(&self) -> Option<&WorktreeInfo> {
        self.current_repo().and_then(|r| r.current_worktree())
    }

    /// Get current session (convenience)
    pub fn current_session(&self) -> Option<&SessionInfo> {
        self.current_repo().and_then(|r| r.current_session())
    }

    /// Get current line comments (convenience)
    pub fn line_comments(&self) -> &[LineCommentInfo] {
        self.current_repo()
            .map(|r| r.line_comments.as_slice())
            .unwrap_or(&[])
    }

    /// Get current git state (convenience)
    pub fn git(&self) -> Option<&super::super::state::GitState> {
        self.current_repo().map(|r| &r.git)
    }

    /// Get current git state (mutable, convenience)
    pub fn git_mut(&mut self) -> Option<&mut super::super::state::GitState> {
        self.current_repo_mut().map(|r| &mut r.git)
    }

    /// Get current diff state (convenience)
    pub fn diff(&self) -> Option<&super::super::state::DiffState> {
        self.current_repo().map(|r| &r.diff)
    }

    /// Get current diff state (mutable, convenience)
    pub fn diff_mut(&mut self) -> Option<&mut super::super::state::DiffState> {
        self.current_repo_mut().map(|r| &mut r.diff)
    }

    /// Get add_worktree_idx (convenience)
    pub fn add_worktree_idx(&self) -> usize {
        self.current_repo().map(|r| r.add_worktree_idx).unwrap_or(0)
    }

    // ============ Repo State Mutation Helpers ============

    /// Set branch_idx in current repo
    pub fn set_branch_idx(&mut self, idx: usize) {
        if let Some(repo) = self.current_repo_mut() {
            repo.branch_idx = idx;
        }
    }

    /// Set session_idx in current repo
    pub fn set_session_idx(&mut self, idx: usize) {
        if let Some(repo) = self.current_repo_mut() {
            repo.session_idx = idx;
        }
    }

    /// Set add_worktree_idx in current repo
    pub fn set_add_worktree_idx(&mut self, idx: usize) {
        if let Some(repo) = self.current_repo_mut() {
            repo.add_worktree_idx = idx;
        }
    }

    // ============ Refresh Operations ============

    /// Refresh all data (repos, branches, sessions)
    pub async fn refresh_all(&mut self) -> Result<()> {
        self.error_message = None;

        // Load repos from daemon
        let repos = self.client.list_repos().await?;

        // Update repo_order
        self.repo_order = repos.iter().map(|r| r.id.clone()).collect();

        // Add new repos, keep existing state for known repos
        for repo_info in repos {
            self.repo_states
                .entry(repo_info.id.clone())
                .and_modify(|r| r.info = repo_info.clone())
                .or_insert_with(|| RepoState::new(repo_info));
        }

        // Remove deleted repos
        self.repo_states
            .retain(|id, _| self.repo_order.contains(id));

        // Mark sidebar as dirty to trigger redraw
        self.dirty.sidebar = true;

        // Set current repo if not set or if current is no longer valid
        if self.current_repo_id.is_none()
            || !self
                .repo_order
                .contains(self.current_repo_id.as_ref().unwrap_or(&String::new()))
        {
            self.current_repo_id = self.repo_order.first().cloned();
        }

        // Load branches for current repo
        self.refresh_branches().await?;

        Ok(())
    }

    /// Refresh worktrees for current repo
    pub async fn refresh_branches(&mut self) -> Result<()> {
        // Get repo_id first to avoid borrow issues
        let repo_id = match self.current_repo_id.clone() {
            Some(id) => id,
            None => return Ok(()),
        };

        // Fetch worktrees from daemon
        let all_branches = self.client.list_worktrees(&repo_id).await?;

        // Update the repo state
        if let Some(repo) = self.repo_states.get_mut(&repo_id) {
            // Split into worktrees (has path) and available branches (no path)
            repo.worktrees = all_branches
                .iter()
                .filter(|b| !b.path.is_empty())
                .cloned()
                .collect();
            repo.available_branches = all_branches
                .into_iter()
                .filter(|b| b.path.is_empty())
                .collect();

            // Clamp indices to valid ranges
            repo.clamp_indices();

            // Filter expanded worktrees to only include valid indices
            repo.expanded_worktrees
                .retain(|&idx| idx < repo.worktrees.len());

            // Load sessions for expanded worktrees
            let expanded_to_load: Vec<usize> = repo.expanded_worktrees.iter().cloned().collect();
            for wt_idx in expanded_to_load {
                let _ = self.load_worktree_sessions(wt_idx).await;
            }
        }

        // Update sidebar total items count
        self.update_sidebar_total_items();

        // Mark sidebar as dirty to trigger redraw
        self.dirty.sidebar = true;

        // Load sessions for current branch
        self.refresh_sessions().await?;

        // Load git status for current worktree
        let _ = self.load_git_status().await;

        Ok(())
    }

    /// Refresh sessions for current branch
    pub async fn refresh_sessions(&mut self) -> Result<()> {
        // Get repo_id and branch info first to avoid borrow issues
        let (repo_id, branch_name) = {
            if let Some(repo) = self.current_repo() {
                if let Some(wt) = repo.current_worktree() {
                    (repo.info.id.clone(), wt.branch.clone())
                } else {
                    return Ok(());
                }
            } else {
                return Ok(());
            }
        };

        // Fetch sessions from daemon
        let sessions = self
            .client
            .list_sessions(Some(&repo_id), Some(&branch_name))
            .await?;

        // Update repo state
        if let Some(repo) = self.repo_states.get_mut(&repo_id) {
            repo.sessions = sessions;
            // Clamp session index
            if !repo.sessions.is_empty() && repo.session_idx >= repo.sessions.len() {
                repo.session_idx = repo.sessions.len() - 1;
            }
        }

        // Mark sidebar as dirty to trigger redraw
        self.dirty.sidebar = true;

        // Update active session for preview
        self.update_active_session().await;

        Ok(())
    }

    /// Update active session based on current selection
    pub(super) async fn update_active_session(&mut self) {
        let new_session_id = self.current_session().map(|s| s.id.clone());

        // If session changed, disconnect old stream and connect new one
        if self.terminal.active_session_id != new_session_id {
            self.disconnect_stream();

            // Save current parser to map if there's an active session
            if let Some(old_id) = &self.terminal.active_session_id {
                self.terminal
                    .session_parsers
                    .insert(old_id.clone(), self.terminal.parser.clone());
            }

            // Get or create parser for new session
            if let Some(new_id) = &new_session_id {
                self.terminal.parser = self
                    .terminal
                    .session_parsers
                    .entry(new_id.clone())
                    .or_insert_with(|| Arc::new(Mutex::new(vt100::Parser::new(24, 80, 10000))))
                    .clone();
            } else {
                // No session selected, use a fresh parser
                self.terminal.parser = Arc::new(Mutex::new(vt100::Parser::new(24, 80, 10000)));
            }

            self.terminal.scroll_offset = 0;
            self.terminal.active_session_id = new_session_id;

            // Auto-connect for preview if there's a session
            if self.terminal.active_session_id.is_some() {
                let _ = self.connect_stream().await;
            }
        }
    }

    /// Update total_items count in sidebar based on current worktrees and expanded state
    /// Note: This is now a no-op since total_items is calculated on-demand
    pub(super) fn update_sidebar_total_items(&mut self) {
        // Total items are now calculated on-demand via repo.calculate_sidebar_total()
    }

    /// Load sessions for a specific worktree (for tree view)
    pub async fn load_worktree_sessions(&mut self, wt_idx: usize) -> Result<()> {
        // Get repo_id and branch first to avoid borrow issues
        let (repo_id, branch) = {
            if let Some(repo) = self.current_repo() {
                if let Some(wt) = repo.worktrees.get(wt_idx) {
                    (repo.info.id.clone(), wt.branch.clone())
                } else {
                    return Ok(());
                }
            } else {
                return Ok(());
            }
        };

        let sessions = self
            .client
            .list_sessions(Some(&repo_id), Some(&branch))
            .await?;

        // Store sessions in repo state
        if let Some(repo) = self.current_repo_mut() {
            repo.sessions_by_worktree.insert(wt_idx, sessions);
        }
        self.update_sidebar_total_items();
        self.dirty.sidebar = true;
        Ok(())
    }

    /// Switch to repo by index (sync version)
    /// Saves current repo's view state and restores the target repo's state
    pub fn switch_repo_sync(&mut self, idx: usize) -> Option<AsyncAction> {
        // Get new repo ID from repo_order
        let new_id = self.repo_order.get(idx).cloned();

        // Check if we're actually switching to a different repo
        if new_id.is_some() && new_id != self.current_repo_id {
            // Switch to new repo - state is already preserved in repo_states!
            // Sidebar cursor is stored per-repo, no need to sync
            self.current_repo_id = new_id;

            // Update sidebar total items
            self.update_sidebar_total_items();
            self.dirty.sidebar = true;

            // Refresh branches to ensure data is fresh
            return Some(AsyncAction::RefreshBranches);
        }

        None
    }

    /// Subscribe to daemon events
    pub(super) async fn subscribe_events(&mut self) {
        use tokio::sync::mpsc;
        use tokio_stream::StreamExt;

        debug!("Subscribing to daemon events");
        match self.client.subscribe_events(None).await {
            Ok(mut stream) => {
                debug!("Event subscription successful");
                let (tx, rx) = mpsc::channel(64);
                self.event_rx = Some(rx);

                // Spawn task to receive events and forward to channel
                tokio::spawn(async move {
                    while let Some(Ok(event)) = stream.next().await {
                        if tx.send(event).await.is_err() {
                            // Receiver dropped, exit
                            break;
                        }
                    }
                    debug!("Event stream ended");
                });
            }
            Err(e) => {
                // Non-fatal: fall back to polling
                tracing::warn!("Failed to subscribe to events: {}", e);
            }
        }
    }
}
