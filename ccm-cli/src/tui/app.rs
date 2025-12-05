//! TUI application state machine

use crate::client::Client;
use anyhow::Result;
use ccm_proto::daemon::{AttachInput, RepoInfo, SessionInfo, WorktreeInfo};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, size, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use super::input::handle_input;
use super::ui::draw;

/// Focus position in the TUI
#[derive(Debug, Clone, PartialEq)]
pub enum Focus {
    Branches,  // Branch list in sidebar
    Sessions,  // Session list in sidebar
    Terminal,  // Terminal interaction area
}

/// Input mode for text entry
#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    Normal,
    NewBranch, // Entering new branch name
}

/// Terminal mode (vim-style)
#[derive(Debug, Clone, PartialEq)]
pub enum TerminalMode {
    Normal,  // View/scroll mode
    Insert,  // Interactive input mode
}

/// Terminal stream state for a session
pub struct TerminalStream {
    pub session_id: String,
    pub input_tx: mpsc::Sender<AttachInput>,
    pub output_rx: mpsc::Receiver<Vec<u8>>,
}

/// Deactivate fcitx5 input method
fn deactivate_ime() {
    let _ = std::process::Command::new("fcitx5-remote")
        .arg("-c")
        .output();
}

/// Activate fcitx5 input method
fn activate_ime() {
    let _ = std::process::Command::new("fcitx5-remote")
        .arg("-o")
        .output();
}

/// Application state
pub struct App {
    pub client: Client,

    // Selection indices
    pub repo_idx: usize,
    pub branch_idx: usize,
    pub session_idx: usize,

    // Focus position
    pub focus: Focus,

    // Data
    pub repos: Vec<RepoInfo>,
    pub branches: Vec<WorktreeInfo>,
    pub sessions: Vec<SessionInfo>,

    // Terminal state
    pub terminal_parser: Arc<Mutex<vt100::Parser>>,
    pub active_session_id: Option<String>,
    pub is_interactive: bool,
    pub terminal_stream: Option<TerminalStream>,
    pub terminal_mode: TerminalMode,
    pub scroll_offset: usize,
    pub terminal_fullscreen: bool,

    // UI state
    pub should_quit: bool,
    pub error_message: Option<String>,
    pub status_message: Option<String>,
    pub input_mode: InputMode,
    pub input_buffer: String,
}

impl App {
    pub async fn new(client: Client) -> Result<Self> {
        let mut app = Self {
            client,
            repo_idx: 0,
            branch_idx: 0,
            session_idx: 0,
            focus: Focus::Branches,
            repos: Vec::new(),
            branches: Vec::new(),
            sessions: Vec::new(),
            terminal_parser: Arc::new(Mutex::new(vt100::Parser::new(24, 80, 10000))),
            active_session_id: None,
            is_interactive: false,
            terminal_stream: None,
            terminal_mode: TerminalMode::Normal,
            scroll_offset: 0,
            terminal_fullscreen: false,
            should_quit: false,
            error_message: None,
            status_message: None,
            input_mode: InputMode::Normal,
            input_buffer: String::new(),
        };

        // Load initial data
        app.refresh_all().await?;

        Ok(app)
    }

    /// Refresh all data (repos, branches, sessions)
    pub async fn refresh_all(&mut self) -> Result<()> {
        self.error_message = None;

        // Load repos
        self.repos = self.client.list_repos().await?;

        // Clamp repo index
        if self.repos.is_empty() {
            self.repo_idx = 0;
            self.branches.clear();
            self.sessions.clear();
            return Ok(());
        }
        if self.repo_idx >= self.repos.len() {
            self.repo_idx = self.repos.len() - 1;
        }

        // Load branches for current repo
        self.refresh_branches().await?;

        Ok(())
    }

    /// Refresh branches for current repo
    pub async fn refresh_branches(&mut self) -> Result<()> {
        if let Some(repo) = self.repos.get(self.repo_idx) {
            self.branches = self.client.list_worktrees(&repo.id).await?;
        } else {
            self.branches.clear();
        }

        // Clamp branch index
        if self.branches.is_empty() {
            self.branch_idx = 0;
            self.sessions.clear();
            return Ok(());
        }
        if self.branch_idx >= self.branches.len() {
            self.branch_idx = self.branches.len() - 1;
        }

        // Load sessions for current branch
        self.refresh_sessions().await?;

        Ok(())
    }

