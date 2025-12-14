//! Session management

use crate::persistence::{self, SessionMeta};
use crate::providers::{AiProvider, ClaudeProvider, ProviderConfig, ProviderRegistry, SessionMode};
use crate::pty::PtyProcess;
use amux_config::{DEFAULT_SCROLLBACK, DEFAULT_TERMINAL_COLS, DEFAULT_TERMINAL_ROWS};
use anyhow::Result;
use serde::{Deserialize, Serialize};
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

/// Session kind - distinguishes interactive, one-shot, and shell sessions
///
/// This enum replaces the previous combination of:
/// - `provider_session_id: Option<String>`
/// - `provider_session_started: bool`
/// - `is_shell: bool`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionKind {
    /// Interactive AI session that can be resumed
    Interactive {
        /// Provider-specific session ID for resume functionality
        provider_session_id: String,
        /// Whether this session has been started before (for resume vs new)
        started: bool,
    },
    /// One-shot session with a prompt (not persisted, not resumable)
    OneShot,
    /// Plain shell session (no AI provider)
    Shell,
}

impl SessionKind {
    /// Check if this session should be persisted
    pub fn should_persist(&self) -> bool {
        matches!(self, SessionKind::Interactive { .. } | SessionKind::Shell)
    }

    /// Check if this is a shell session
    pub fn is_shell(&self) -> bool {
        matches!(self, SessionKind::Shell)
    }

    /// Get provider session ID if interactive
    pub fn provider_session_id(&self) -> Option<&str> {
        match self {
            SessionKind::Interactive { provider_session_id, .. } => Some(provider_session_id),
            _ => None,
        }
    }

    /// Mark as started (for interactive sessions)
    pub fn mark_started(&mut self) {
        if let SessionKind::Interactive { started, .. } = self {
            *started = true;
        }
    }
}

/// An AI coding session
pub struct Session {
    pub id: String,
    pub name: String,
    pub repo_id: String,
    pub branch: String,
    pub worktree_path: PathBuf,
    pub provider: String,                 // AI provider name (e.g., "claude", "codex")
    pub kind: SessionKind,                // Session type (Interactive/OneShot/Shell)
    pub name_updated_from_provider: bool, // Whether name was updated from provider's first message
    pub model: Option<String>,            // Model to use (e.g., "haiku", "sonnet")
    pub prompt: Option<String>,           // Initial prompt (only used on first start)
    pub pty: Option<PtyProcess>,
    pub screen_buffer: Arc<Mutex<vt100::Parser>>,
    pub raw_output_buffer: Arc<Mutex<Vec<u8>>>,
}

impl Session {
    // ============ Compatibility Methods ============
    // These methods provide backward compatibility for code using old field names

    /// Check if this is a shell session
    pub fn is_shell(&self) -> bool {
        self.kind.is_shell()
    }

    /// Get provider session ID if this is an interactive session
    pub fn provider_session_id(&self) -> Option<&str> {
        self.kind.provider_session_id()
    }
}

