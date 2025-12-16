//! Input form handling

use super::super::state::{
    AsyncAction, DeleteTarget, ExitCleanupAction, Focus, InputMode, SavedFocusState, SidebarItem,
};
use super::super::widgets::VirtualList;
use super::super::App;
use crate::error::TuiError;
use amux_config::{DEFAULT_SCROLLBACK, DEFAULT_TERMINAL_COLS, DEFAULT_TERMINAL_ROWS};
use std::sync::{Arc, Mutex};

type Result<T> = std::result::Result<T, TuiError>;

impl App {
    /// Save current focus before opening a dialog/popup
    pub fn save_focus(&mut self) {
        let terminal_mode = if self.focus == Focus::Terminal {
            Some(self.terminal.mode)
        } else {
            None
        };
        self.saved_focus_stack.push(SavedFocusState {
            focus: self.focus.clone(),
            terminal_mode,
        });
    }

    /// Restore focus after closing a dialog/popup
    /// Returns true if focus was restored, false if stack was empty
    pub fn restore_focus(&mut self) -> bool {
        if let Some(saved) = self.saved_focus_stack.pop() {
            self.focus = saved.focus;
            // When restoring to Terminal, restore the saved terminal mode
            if let Some(mode) = saved.terminal_mode {
                self.terminal.mode = mode;
            }
            true
        } else {
            // Stack empty - graceful degradation
            tracing::debug!("Focus stack empty during restore_focus()");
            false
        }
    }

    /// Cancel input mode and restore focus
    pub fn cancel_input(&mut self) {
        self.input_mode = InputMode::Normal;
        self.text_input.clear();
        self.status_message = None;

        // Restore focus when canceling
        self.restore_focus();
    }

    /// Start add worktree mode
    pub fn start_add_worktree(&mut self) {
        // Get current selected branch as base (None = use HEAD)
        let base_branch = self.current_worktree().map(|w| w.branch.clone());

        self.input_mode = InputMode::AddWorktree { base_branch };
        self.text_input.clear();
        self.set_add_worktree_idx(0);
    }

    /// Start select provider mode for new session creation
    pub fn start_select_provider(&mut self, repo_id: String, branch: String) {
        self.save_focus();
        self.input_mode = InputMode::SelectProvider {
            repo_id,
            branch,
            providers: vec![],
            selected_index: 0,
            loading: true,
        };
    }

    /// Fetch available providers from daemon
    pub async fn fetch_providers(&mut self, _repo_id: &str, _branch: &str) -> Result<()> {
        match self.client.list_providers().await {
            Ok(provider_infos) => {
                let provider_names: Vec<String> =
                    provider_infos.iter().map(|p| p.name.clone()).collect();
                let provider_count = provider_names.len();

                // Update the input mode with fetched providers
                if let InputMode::SelectProvider {
                    ref mut providers,
                    ref mut loading,
                    ..
                } = &mut self.input_mode
                {
                    *providers = provider_names;
                    *loading = false;

                    if providers.is_empty() {
                        self.status_message = Some("No providers available".to_string());
                        self.input_mode = InputMode::Normal;
                        self.restore_focus();
                    } else {
                        self.status_message = Some(format!("Loaded {} providers", provider_count));
                    }
                }
            }
            Err(e) => {
                self.status_message = Some(format!("Failed to fetch providers: {}", e));
                self.input_mode = InputMode::Normal;
                self.restore_focus();
            }
        }
        Ok(())
    }