    /// Refresh sessions for current branch
    pub async fn refresh_sessions(&mut self) -> Result<()> {
        if let (Some(repo), Some(branch)) = (
            self.repos.get(self.repo_idx),
            self.branches.get(self.branch_idx),
        ) {
            self.sessions = self
                .client
                .list_sessions(Some(&repo.id), Some(&branch.branch))
                .await?;
        } else {
            self.sessions.clear();
        }

        // Clamp session index
        if !self.sessions.is_empty() && self.session_idx >= self.sessions.len() {
            self.session_idx = self.sessions.len() - 1;
        }

        // Update active session for preview
        self.update_active_session().await;

        Ok(())
    }

    /// Update active session based on current selection
    async fn update_active_session(&mut self) {
        let new_session_id = self.sessions.get(self.session_idx).map(|s| s.id.clone());

        // If session changed, disconnect old stream and connect new one
        if self.active_session_id != new_session_id {
            self.disconnect_stream();
            // Clear terminal parser
            if let Ok(mut parser) = self.terminal_parser.lock() {
                *parser = vt100::Parser::new(24, 80, 10000);
            }
            self.scroll_offset = 0;
            self.active_session_id = new_session_id;

            // Auto-connect for preview if there's a session
            if self.active_session_id.is_some() {
                let _ = self.connect_stream().await;
            }
        }
    }

    /// Get current list length based on focus
    pub fn current_list_len(&self) -> usize {
        match self.focus {
            Focus::Branches => self.branches.len(),
            Focus::Sessions => self.sessions.len(),
            Focus::Terminal => 0,
        }
    }

    /// Get current selection index based on focus
    pub fn current_idx(&self) -> usize {
        match self.focus {
            Focus::Branches => self.branch_idx,
            Focus::Sessions => self.session_idx,
            Focus::Terminal => 0,
        }
    }

    /// Move selection up
    pub async fn select_prev(&mut self) {
        match self.focus {
            Focus::Branches => {
                if self.branch_idx > 0 {
                    self.branch_idx -= 1;
                    let _ = self.refresh_sessions().await;
                }
            }
            Focus::Sessions => {
                if self.session_idx > 0 {
                    self.session_idx -= 1;
                    self.update_active_session().await;
                }
            }
            Focus::Terminal => {}
        }
    }

    /// Move selection down
    pub async fn select_next(&mut self) {
        match self.focus {
            Focus::Branches => {
                if !self.branches.is_empty() && self.branch_idx < self.branches.len() - 1 {
                    self.branch_idx += 1;
                    let _ = self.refresh_sessions().await;
                }
            }
            Focus::Sessions => {
                if !self.sessions.is_empty() && self.session_idx < self.sessions.len() - 1 {
                    self.session_idx += 1;
                    self.update_active_session().await;
                }
            }
            Focus::Terminal => {}
        }
    }

    /// Switch to repo by index (1-9 keys)
    pub async fn switch_repo(&mut self, idx: usize) {
        if idx < self.repos.len() {
            self.repo_idx = idx;
            self.branch_idx = 0;
            self.session_idx = 0;
            let _ = self.refresh_branches().await;
        }
    }

