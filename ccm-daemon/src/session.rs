//! Session management

use crate::persistence::{self, SessionMeta};
use crate::pty::{ClaudeSessionMode, PtyProcess};
use anyhow::Result;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Maximum raw buffer size (1MB)
const MAX_RAW_BUFFER_SIZE: usize = 1024 * 1024;

/// Session status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    Running,
    Stopped,
}

/// A Claude Code session
pub struct Session {
    pub id: String,
    pub name: String,
    pub repo_id: String,
    pub branch: String,
    pub worktree_path: PathBuf,
    pub claude_session_id: Option<String>,  // Associated Claude Code session ID
    pub claude_session_started: bool,       // Whether Claude session has been started before
    pub pty: Option<PtyProcess>,
    pub screen_buffer: Arc<Mutex<vt100::Parser>>,
    pub raw_output_buffer: Arc<Mutex<Vec<u8>>>,
}

impl Session {
    /// Create a new session
    pub fn new(
        id: String,
        name: String,
        repo_id: String,
        branch: String,
        worktree_path: PathBuf,
        claude_session_id: Option<String>,
    ) -> Self {
        Self {
            id,
            name,
            repo_id,
            branch,
            worktree_path,
            claude_session_id,
            claude_session_started: false,  // New session, not started yet
            pty: None,
            screen_buffer: Arc::new(Mutex::new(vt100::Parser::new(24, 80, 10000))),
            raw_output_buffer: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Restore a session from persisted metadata
    pub fn from_meta(meta: SessionMeta) -> Self {
        // If session has claude_session_id, assume it was started before (restored session)
        let claude_session_started = meta.claude_session_id.is_some();
        Self {
            id: meta.id,
            name: meta.name,
            repo_id: meta.repo_id,
            branch: meta.branch,
            worktree_path: meta.worktree_path,
            claude_session_id: meta.claude_session_id,
            claude_session_started,  // Restored session was likely started before
            pty: None, // PTY will be started on demand
            screen_buffer: Arc::new(Mutex::new(vt100::Parser::new(24, 80, 10000))),
            raw_output_buffer: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Load terminal history from disk and restore buffer
    pub fn load_history(&self) -> Result<()> {
        let history = persistence::load_session_history(&self.id)?;
        if !history.is_empty() {
            // Restore raw buffer
            if let Ok(mut buffer) = self.raw_output_buffer.lock() {
                *buffer = history.clone();
            }
            // Replay through VT100 parser
            if let Ok(mut parser) = self.screen_buffer.lock() {
                parser.process(&history);
            }
        }
        Ok(())
    }

    /// Save session to disk (metadata + history)
    #[allow(dead_code)]
    pub fn save(&self) -> Result<()> {
        persistence::save_session(self)
    }

    /// Start the session (spawn PTY)
    pub fn start(&mut self) -> Result<()> {
        if self.pty.is_some() {
            return Ok(()); // Already running
        }

        // Determine session mode based on claude_session_id and started flag
        let session_mode = match &self.claude_session_id {
            Some(id) if self.claude_session_started => ClaudeSessionMode::Resume(id.clone()),
            Some(id) => ClaudeSessionMode::New(id.clone()),
            None => ClaudeSessionMode::None,
        };

        let pty = PtyProcess::spawn_with_session(&self.worktree_path, session_mode)?;
        self.pty = Some(pty);

        // Mark as started for next time
        self.claude_session_started = true;

        Ok(())
    }

    /// Stop the session
    pub fn stop(&mut self) -> Result<()> {
        if let Some(pty) = self.pty.take() {
            pty.kill()?;
        }
        Ok(())
    }

    /// Get session status
    pub fn status(&self) -> SessionStatus {
        match &self.pty {
            Some(pty) if pty.is_running() => SessionStatus::Running,
            _ => SessionStatus::Stopped,
        }
    }

    /// Read from PTY
    pub fn read(&self, buf: &mut [u8]) -> Result<usize> {
        match &self.pty {
            Some(pty) => pty.read(buf),
            None => Ok(0),
        }
    }

    /// Write to PTY
    pub fn write(&self, data: &[u8]) -> Result<usize> {
        match &self.pty {
            Some(pty) => pty.write(data),
            None => Ok(0),
        }
    }

    /// Resize PTY
    pub fn resize(&self, rows: u16, cols: u16) -> Result<()> {
        if let Some(pty) = &self.pty {
            pty.resize(rows, cols)?;
        }
        // Also resize screen buffer
        if let Ok(mut parser) = self.screen_buffer.lock() {
            parser.set_size(rows, cols);
        }
        Ok(())
    }

    /// Get PTY master fd for polling
    #[allow(dead_code)]
    pub fn master_fd(&self) -> Option<i32> {
        self.pty.as_ref().map(|p| p.master_fd())
    }

    /// Process output data (store in buffers)
    pub fn process_output(&self, data: &[u8]) {
        // Update screen buffer
        if let Ok(mut parser) = self.screen_buffer.lock() {
            parser.process(data);
        }

        // Store raw output for history replay
        if let Ok(mut buffer) = self.raw_output_buffer.lock() {
            buffer.extend_from_slice(data);
            // Trim if too large
            if buffer.len() > MAX_RAW_BUFFER_SIZE {
                let excess = buffer.len() - MAX_RAW_BUFFER_SIZE;
                buffer.drain(..excess);
            }
        }
    }

    /// Get screen state (raw buffer for replay)
    pub fn get_screen_state(&self) -> Vec<u8> {
        if let Ok(buffer) = self.raw_output_buffer.lock() {
            buffer.clone()
        } else {
            Vec::new()
        }
    }
}

/// Generate a unique session ID
pub fn generate_session_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Generate a session name based on branch and existing sessions
pub fn generate_session_name(branch: &str, existing_names: &[String]) -> String {
    // First session: just branch name
    if !existing_names.contains(&branch.to_string()) {
        return branch.to_string();
    }

    // Find next available number
    let mut n = 2;
    loop {
        let name = format!("{}-{}", branch, n);
        if !existing_names.contains(&name) {
            return name;
        }
        n += 1;
    }
}
