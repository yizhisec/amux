//! Session management

use crate::pty::PtyProcess;
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
    ) -> Self {
        Self {
            id,
            name,
            repo_id,
            branch,
            worktree_path,
            pty: None,
            screen_buffer: Arc::new(Mutex::new(vt100::Parser::new(24, 80, 10000))),
            raw_output_buffer: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Start the session (spawn PTY)
    pub fn start(&mut self) -> Result<()> {
        if self.pty.is_some() {
            return Ok(()); // Already running
        }

        let pty = PtyProcess::spawn(&self.worktree_path)?;
        self.pty = Some(pty);
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