    /// Toggle focus between Branches and Sessions
    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Branches => Focus::Sessions,
            Focus::Sessions => Focus::Branches,
            Focus::Terminal => Focus::Sessions,
        };
    }

    /// Enter terminal Normal mode (from Sessions)
    pub async fn enter_terminal_normal(&mut self) -> Result<()> {
        if self.active_session_id.is_some() {
            self.focus = Focus::Terminal;
            self.terminal_mode = TerminalMode::Normal;
            self.scroll_to_bottom();
            deactivate_ime();

            // Ensure stream is connected
            if self.terminal_stream.is_none() {
                self.connect_stream().await?;
            }
        }
        Ok(())
    }

    /// Enter Insert mode (from Normal mode)
    pub fn enter_insert_mode(&mut self) {
        self.terminal_mode = TerminalMode::Insert;
        self.scroll_to_bottom();
    }

    /// Exit to Normal mode (from Insert mode)
    pub fn exit_to_normal_mode(&mut self) {
        self.terminal_mode = TerminalMode::Normal;
        deactivate_ime();
    }

    /// Exit terminal mode (back to Sessions)
    pub fn exit_terminal(&mut self) {
        if self.terminal_fullscreen {
            self.terminal_fullscreen = false;
        } else {
            self.focus = Focus::Sessions;
            self.terminal_mode = TerminalMode::Normal;
            self.is_interactive = false;
        }
    }

    /// Toggle fullscreen mode
    pub fn toggle_fullscreen(&mut self) {
        self.terminal_fullscreen = !self.terminal_fullscreen;
    }

    /// Scroll up (older content)
    pub fn scroll_up(&mut self, lines: usize) {
        if let Ok(mut parser) = self.terminal_parser.lock() {
            let current = parser.screen().scrollback();
            let new_offset = current + lines;
            parser.set_scrollback(new_offset);
            self.scroll_offset = parser.screen().scrollback();
        }
    }

    /// Scroll down (newer content)
    pub fn scroll_down(&mut self, lines: usize) {
        if let Ok(mut parser) = self.terminal_parser.lock() {
            let current = parser.screen().scrollback();
            let new_offset = current.saturating_sub(lines);
            parser.set_scrollback(new_offset);
            self.scroll_offset = parser.screen().scrollback();
        }
    }

    /// Scroll to top
    pub fn scroll_to_top(&mut self) {
        if let Ok(mut parser) = self.terminal_parser.lock() {
            parser.set_scrollback(usize::MAX);
            self.scroll_offset = parser.screen().scrollback();
        }
    }

    /// Scroll to bottom
    pub fn scroll_to_bottom(&mut self) {
        if let Ok(mut parser) = self.terminal_parser.lock() {
            parser.set_scrollback(0);
            self.scroll_offset = 0;
        }
    }

    /// Enter interactive mode (deprecated, use enter_terminal_normal)
    pub async fn enter_interactive(&mut self) -> Result<()> {
        self.enter_terminal_normal().await
    }

    /// Exit interactive mode
    pub fn exit_interactive(&mut self) {
        self.is_interactive = false;
        self.focus = Focus::Sessions;
    }

    /// Create new session and enter interactive mode
    pub async fn create_new(&mut self) -> Result<()> {
        match self.focus {
            Focus::Branches => {
                // Enter input mode for new branch name
                self.input_mode = InputMode::NewBranch;
                self.input_buffer.clear();
                self.status_message = Some("Enter branch name:".to_string());
            }
            Focus::Sessions => {
                // Create new session for current branch
                if let (Some(repo), Some(branch)) = (
                    self.repos.get(self.repo_idx).cloned(),
                    self.branches.get(self.branch_idx).cloned(),
                ) {
                    match self.client.create_session(&repo.id, &branch.branch, None).await {
                        Ok(session) => {
                            self.refresh_sessions().await?;
                            // Find and select the new session
                            if let Some(idx) = self.sessions.iter().position(|s| s.id == session.id) {
                                self.session_idx = idx;
                                self.update_active_session().await;
                            }
                            self.enter_terminal_normal().await?;
                        }
                        Err(e) => {
                            self.error_message = Some(e.to_string());
                        }
                    }
                }
            }
            Focus::Terminal => {}
        }
        Ok(())
    }

    /// Submit the input buffer (for new branch creation)
    pub async fn submit_input(&mut self) -> Result<()> {
        if self.input_mode != InputMode::NewBranch {
            return Ok(());
        }

        let branch_name = self.input_buffer.trim().to_string();
        self.input_mode = InputMode::Normal;
        self.input_buffer.clear();
        self.status_message = None;

        if branch_name.is_empty() {
            self.error_message = Some("Branch name cannot be empty".to_string());
            return Ok(());
        }

        // Create session (will auto-create worktree if needed)
        if let Some(repo) = self.repos.get(self.repo_idx).cloned() {
            match self.client.create_session(&repo.id, &branch_name, None).await {
                Ok(session) => {
                    self.refresh_branches().await?;
                    // Find the branch and session
                    if let Some(b_idx) = self.branches.iter().position(|b| b.branch == branch_name) {
                        self.branch_idx = b_idx;
                        self.refresh_sessions().await?;
                        if let Some(s_idx) = self.sessions.iter().position(|s| s.id == session.id) {
                            self.session_idx = s_idx;
                            self.update_active_session().await;
                        }
                    }
                    self.focus = Focus::Sessions;
                    self.enter_terminal_normal().await?;
                }
                Err(e) => {
                    self.error_message = Some(e.to_string());
                }
            }
        }
        Ok(())
    }

    /// Cancel input mode
    pub fn cancel_input(&mut self) {
        self.input_mode = InputMode::Normal;
        self.input_buffer.clear();
        self.status_message = None;
    }

    /// Delete selected item
    pub async fn delete(&mut self) -> Result<()> {
        match self.focus {
            Focus::Branches => {
                if let (Some(repo), Some(wt)) = (
                    self.repos.get(self.repo_idx).cloned(),
                    self.branches.get(self.branch_idx).cloned(),
                ) {
                    if !wt.is_main && !wt.path.is_empty() {
                        match self.client.remove_worktree(&repo.id, &wt.branch).await {
                            Ok(_) => {
                                self.status_message = Some(format!("Removed worktree: {}", wt.branch));
                                self.refresh_branches().await?;
                            }
                            Err(e) => {
                                self.error_message = Some(e.to_string());
                            }
                        }
                    } else {
                        self.error_message = Some("Cannot remove main worktree".to_string());
                    }
                }
            }
            Focus::Sessions => {
                if let Some(session) = self.sessions.get(self.session_idx).cloned() {
                    // Disconnect if this is the active session
                    if self.active_session_id.as_ref() == Some(&session.id) {
                        self.disconnect_stream();
                    }

                    match self.client.destroy_session(&session.id).await {
                        Ok(_) => {
                            self.status_message = Some(format!("Destroyed session: {}", session.name));
                            self.refresh_sessions().await?;
                        }
                        Err(e) => {
                            self.error_message = Some(e.to_string());
                        }
                    }
                }
            }
            Focus::Terminal => {}
        }
        Ok(())
    }

    /// Connect to session stream for preview/interaction
    pub async fn connect_stream(&mut self) -> Result<()> {
        let session_id = match &self.active_session_id {
            Some(id) => id.clone(),
            None => return Ok(()),
        };

        // Get terminal size and calculate inner area
        // Layout: Tab bar (3) + Main content + Status bar (3)
        // Main content: Sidebar (25%) + Terminal (75%)
        // Terminal has borders (2 lines, 2 cols)
        let (full_cols, full_rows) = size()?;
        let main_height = full_rows.saturating_sub(6); // tab + status bars
        let terminal_width = (full_cols as f32 * 0.75) as u16;
        let inner_rows = main_height.saturating_sub(2); // borders
        let inner_cols = terminal_width.saturating_sub(2); // borders

        // Resize vt100 parser to match
        if let Ok(mut parser) = self.terminal_parser.lock() {
            parser.set_size(inner_rows, inner_cols);
        }

        // Create input channel
        let (input_tx, input_rx) = mpsc::channel::<AttachInput>(32);

        // Send initial message with session ID and size
        input_tx
            .send(AttachInput {
                session_id: session_id.clone(),
                data: vec![],
                rows: Some(inner_rows as u32),
                cols: Some(inner_cols as u32),
            })
            .await?;

        // Start attach stream
        let response = self
            .client
            .inner_mut()
            .attach_session(ReceiverStream::new(input_rx))
            .await?;

        let mut output_stream = response.into_inner();

        // Create output channel
        let (output_tx, output_rx) = mpsc::channel::<Vec<u8>>(64);

        // Spawn task to read from output stream
        tokio::spawn(async move {
            loop {
                match output_stream.message().await {
                    Ok(Some(msg)) => {
                        if output_tx.send(msg.data).await.is_err() {
                            break;
                        }
                    }
                    Ok(None) | Err(_) => break,
                }
            }
        });

        self.terminal_stream = Some(TerminalStream {
            session_id,
            input_tx,
            output_rx,
        });

        Ok(())
    }

    /// Disconnect from session stream
    pub fn disconnect_stream(&mut self) {
        self.terminal_stream = None;
    }

    /// Poll terminal output (non-blocking)
    pub fn poll_terminal_output(&mut self) {
        if let Some(stream) = &mut self.terminal_stream {
            // Try to receive without blocking
            while let Ok(data) = stream.output_rx.try_recv() {
                if let Ok(mut parser) = self.terminal_parser.lock() {
                    parser.process(&data);
                }
            }
        }
    }

    /// Send data to terminal
    pub async fn send_to_terminal(&mut self, data: Vec<u8>) -> Result<()> {
        if let Some(stream) = &self.terminal_stream {
            stream
                .input_tx
                .send(AttachInput {
                    session_id: stream.session_id.clone(),
                    data,
                    rows: None,
                    cols: None,
                })
                .await?;
        }
        Ok(())
    }

    /// Send resize to terminal
    pub async fn resize_terminal(&mut self, rows: u16, cols: u16) -> Result<()> {
        // Calculate inner area (same as connect_stream)
        let main_height = rows.saturating_sub(6);
        let terminal_width = (cols as f32 * 0.75) as u16;
        let inner_rows = main_height.saturating_sub(2);
        let inner_cols = terminal_width.saturating_sub(2);

        // Resize parser
        if let Ok(mut parser) = self.terminal_parser.lock() {
            parser.set_size(inner_rows, inner_cols);
        }

        if let Some(stream) = &self.terminal_stream {
            stream
                .input_tx
                .send(AttachInput {
                    session_id: stream.session_id.clone(),
                    data: vec![],
                    rows: Some(inner_rows as u32),
                    cols: Some(inner_cols as u32),
                })
                .await?;
        }
        Ok(())
    }

    /// Get terminal lines for rendering
    pub fn get_terminal_lines(&self, height: u16, width: u16) -> Vec<ratatui::text::Line<'static>> {
        let mut lines = Vec::new();

        if let Ok(parser) = self.terminal_parser.lock() {
            let screen = parser.screen();

            for row in 0..height {
                let mut spans: Vec<ratatui::text::Span<'static>> = Vec::new();
                let mut col = 0u16;

                while col < width {
                    if let Some(cell) = screen.cell(row, col) {
                        let ch = cell.contents();
                        if ch.is_empty() {
                            spans.push(ratatui::text::Span::raw(" "));
                            col += 1;
                        } else {
                            // Build style from cell attributes
                            let mut style = ratatui::style::Style::default();

                            // Apply foreground color
                            let fg = cell.fgcolor();
                            if fg != vt100::Color::Default {
                                style = style.fg(vt100_color_to_ratatui(fg));
                            }

                            // Apply background color
                            let bg = cell.bgcolor();
                            if bg != vt100::Color::Default {
                                style = style.bg(vt100_color_to_ratatui(bg));
                            }

                            // Apply attributes
                            if cell.bold() {
                                style = style.add_modifier(ratatui::style::Modifier::BOLD);
                            }
                            if cell.italic() {
                                style = style.add_modifier(ratatui::style::Modifier::ITALIC);
                            }
                            if cell.underline() {
                                style = style.add_modifier(ratatui::style::Modifier::UNDERLINED);
                            }
                            if cell.inverse() {
                                style = style.add_modifier(ratatui::style::Modifier::REVERSED);
                            }

                            spans.push(ratatui::text::Span::styled(ch.clone(), style));

                            // Skip columns for wide characters
                            use unicode_width::UnicodeWidthStr;
                            let w = ch.width();
                            col += w.max(1) as u16;
                        }
                    } else {
                        spans.push(ratatui::text::Span::raw(" "));
                        col += 1;
                    }
                }

                lines.push(ratatui::text::Line::from(spans));
            }
        }

        lines
    }
}