impl Session {
    /// Create a new session with explicit SessionKind
    pub fn with_kind(
        id: String,
        name: String,
        repo_id: String,
        branch: String,
        worktree_path: PathBuf,
        provider: String,
        kind: SessionKind,
        model: Option<String>,
        prompt: Option<String>,
    ) -> Self {
        Self {
            id,
            name,
            repo_id,
            branch,
            worktree_path,
            provider,
            kind,
            name_updated_from_provider: false,
            model,
            prompt,
            pty: None,
            screen_buffer: Arc::new(Mutex::new(vt100::Parser::new(
                DEFAULT_TERMINAL_ROWS,
                DEFAULT_TERMINAL_COLS,
                DEFAULT_SCROLLBACK,
            ))),
            raw_output_buffer: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Create a new session (backward compatible API)
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        name: String,
        repo_id: String,
        branch: String,
        worktree_path: PathBuf,
        provider: String,
        provider_session_id: Option<String>,
        is_shell: bool,
        model: Option<String>,
        prompt: Option<String>,
    ) -> Self {
        // Convert old API to SessionKind
        let kind = if is_shell {
            SessionKind::Shell
        } else if let Some(session_id) = provider_session_id {
            SessionKind::Interactive {
                provider_session_id: session_id,
                started: false,
            }
        } else {
            // No provider_session_id and not shell - treat as one-shot
            SessionKind::OneShot
        };

        Self::with_kind(
            id,
            name,
            repo_id,
            branch,
            worktree_path,
            provider,
            kind,
            model,
            prompt,
        )
    }

    /// Restore a session from persisted metadata
    pub fn from_meta(meta: SessionMeta) -> Self {
        // Convert persisted metadata to SessionKind
        let kind = if let Some(kind) = meta.kind {
            kind
        } else {
            // Backward compatibility: convert old format
            if meta.is_shell {
                SessionKind::Shell
            } else if let Some(session_id) = meta.provider_session_id {
                SessionKind::Interactive {
                    provider_session_id: session_id,
                    started: true, // Restored sessions were already started
                }
            } else {
                SessionKind::OneShot
            }
        };

        Self {
            id: meta.id,
            name: meta.name,
            repo_id: meta.repo_id,
            branch: meta.branch,
            worktree_path: meta.worktree_path,
            provider: meta.provider,
            kind,
            name_updated_from_provider: meta.name_updated_from_provider,
            model: meta.model,
            prompt: None, // Prompt is only used on first start, not restored
            pty: None,    // PTY will be started on demand
            screen_buffer: Arc::new(Mutex::new(vt100::Parser::new(
                DEFAULT_TERMINAL_ROWS,
                DEFAULT_TERMINAL_COLS,
                DEFAULT_SCROLLBACK,
            ))),
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
        Ok(persistence::save_session(self)?)
    }

    /// Start the session (spawn PTY) with default size
    pub fn start(&mut self, registry: &ProviderRegistry) -> Result<()> {
        self.start_with_size(registry, DEFAULT_TERMINAL_ROWS, DEFAULT_TERMINAL_COLS)
    }

    /// Start the session (spawn PTY) with specific terminal size
    pub fn start_with_size(
        &mut self,
        registry: &ProviderRegistry,
        rows: u16,
        cols: u16,
    ) -> Result<()> {
        if self.pty.is_some() {
            return Ok(()); // Already running
        }

        // Determine session mode and spawn PTY based on SessionKind
        let pty = match &self.kind {
            SessionKind::Shell => {
                // Shell session - run plain shell (no provider)
                PtyProcess::spawn_shell(&self.worktree_path, rows, cols)?
            }
            SessionKind::OneShot => {
                // One-shot session with prompt
                let prompt = self.prompt.take();
                let config = ProviderConfig {
                    session_mode: SessionMode::OneShot,
                    model: self.model.clone(),
                    prompt,
                };

                // Get provider and build command
                let provider = registry
                    .get(&self.provider)
                    .ok_or_else(|| anyhow::anyhow!("Provider '{}' not found", self.provider))?;
                let (cmd, args) = provider.build_command(&config)?;

                tracing::info!(
                    "Spawning PTY with provider '{}': cmd={:?}, size={}x{}",
                    self.provider,
                    cmd,
                    rows,
                    cols
                );

                PtyProcess::spawn(&self.worktree_path, cmd, args, rows, cols)?
            }
            SessionKind::Interactive { provider_session_id, started } => {
                // Take prompt if set (only used on first start)
                let prompt = self.prompt.take();

                // Determine session mode
                let session_mode = if prompt.is_some() {
                    // New session with prompt
                    SessionMode::New {
                        session_id: Some(provider_session_id.clone()),
                    }
                } else if *started {
                    // Resume existing session
                    SessionMode::Resume {
                        session_id: provider_session_id.clone(),
                    }
                } else {
                    // New session without prompt
                    SessionMode::New {
                        session_id: Some(provider_session_id.clone()),
                    }
                };

                let config = ProviderConfig {
                    session_mode,
                    model: self.model.clone(),
                    prompt,
                };

                // Get provider and build command
                let provider = registry
                    .get(&self.provider)
                    .ok_or_else(|| anyhow::anyhow!("Provider '{}' not found", self.provider))?;
                let (cmd, args) = provider.build_command(&config)?;

                tracing::info!(
                    "Spawning PTY with provider '{}': cmd={:?}, size={}x{}",
                    self.provider,
                    cmd,
                    rows,
                    cols
                );

                PtyProcess::spawn(&self.worktree_path, cmd, args, rows, cols)?
            }
        };

        self.pty = Some(pty);

        // Mark interactive session as started for next time
        self.kind.mark_started();

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
            Some(pty) => Ok(pty.read(buf)?),
            None => Ok(0),
        }
    }

    /// Write to PTY
    pub fn write(&self, data: &[u8]) -> Result<usize> {
        match &self.pty {
            Some(pty) => Ok(pty.write(data)?),
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

    /// Update session name from provider's first user message
    pub fn update_name_from_provider(&mut self) {
        if self.name_updated_from_provider {
            return; // Already updated
        }
        if let Some(session_id) = self.kind.provider_session_id() {
            // Currently only Claude provider supports reading session info
            // TODO: Use ProviderRegistry to get appropriate provider
            if self.provider == "claude" {
                let claude_provider = ClaudeProvider::new();
                if let Ok(Some(info)) = claude_provider.read_session_info(session_id, &self.worktree_path) {
                    if let Some(description) = info.description {
                        self.name = description;
                        self.name_updated_from_provider = true;
                    }
                }
            }
        }
    }
}

/// Generate a unique session ID
pub fn generate_session_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Generate a session name based on provider and existing sessions
/// Format: provider-N (e.g., claude-1, codex-2)
pub fn generate_session_name(provider: &str, existing_names: &[String]) -> String {
    // Find next available number for this provider
    let mut n = 1;
    loop {
        let name = format!("{}-{}", provider, n);
        if !existing_names.contains(&name) {
            return name;
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_session_id_is_unique() {
        let id1 = generate_session_id();
        let id2 = generate_session_id();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_generate_session_id_is_valid_uuid() {
        let id = generate_session_id();
        assert!(uuid::Uuid::parse_str(&id).is_ok());
    }

    #[test]
    fn test_generate_session_name_first_session() {
        let existing: Vec<String> = vec![];
        let name = generate_session_name("claude", &existing);
        assert_eq!(name, "claude-1");
    }

    #[test]
    fn test_generate_session_name_second_session() {
        let existing = vec!["claude-1".to_string()];
        let name = generate_session_name("claude", &existing);
        assert_eq!(name, "claude-2");
    }

    #[test]
    fn test_generate_session_name_third_session() {
        let existing = vec!["claude-1".to_string(), "claude-2".to_string()];
        let name = generate_session_name("claude", &existing);
        assert_eq!(name, "claude-3");
    }

    #[test]
    fn test_generate_session_name_with_gap() {
        // If claude-2 is missing, should still use claude-2
        let existing = vec!["claude-1".to_string(), "claude-3".to_string()];
        let name = generate_session_name("claude", &existing);
        assert_eq!(name, "claude-2");
    }

    #[test]
    fn test_generate_session_name_different_provider() {
        let existing = vec!["claude-1".to_string()];
        let name = generate_session_name("codex", &existing);
        assert_eq!(name, "codex-1");
    }
}