    /// Submit provider selection and create session
    pub async fn submit_provider_selection(&mut self) -> Result<()> {
        let (repo_id, branch, provider) = match &self.input_mode {
            InputMode::SelectProvider {
                repo_id,
                branch,
                providers,
                selected_index,
                loading,
            } => {
                if *loading {
                    return Ok(()); // Still loading, ignore submit
                }

                if providers.is_empty() {
                    self.status_message = Some("No provider selected".to_string());
                    return Ok(());
                }

                let provider = providers.get(*selected_index).cloned().unwrap_or_else(|| {
                    // Should not happen if providers is not empty
                    providers.first().cloned().unwrap_or_default()
                });

                if provider.is_empty() {
                    self.status_message = Some("Invalid provider selected".to_string());
                    return Ok(());
                }

                (repo_id.clone(), branch.clone(), provider)
            }
            _ => return Ok(()),
        };

        self.input_mode = InputMode::Normal;
        self.restore_focus();

        // Update status message to show which provider was selected
        self.status_message = Some(format!("Creating session with {}...", provider));

        // Get terminal size for PTY creation
        let (inner_rows, inner_cols) = self.get_inner_terminal_size();

        // Create session with selected provider
        match self
            .client
            .create_session(
                &repo_id,
                &branch,
                None,
                None,
                None,
                None,
                Some(&provider),
                Some(inner_rows as u32),
                Some(inner_cols as u32),
            )
            .await
        {
            Ok(session) => {
                // Refresh sessions for this worktree
                let b_idx = self.branch_idx();
                self.load_worktree_sessions(b_idx).await?;
                // Expand worktree
                if let Some(repo) = self.current_repo_mut() {
                    repo.expanded_worktrees.insert(b_idx);
                }
                self.update_sidebar_total_items();

                // Update sidebar cursor to point to the new session
                if let Some(repo) = self.current_repo_mut() {
                    let session_idx = repo
                        .sessions_by_worktree
                        .get(&b_idx)
                        .and_then(|sessions| sessions.iter().position(|s| s.id == session.id));

                    if let Some(s_idx) = session_idx {
                        let mut cursor_pos = 0;
                        for wt_idx in 0..b_idx {
                            cursor_pos += 1;
                            if repo.expanded_worktrees.contains(&wt_idx) {
                                if let Some(sessions) = repo.sessions_by_worktree.get(&wt_idx) {
                                    cursor_pos += sessions.len();
                                }
                            }
                        }
                        cursor_pos += 1;
                        cursor_pos += s_idx;
                        repo.sidebar_cursor = cursor_pos;
                    }
                }

                // Disconnect current stream
                self.disconnect_stream();

                // Save current parser if there was an active session
                if let Some(old_id) = &self.terminal.active_session_id {
                    self.terminal
                        .session_parsers
                        .insert(old_id.clone(), self.terminal.parser.clone());
                }

                // Create new parser for the new session
                self.terminal.parser = Arc::new(Mutex::new(vt100::Parser::new(
                    DEFAULT_TERMINAL_ROWS,
                    DEFAULT_TERMINAL_COLS,
                    DEFAULT_SCROLLBACK,
                )));
                self.terminal
                    .session_parsers
                    .insert(session.id.clone(), self.terminal.parser.clone());
                self.terminal.scroll_offset = 0;
                self.terminal.active_session_id = Some(session.id.clone());

                self.enter_terminal().await?;
            }
            Err(e) => {
                self.error_message = Some(e.to_string());
            }
        }

        Ok(())
    }

