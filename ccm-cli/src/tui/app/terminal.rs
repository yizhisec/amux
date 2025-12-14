//! Terminal operations and stream management

use super::super::state::{Focus, TerminalMode};
use super::super::App;
use crate::error::TuiError;
use ccm_proto::daemon::AttachInput;
use crossterm::terminal::size;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

type Result<T> = std::result::Result<T, TuiError>;

/// Terminal stream state for a session
pub struct TerminalStream {
    pub session_id: String,
    pub input_tx: mpsc::Sender<AttachInput>,
    pub output_rx: mpsc::Receiver<Vec<u8>>,
}

impl App {
    /// Enter terminal Insert mode (from Sessions)
    pub async fn enter_terminal(&mut self) -> Result<()> {
        if self.terminal.active_session_id.is_some() {
            self.focus = Focus::Terminal;
            self.terminal.mode = TerminalMode::Insert;
            self.scroll_to_bottom();

            // Ensure stream is connected
            if self.terminal_stream.is_none() {
                self.connect_stream().await?;
            }
        }
        Ok(())
    }

    /// Enter Insert mode (from Normal mode)
    pub fn enter_insert_mode(&mut self) {
        self.terminal.mode = TerminalMode::Insert;
        self.scroll_to_bottom();
    }

    /// Exit terminal mode (back to sidebar)
    pub fn exit_terminal(&mut self) {
        if self.terminal.fullscreen {
            self.terminal.fullscreen = false;
        } else {
            self.focus = Focus::Sidebar;
            self.terminal.mode = TerminalMode::Normal;
            self.terminal.is_interactive = false;
        }
    }

    /// Toggle fullscreen mode
    pub fn toggle_fullscreen(&mut self) {
        self.terminal.fullscreen = !self.terminal.fullscreen;
    }

    /// Scroll up (older content)
    pub fn scroll_up(&mut self, lines: usize) {
        if let Ok(mut parser) = self.terminal.parser.lock() {
            let current = parser.screen().scrollback();
            let new_offset = current + lines;
            parser.screen_mut().set_scrollback(new_offset);
            self.terminal.scroll_offset = parser.screen().scrollback();
        }
    }

    /// Scroll down (newer content)
    pub fn scroll_down(&mut self, lines: usize) {
        if let Ok(mut parser) = self.terminal.parser.lock() {
            let current = parser.screen().scrollback();
            let new_offset = current.saturating_sub(lines);
            parser.screen_mut().set_scrollback(new_offset);
            self.terminal.scroll_offset = parser.screen().scrollback();
        }
    }

    /// Scroll to top
    pub fn scroll_to_top(&mut self) {
        if let Ok(mut parser) = self.terminal.parser.lock() {
            parser.screen_mut().set_scrollback(usize::MAX);
            self.terminal.scroll_offset = parser.screen().scrollback();
        }
    }

    /// Scroll to bottom
    pub fn scroll_to_bottom(&mut self) {
        if let Ok(mut parser) = self.terminal.parser.lock() {
            parser.screen_mut().set_scrollback(0);
            self.terminal.scroll_offset = 0;
        }
    }