/// Convert vt100 color to ratatui color
fn vt100_color_to_ratatui(color: vt100::Color) -> ratatui::style::Color {
    match color {
        vt100::Color::Default => ratatui::style::Color::Reset,
        vt100::Color::Idx(idx) => ratatui::style::Color::Indexed(idx),
        vt100::Color::Rgb(r, g, b) => ratatui::style::Color::Rgb(r, g, b),
    }
}

/// Result of TUI run
pub enum RunResult {
    /// User quit (q)
    Quit,
}

/// Run the TUI application
pub async fn run_with_client(mut app: App) -> Result<RunResult> {
    // Deactivate IME at startup
    deactivate_ime();

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Main loop
    loop {
        // Poll terminal output
        app.poll_terminal_output();

        // Draw UI
        terminal.draw(|f| draw(f, &app))?;

        // Handle events with short timeout for responsive terminal preview
        if event::poll(std::time::Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                handle_input(&mut app, key).await?;
            } else if let Event::Resize(cols, rows) = event::read()? {
                let _ = app.resize_terminal(rows, cols).await;
            }
        }

        // Check if should quit
        if app.should_quit {
            break;
        }
    }

    // Cleanup
    app.disconnect_stream();

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    // Activate IME at exit
    activate_ime();

    Ok(RunResult::Quit)
}