    /// Start rename session mode
    pub fn start_rename_session(&mut self) {
        // Get session from current sidebar item (tree view uses sessions_by_worktree)
        let item = self.current_sidebar_item();
        tracing::debug!("start_rename_session: current_sidebar_item = {:?}", item);

        let session = match item {
            SidebarItem::Session(wt_idx, s_idx) => {
                let repo = self.current_repo();
                tracing::debug!(
                    "start_rename_session: wt_idx={}, s_idx={}, repo={:?}",
                    wt_idx,
                    s_idx,
                    repo.map(|r| &r.info.id)
                );
                if let Some(repo) = repo {
                    tracing::debug!(
                        "start_rename_session: sessions_by_worktree keys = {:?}",
                        repo.sessions_by_worktree.keys().collect::<Vec<_>>()
                    );
                    if let Some(sessions) = repo.sessions_by_worktree.get(&wt_idx) {
                        tracing::debug!(
                            "start_rename_session: found {} sessions for wt_idx={}",
                            sessions.len(),
                            wt_idx
                        );
                        sessions.get(s_idx).cloned()
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => {
                tracing::debug!("start_rename_session: not a session item");
                None
            }
        };

        if let Some(session) = session {
            tracing::debug!("start_rename_session: renaming session {}", session.id);
            self.save_focus();
            self.input_mode = InputMode::RenameSession {
                session_id: session.id.clone(),
            };
            self.text_input.set_content(session.name.clone());
        } else {
            tracing::debug!("start_rename_session: no session found");
            self.error_message = Some("No session selected".to_string());
        }
    }

    /// Submit rename session
    pub async fn submit_rename_session(&mut self) -> Result<()> {
        let session_id = match &self.input_mode {
            InputMode::RenameSession { session_id } => session_id.clone(),
            _ => return Ok(()),
        };

        let new_name = self.text_input.trim().to_string();
        self.input_mode = InputMode::Normal;
        self.text_input.clear();
        self.restore_focus();

        if new_name.is_empty() {
            self.error_message = Some("Session name cannot be empty".to_string());
            return Ok(());
        }

        match self.client.rename_session(&session_id, &new_name).await {
            Ok(_) => {
                self.status_message = Some(format!("Renamed session to: {}", new_name));
                // Refresh sessions from server to ensure UI is updated
                self.refresh_sessions().await?;
                // Also refresh worktree sessions for tree view
                self.load_worktree_sessions(self.branch_idx()).await?;
            }
            Err(e) => {
                self.error_message = Some(e.to_string());
            }
        }

        Ok(())
    }

    /// Submit add worktree (create worktree for selected or new branch)
    pub async fn submit_add_worktree(&mut self) -> Result<()> {
        // Get base_branch from input mode before clearing
        let base_branch = match &self.input_mode {
            InputMode::AddWorktree { base_branch } => base_branch.clone(),
            _ => None,
        };

        // Determine branch name: typed input or selected from list
        // Only use base_branch when creating a NEW branch (typing in input)
        let (branch_name, use_base) = if !self.text_input.is_empty() {
            // Creating new branch - use base_branch
            (self.text_input.trim().to_string(), true)
        } else if let Some(branch) = self.available_branches().get(self.add_worktree_idx()) {
            // Selecting existing branch - no need for base
            (branch.branch.clone(), false)
        } else {
            self.cancel_input();
            return Ok(());
        };

        let repo_id = match self.current_repo() {
            Some(repo) => repo.info.id.clone(),
            None => {
                self.cancel_input();
                return Ok(());
            }
        };

        self.input_mode = InputMode::Normal;
        self.text_input.clear();
        self.restore_focus();

        // Create worktree (pass base_branch only when creating new branch)
        let base = if use_base {
            base_branch.as_deref()
        } else {
            None
        };
        match self
            .client
            .create_worktree(&repo_id, &branch_name, base)
            .await
        {
            Ok(_) => {
                self.status_message = Some(format!("Created worktree for: {}", branch_name));
                self.refresh_branches().await?;
                // Select the new worktree
                if let Some(idx) = self
                    .worktrees()
                    .iter()
                    .position(|w| w.branch == branch_name)
                {
                    self.set_branch_idx(idx);
                    if let Some(repo) = self.current_repo_mut() {
                        repo.sidebar_cursor = idx;
                    }
                    self.refresh_sessions().await?;
                }
            }
            Err(e) => {
                self.error_message = Some(e.to_string());
            }
        }

        Ok(())
    }

    /// Submit the input buffer (for new branch creation)
    pub async fn submit_input(&mut self) -> Result<()> {
        if self.input_mode != InputMode::NewBranch {
            return Ok(());
        }

        let branch_name = self.text_input.trim().to_string();
        self.input_mode = InputMode::Normal;
        self.text_input.clear();
        self.status_message = None;

        if branch_name.is_empty() {
            self.error_message = Some("Branch name cannot be empty".to_string());
            return Ok(());
        }

        // Create session (will auto-create worktree if needed)
        if let Some(repo) = self.current_repo().map(|r| r.info.clone()) {
            let (inner_rows, inner_cols) = self.get_inner_terminal_size();
            match self
                .client
                .create_session(
                    &repo.id,
                    &branch_name,
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some(inner_rows as u32),
                    Some(inner_cols as u32),
                )
                .await
            {
                Ok(session) => {
                    self.refresh_branches().await?;
                    // Find the branch and session
                    if let Some(b_idx) = self
                        .worktrees()
                        .iter()
                        .position(|b| b.branch == branch_name)
                    {
                        self.set_branch_idx(b_idx);
                        self.refresh_sessions().await?;
                        if let Some(s_idx) = self.sessions().iter().position(|s| s.id == session.id)
                        {
                            self.set_session_idx(s_idx);
                            self.update_active_session().await;
                            // Also load sessions for tree view
                            self.load_worktree_sessions(b_idx).await?;
                            if let Some(repo) = self.current_repo_mut() {
                                repo.expanded_worktrees.insert(b_idx);
                            }
                            self.update_sidebar_total_items();
                            self.focus = Focus::Sidebar;
                            self.enter_terminal().await?;
                        }
                    }
                }
                Err(e) => {
                    self.error_message = Some(e.to_string());
                }
            }
        }
        Ok(())
    }

    /// Start create session input mode (prompt for session name in status bar)
    pub fn start_create_session_input(&mut self) {
        // Get current repo and branch
        let (repo_id, branch) = match (
            self.current_repo().map(|r| r.info.id.clone()),
            self.current_worktree().map(|w| w.branch.clone()),
        ) {
            (Some(repo_id), Some(branch)) => (repo_id, branch),
            _ => {
                self.error_message = Some("No worktree selected".to_string());
                return;
            }
        };

        self.save_focus();
        self.input_mode = InputMode::CreateSessionInput {
            repo_id,
            branch,
            provider: None,
        };
        self.text_input.clear();
    }

    /// Submit create session with name from input
    pub async fn submit_create_session_input(&mut self) -> Result<()> {
        let (repo_id, branch, provider) = match &self.input_mode {
            InputMode::CreateSessionInput {
                repo_id,
                branch,
                provider,
            } => (repo_id.clone(), branch.clone(), provider.clone()),
            _ => return Ok(()),
        };

        // Get name from input (None if empty for default name)
        let name = {
            let trimmed = self.text_input.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        };

        self.input_mode = InputMode::Normal;
        self.text_input.clear();
        self.restore_focus();

        // Get terminal size for PTY creation
        let (inner_rows, inner_cols) = self.get_inner_terminal_size();

        // Create session with optional name and provider
        match self
            .client
            .create_session(
                &repo_id,
                &branch,
                name.as_deref(),
                None,
                None,
                None,
                provider.as_deref(),
                Some(inner_rows as u32),
                Some(inner_cols as u32),
            )
            .await
        {
            Ok(session) => {
                // Refresh sessions for this worktree
                let b_idx = self.branch_idx();
                self.load_worktree_sessions(b_idx).await?;
                // Expand worktree
                if let Some(repo) = self.current_repo_mut() {
                    repo.expanded_worktrees.insert(b_idx);
                }
                self.update_sidebar_total_items();

                // Update sidebar cursor to point to the new session
                if let Some(repo) = self.current_repo_mut() {
                    let session_idx = repo
                        .sessions_by_worktree
                        .get(&b_idx)
                        .and_then(|sessions| sessions.iter().position(|s| s.id == session.id));

                    if let Some(s_idx) = session_idx {
                        let mut cursor_pos = 0;
                        for wt_idx in 0..b_idx {
                            cursor_pos += 1;
                            if repo.expanded_worktrees.contains(&wt_idx) {
                                if let Some(sessions) = repo.sessions_by_worktree.get(&wt_idx) {
                                    cursor_pos += sessions.len();
                                }
                            }
                        }
                        cursor_pos += 1;
                        cursor_pos += s_idx;
                        repo.sidebar_cursor = cursor_pos;
                    }
                }

                // Disconnect current stream
                self.disconnect_stream();

                // Save current parser if there was an active session
                if let Some(old_id) = &self.terminal.active_session_id {
                    self.terminal
                        .session_parsers
                        .insert(old_id.clone(), self.terminal.parser.clone());
                }

                // Create new parser for the new session
                self.terminal.parser = Arc::new(Mutex::new(vt100::Parser::new(
                    DEFAULT_TERMINAL_ROWS,
                    DEFAULT_TERMINAL_COLS,
                    DEFAULT_SCROLLBACK,
                )));
                self.terminal
                    .session_parsers
                    .insert(session.id.clone(), self.terminal.parser.clone());
                self.terminal.scroll_offset = 0;
                self.terminal.active_session_id = Some(session.id.clone());

                self.enter_terminal().await?;
            }
            Err(e) => {
                self.error_message = Some(e.to_string());
            }
        }

        Ok(())
    }

    /// Create new session and enter interactive mode
    pub async fn create_new(&mut self) -> Result<()> {
        match self.focus {
            Focus::Sidebar => {
                // In tree view: create session for currently selected worktree
                if let (Some(repo), Some(branch)) = (
                    self.current_repo().map(|r| r.info.clone()),
                    self.current_worktree().cloned(),
                ) {
                    let (inner_rows, inner_cols) = self.get_inner_terminal_size();
                    match self
                        .client
                        .create_session(
                            &repo.id,
                            &branch.branch,
                            None,
                            None,
                            None,
                            None,
                            None,
                            Some(inner_rows as u32),
                            Some(inner_cols as u32),
                        )
                        .await
                    {
                        Ok(session) => {
                            // Refresh sessions for this worktree
                            let b_idx = self.branch_idx();
                            self.load_worktree_sessions(b_idx).await?;
                            // Expand worktree
                            if let Some(repo) = self.current_repo_mut() {
                                repo.expanded_worktrees.insert(b_idx);
                            }
                            self.update_sidebar_total_items();

                            // Update sidebar cursor to point to the new session
                            // Calculate position: worktree position + 1 + session index within worktree
                            if let Some(repo) = self.current_repo_mut() {
                                // Find the new session's index in sessions_by_worktree
                                let session_idx =
                                    repo.sessions_by_worktree.get(&b_idx).and_then(|sessions| {
                                        sessions.iter().position(|s| s.id == session.id)
                                    });

                                if let Some(s_idx) = session_idx {
                                    // Calculate cursor position
                                    let mut cursor_pos = 0;
                                    for wt_idx in 0..b_idx {
                                        cursor_pos += 1; // worktree itself
                                        if repo.expanded_worktrees.contains(&wt_idx) {
                                            if let Some(sessions) =
                                                repo.sessions_by_worktree.get(&wt_idx)
                                            {
                                                cursor_pos += sessions.len();
                                            }
                                        }
                                    }
                                    cursor_pos += 1; // current worktree
                                    cursor_pos += s_idx; // session position within worktree

                                    repo.sidebar_cursor = cursor_pos;
                                }
                            }

                            // Disconnect current stream
                            self.disconnect_stream();

                            // Save current parser if there was an active session
                            if let Some(old_id) = &self.terminal.active_session_id {
                                self.terminal
                                    .session_parsers
                                    .insert(old_id.clone(), self.terminal.parser.clone());
                            }

                            // Create new parser for the new session
                            self.terminal.parser = Arc::new(Mutex::new(vt100::Parser::new(
                                DEFAULT_TERMINAL_ROWS,
                                DEFAULT_TERMINAL_COLS,
                                DEFAULT_SCROLLBACK,
                            )));
                            self.terminal
                                .session_parsers
                                .insert(session.id.clone(), self.terminal.parser.clone());
                            self.terminal.scroll_offset = 0;
                            self.terminal.active_session_id = Some(session.id.clone());

                            self.enter_terminal().await?;
                        }
                        Err(e) => {
                            self.error_message = Some(e.to_string());
                        }
                    }
                }
            }
            Focus::Terminal | Focus::DiffFiles | Focus::GitStatus => {}
        }
        Ok(())
    }

    /// Request deletion (enters confirm mode)
    /// Note: Caller should call save_focus() before this if needed
    pub fn request_delete(&mut self) {
        match self.focus {
            Focus::Sidebar => {
                // In tree view: delete based on current selection
                match self.current_sidebar_item() {
                    SidebarItem::Worktree(wt_idx) => {
                        if let (Some(repo), Some(wt)) = (
                            self.current_repo().map(|r| r.info.clone()),
                            self.worktrees().get(wt_idx).cloned(),
                        ) {
                            if wt.is_main {
                                self.error_message =
                                    Some("Cannot remove main worktree".to_string());
                            } else if wt.path.is_empty() {
                                self.error_message = Some("No worktree to remove".to_string());
                            } else if wt.session_count > 0 {
                                self.input_mode = InputMode::ConfirmDeleteWorktreeSessions {
                                    repo_id: repo.id,
                                    branch: wt.branch,
                                    session_count: wt.session_count,
                                };
                            } else {
                                self.input_mode =
                                    InputMode::ConfirmDelete(DeleteTarget::Worktree {
                                        repo_id: repo.id,
                                        branch: wt.branch,
                                    });
                            }
                        }
                    }
                    SidebarItem::Session(wt_idx, s_idx) => {
                        if let Some(repo) = self.current_repo() {
                            if let Some(sessions) = repo.sessions_by_worktree.get(&wt_idx) {
                                if let Some(session) = sessions.get(s_idx) {
                                    self.input_mode =
                                        InputMode::ConfirmDelete(DeleteTarget::Session {
                                            session_id: session.id.clone(),
                                            name: session.name.clone(),
                                        });
                                }
                            }
                        }
                    }
                    SidebarItem::None => {}
                }
            }
            Focus::Terminal | Focus::DiffFiles | Focus::GitStatus => {}
        }
    }

    /// Confirm and execute deletion
    pub async fn confirm_delete(
        &mut self,
        target: DeleteTarget,
        action: ExitCleanupAction,
    ) -> Result<()> {
        self.input_mode = InputMode::Normal;

        match target {
            DeleteTarget::Worktree { repo_id, branch } => {
                match self.client.remove_worktree(&repo_id, &branch).await {
                    Ok(_) => {
                        // After removing worktree, ask if user wants to delete branch too
                        // Don't restore focus yet - we're chaining to another dialog
                        self.input_mode = InputMode::ConfirmDeleteBranch(branch);
                        self.refresh_branches().await?;
                    }
                    Err(e) => {
                        self.error_message = Some(e.to_string());
                        self.restore_focus();
                    }
                }
            }
            DeleteTarget::Session { session_id, name } => {
                // Disconnect if this is the active session
                if self.terminal.active_session_id.as_ref() == Some(&session_id) {
                    self.disconnect_stream();
                }

                // Execute action based on user selection
                let result = match action {
                    ExitCleanupAction::Destroy => self
                        .client
                        .destroy_session(&session_id)
                        .await
                        .map(|_| format!("Destroyed session: {}", name)),
                    ExitCleanupAction::Stop => self
                        .client
                        .stop_session(&session_id)
                        .await
                        .map(|_| format!("Stopped session: {}", name)),
                };

                match result {
                    Ok(msg) => {
                        self.status_message = Some(msg);
                        self.refresh_sessions().await?;
                        // Also refresh worktree sessions for tree view
                        self.load_worktree_sessions(self.branch_idx()).await?;
                        self.restore_focus();
                    }
                    Err(e) => {
                        self.error_message = Some(e.to_string());
                        self.restore_focus();
                    }
                }
            }
        }

        Ok(())
    }

    /// Confirm deletion of sessions and worktree
    pub async fn confirm_delete_worktree_sessions(&mut self) -> Result<()> {
        let (repo_id, branch) = match &self.input_mode {
            InputMode::ConfirmDeleteWorktreeSessions {
                repo_id, branch, ..
            } => (repo_id.clone(), branch.clone()),
            _ => return Ok(()),
        };

        // Get sessions for this worktree
        let sessions = self
            .client
            .list_sessions(Some(&repo_id), Some(&branch))
            .await?;

        // Delete all sessions
        for session in sessions {
            // Disconnect if this is the active session
            if self.terminal.active_session_id.as_ref() == Some(&session.id) {
                self.disconnect_stream();
            }
            self.client.destroy_session(&session.id).await?;
        }

        // Now proceed to delete worktree (show confirmation for worktree deletion)
        self.input_mode = InputMode::ConfirmDelete(DeleteTarget::Worktree { repo_id, branch });

        // Refresh sessions to update the UI
        self.refresh_sessions().await?;
        // Also refresh worktree sessions for tree view
        self.load_worktree_sessions(self.branch_idx()).await?;

        Ok(())
    }

    /// Confirm and delete branch (called after worktree deletion)
    pub async fn confirm_delete_branch(&mut self) -> Result<()> {
        let branch_name = match &self.input_mode {
            InputMode::ConfirmDeleteBranch(b) => b.clone(),
            _ => return Ok(()),
        };

        self.input_mode = InputMode::Normal;
        self.restore_focus();

        // Get repo_id
        let repo_id = match self.current_repo() {
            Some(repo) => repo.info.id.clone(),
            None => return Ok(()),
        };

        // Delete branch via daemon
        match self.client.delete_branch(&repo_id, &branch_name).await {
            Ok(_) => {
                self.status_message = Some(format!("Deleted branch: {}", branch_name));
                self.refresh_branches().await?;
            }
            Err(e) => {
                self.error_message = Some(format!("Failed to delete branch: {}", e));
            }
        }

        Ok(())
    }

    // ========== Sidebar Navigation ==========

    /// Get the current sidebar item at cursor position
    pub fn current_sidebar_item(&self) -> SidebarItem {
        let Some(repo) = self.current_repo() else {
            return SidebarItem::None;
        };

        let cursor = repo.sidebar_cursor;
        let mut pos = 0;
        for (wt_idx, _wt) in repo.worktrees.iter().enumerate() {
            if pos == cursor {
                return SidebarItem::Worktree(wt_idx);
            }
            pos += 1;
            if repo.expanded_worktrees.contains(&wt_idx) {
                if let Some(sessions) = repo.sessions_by_worktree.get(&wt_idx) {
                    for (s_idx, _session) in sessions.iter().enumerate() {
                        if pos == cursor {
                            return SidebarItem::Session(wt_idx, s_idx);
                        }
                        pos += 1;
                    }
                }
            }
        }
        SidebarItem::None
    }

    /// Toggle expansion of current worktree
    pub fn toggle_sidebar_expand(&mut self) -> Option<AsyncAction> {
        let item = self.current_sidebar_item();
        if let SidebarItem::Worktree(wt_idx) = item {
            if let Some(repo) = self.current_repo_mut() {
                if repo.expanded_worktrees.contains(&wt_idx) {
                    repo.expanded_worktrees.remove(&wt_idx);
                } else {
                    repo.expanded_worktrees.insert(wt_idx);
                    // Load sessions for this worktree if not loaded
                    if !repo.sessions_by_worktree.contains_key(&wt_idx) {
                        self.update_sidebar_total_items();
                        return Some(AsyncAction::LoadWorktreeSessions { wt_idx });
                    }
                }
            }
            self.update_sidebar_total_items();
            self.dirty.sidebar = true;
        }
        None
    }

    /// Move cursor up in sidebar tree view
    pub fn sidebar_move_up(&mut self) -> Option<AsyncAction> {
        let moved = self
            .current_repo_mut()
            .map(|r| r.move_up())
            .unwrap_or(false);
        if moved {
            self.dirty.sidebar = true;
            if self.update_selection_from_sidebar() {
                return Some(AsyncAction::LoadGitStatus);
            }
        }
        None
    }

    /// Move cursor down in sidebar tree view
    pub fn sidebar_move_down(&mut self) -> Option<AsyncAction> {
        let moved = self
            .current_repo_mut()
            .map(|r| r.move_down())
            .unwrap_or(false);
        if moved {
            self.dirty.sidebar = true;
            if self.update_selection_from_sidebar() {
                return Some(AsyncAction::LoadGitStatus);
            }
        }
        None
    }

    /// Update branch_idx and session_idx based on sidebar cursor
    /// Returns true if the worktree changed (needs git status refresh)
    pub(super) fn update_selection_from_sidebar(&mut self) -> bool {
        let old_branch_idx = self.branch_idx();

        match self.current_sidebar_item() {
            SidebarItem::Worktree(wt_idx) => {
                self.set_branch_idx(wt_idx);
                self.set_session_idx(0);
                // Don't clear active session when navigating to worktree
                // Keep showing the current terminal content
            }
            SidebarItem::Session(wt_idx, s_idx) => {
                self.set_branch_idx(wt_idx);
                self.set_session_idx(s_idx);
                // Get session id from the correct source (RepoState, not SidebarState)
                let session_id = self
                    .current_repo()
                    .and_then(|repo| repo.sessions_by_worktree.get(&wt_idx))
                    .and_then(|sessions| sessions.get(s_idx))
                    .map(|s| s.id.clone());

                if let Some(new_id) = session_id {
                    if self.terminal.active_session_id.as_ref() != Some(&new_id) {
                        self.disconnect_stream();

                        // Save current parser if there was an active session
                        if let Some(old_id) = &self.terminal.active_session_id {
                            self.terminal
                                .session_parsers
                                .insert(old_id.clone(), self.terminal.parser.clone());
                        }

                        // Get or create parser for new session
                        self.terminal.parser = self
                            .terminal
                            .session_parsers
                            .entry(new_id.clone())
                            .or_insert_with(|| {
                                Arc::new(Mutex::new(vt100::Parser::new(
                                    DEFAULT_TERMINAL_ROWS,
                                    DEFAULT_TERMINAL_COLS,
                                    DEFAULT_SCROLLBACK,
                                )))
                            })
                            .clone();

                        self.terminal.active_session_id = Some(new_id);
                        self.terminal.scroll_offset = 0;
                    }
                }
            }
            SidebarItem::None => {}
        }

        // Return true if worktree changed
        self.branch_idx() != old_branch_idx
    }

    // ========== Sync versions for responsive input handling ==========

    /// Move selection up (sync version - returns async action if needed)
    pub fn select_prev_sync(&mut self) -> Option<AsyncAction> {
        match self.focus {
            Focus::Sidebar => {
                return self.sidebar_move_up();
            }
            Focus::GitStatus => {
                self.git_status_move_up();
            }
            Focus::Terminal => {}
            Focus::DiffFiles => {
                self.diff_move_up();
            }
        }
        None
    }

    /// Move selection down (sync version - returns async action if needed)
    pub fn select_next_sync(&mut self) -> Option<AsyncAction> {
        match self.focus {
            Focus::Sidebar => {
                return self.sidebar_move_down();
            }
            Focus::GitStatus => {
                self.git_status_move_down();
            }
            Focus::Terminal => {}
            Focus::DiffFiles => {
                self.diff_move_down();
            }
        }
        None
    }
}