    /// Connect to session stream for preview/interaction
    pub async fn connect_stream(&mut self) -> Result<()> {
        let session_id = match &self.terminal.active_session_id {
            Some(id) => id.clone(),
            None => return Ok(()),
        };

        // Get terminal size and calculate inner area
        // Layout: Tab bar (3) + Main content + Status bar (3)
        // Main content: Sidebar (25%) + Terminal (75%)
        // Terminal has borders (2 lines, 2 cols)
        let (full_cols, full_rows) = size().map_err(TuiError::TerminalInit)?;
        let main_height = full_rows.saturating_sub(6); // tab + status bars
        let terminal_width = (full_cols as f32 * 0.75) as u16;
        let inner_rows = main_height.saturating_sub(2); // borders
        let inner_cols = terminal_width.saturating_sub(2); // borders

        // Resize vt100 parser to match
        if let Ok(mut parser) = self.terminal.parser.lock() {
            parser.screen_mut().set_size(inner_rows, inner_cols);
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
            .await
            .map_err(|_| TuiError::ChannelSend)?;

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
            while let Ok(Some(msg)) = output_stream.message().await {
                if output_tx.send(msg.data).await.is_err() {
                    break;
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
                .await
                .map_err(|_| TuiError::ChannelSend)?;
        }
        Ok(())
    }

    /// Send resize to terminal
    pub async fn resize_terminal(&mut self, rows: u16, cols: u16) -> Result<()> {
        // Store terminal size for mouse position calculations
        self.terminal.cols = Some(cols);
        self.terminal.rows = Some(rows);

        // Calculate inner area (same as connect_stream)
        let main_height = rows.saturating_sub(6);
        let terminal_width = (cols as f32 * 0.75) as u16;
        let inner_rows = main_height.saturating_sub(2);
        let inner_cols = terminal_width.saturating_sub(2);

        // Resize parser
        if let Ok(mut parser) = self.terminal.parser.lock() {
            parser.screen_mut().set_size(inner_rows, inner_cols);
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
                .await
                .map_err(|_| TuiError::ChannelSend)?;
        }
        Ok(())
    }

    /// Toggle between current session and shell session (Ctrl+`)
    pub async fn switch_to_shell_session(&mut self) -> Result<()> {
        const SHELL_SESSION_NAME: &str = "__shell__";

        // Get current worktree
        let current_worktree = match self.current_worktree() {
            Some(wt) => wt.clone(),
            None => {
                self.status_message = Some("No worktree selected".to_string());
                return Ok(());
            }
        };

        // Find shell session in current worktree
        let shell_session = self
            .sessions()
            .iter()
            .find(|s| {
                s.name == SHELL_SESSION_NAME
                    && s.branch == current_worktree.branch
                    && s.is_shell == Some(true)
            })
            .cloned();

        // Check if currently in shell session
        let currently_in_shell = self
            .terminal
            .active_session_id
            .as_ref()
            .and_then(|id| shell_session.as_ref().map(|s| &s.id == id))
            .unwrap_or(false);

        if currently_in_shell {
            // Currently in shell, toggle back to previous session
            if let Some(previous_session_id) = &self.terminal.session_before_shell {
                // Verify previous session still exists
                let previous_session_exists =
                    self.sessions().iter().any(|s| &s.id == previous_session_id);

                if previous_session_exists {
                    let target_id = previous_session_id.clone();

                    // Disconnect current stream
                    self.disconnect_stream();

                    // Save current parser
                    if let Some(old_id) = &self.terminal.active_session_id {
                        self.terminal
                            .session_parsers
                            .insert(old_id.clone(), self.terminal.parser.clone());
                    }

                    // Restore or create parser for target session
                    self.terminal.parser = self
                        .terminal
                        .session_parsers
                        .entry(target_id.clone())
                        .or_insert_with(|| Arc::new(Mutex::new(vt100::Parser::new(24, 80, 10000))))
                        .clone();

                    self.terminal.scroll_offset = 0;
                    self.terminal.active_session_id = Some(target_id);

                    // Clear session_before_shell (we've returned to it)
                    self.terminal.session_before_shell = None;

                    self.enter_terminal().await?;
                    self.status_message = Some("Switched back from shell".to_string());
                } else {
                    // Previous session no longer exists
                    self.terminal.session_before_shell = None;
                    self.status_message = Some("Previous session no longer exists".to_string());
                }
            } else {
                // Already in shell but no previous session saved
                self.status_message = Some("Already in shell session".to_string());
            }
        } else {
            // Not in shell, toggle to shell

            // Save current session ID (if any)
            if let Some(current_id) = &self.terminal.active_session_id {
                self.terminal.session_before_shell = Some(current_id.clone());
            }

            if let Some(session) = shell_session {
                // Shell session exists, switch to it
                let new_id = session.id.clone();

                // Disconnect current stream
                self.disconnect_stream();

                // Save current parser
                if let Some(old_id) = &self.terminal.active_session_id {
                    self.terminal
                        .session_parsers
                        .insert(old_id.clone(), self.terminal.parser.clone());
                }

                // Get or create parser for shell session
                self.terminal.parser = self
                    .terminal
                    .session_parsers
                    .entry(new_id.clone())
                    .or_insert_with(|| Arc::new(Mutex::new(vt100::Parser::new(24, 80, 10000))))
                    .clone();

                self.terminal.scroll_offset = 0;
                self.terminal.active_session_id = Some(new_id);

                self.enter_terminal().await?;
                self.status_message = Some("Switched to shell session".to_string());
            } else {
                // Create new shell session
                let repo = match self.current_repo() {
                    Some(r) => r.info.clone(),
                    None => {
                        self.status_message = Some("No repository selected".to_string());
                        return Ok(());
                    }
                };

                match self
                    .client
                    .create_session(
                        &repo.id,
                        &current_worktree.branch,
                        Some(SHELL_SESSION_NAME),
                        Some(true),
                    )
                    .await
                {
                    Ok(session) => {
                        // Refresh sessions list
                        self.refresh_sessions().await?;
                        self.load_worktree_sessions(self.branch_idx()).await?;

                        let new_id = session.id;

                        // Disconnect current stream
                        self.disconnect_stream();

                        // Save current parser
                        if let Some(old_id) = &self.terminal.active_session_id {
                            self.terminal
                                .session_parsers
                                .insert(old_id.clone(), self.terminal.parser.clone());
                        }

                        // Create parser for new shell session
                        self.terminal.parser =
                            Arc::new(Mutex::new(vt100::Parser::new(24, 80, 10000)));
                        self.terminal
                            .session_parsers
                            .insert(new_id.clone(), self.terminal.parser.clone());

                        self.terminal.scroll_offset = 0;
                        self.terminal.active_session_id = Some(new_id);

                        self.enter_terminal().await?;
                        self.status_message = Some("Created shell session".to_string());
                    }
                    Err(e) => {
                        self.error_message = Some(format!("Failed to create shell session: {}", e));
                    }
                }
            }
        }

        Ok(())
    }
}
