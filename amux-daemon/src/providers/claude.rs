//! Claude Code provider implementation

use super::{
    AiProvider, ProviderConfig, ProviderError, ProviderResult, ProviderSessionInfo, SessionMode,
};
use serde::Deserialize;
use std::ffi::CString;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// Claude Code CLI provider
pub struct ClaudeProvider {
    /// Path to claude CLI (defaults to "claude" in PATH)
    command_path: String,
}

impl Default for ClaudeProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ClaudeProvider {
    /// Create a new ClaudeProvider with default settings
    pub fn new() -> Self {
        Self {
            command_path: "claude".to_string(),
        }
    }

    /// Create a ClaudeProvider with custom command path
    pub fn with_command_path(path: impl Into<String>) -> Self {
        Self {
            command_path: path.into(),
        }
    }
}

impl AiProvider for ClaudeProvider {
    fn name(&self) -> &str {
        "claude"
    }

    fn display_name(&self) -> &str {
        "Claude"
    }

    fn build_command(&self, config: &ProviderConfig) -> ProviderResult<(CString, Vec<CString>)> {
        let cmd = CString::new(self.command_path.clone())
            .map_err(|e| ProviderError::CommandBuild(e.to_string()))?;

        let mut args = vec![cmd.clone()];

        match &config.session_mode {
            SessionMode::Shell => {
                // For shell mode, we don't use Claude at all
                // This is handled separately in PTY
                return Err(ProviderError::InvalidConfig(
                    "Shell mode should not use AiProvider".to_string(),
                ));
            }

            SessionMode::New { session_id } => {
                // Add model if specified
                if let Some(ref model) = config.model {
                    args.push(
                        CString::new("--model")
                            .map_err(|e| ProviderError::CommandBuild(e.to_string()))?,
                    );
                    args.push(
                        CString::new(model.as_str())
                            .map_err(|e| ProviderError::CommandBuild(e.to_string()))?,
                    );
                }

                // Add session ID if specified
                if let Some(ref id) = session_id {
                    args.push(
                        CString::new("--session-id")
                            .map_err(|e| ProviderError::CommandBuild(e.to_string()))?,
                    );
                    args.push(
                        CString::new(id.as_str())
                            .map_err(|e| ProviderError::CommandBuild(e.to_string()))?,
                    );
                }

                // Add prompt if specified (as positional argument)
                if let Some(ref prompt) = config.prompt {
                    args.push(
                        CString::new(prompt.as_str())
                            .map_err(|e| ProviderError::CommandBuild(e.to_string()))?,
                    );
                }
            }

            SessionMode::Resume { session_id } => {
                args.push(
                    CString::new("--resume")
                        .map_err(|e| ProviderError::CommandBuild(e.to_string()))?,
                );
                args.push(
                    CString::new(session_id.as_str())
                        .map_err(|e| ProviderError::CommandBuild(e.to_string()))?,
                );
            }

            SessionMode::OneShot => {
                // Add model if specified
                if let Some(ref model) = config.model {
                    args.push(
                        CString::new("--model")
                            .map_err(|e| ProviderError::CommandBuild(e.to_string()))?,
                    );
                    args.push(
                        CString::new(model.as_str())
                            .map_err(|e| ProviderError::CommandBuild(e.to_string()))?,
                    );
                }

                // Add prompt as positional argument
                if let Some(ref prompt) = config.prompt {
                    args.push(
                        CString::new(prompt.as_str())
                            .map_err(|e| ProviderError::CommandBuild(e.to_string()))?,
                    );
                }
            }
        }

        Ok((cmd, args))
    }

    fn read_session_info(
        &self,
        session_id: &str,
        worktree_path: &Path,
    ) -> ProviderResult<Option<ProviderSessionInfo>> {
        match get_first_user_message(worktree_path, session_id) {
            Some(description) => Ok(Some(ProviderSessionInfo {
                description: Some(description),
            })),
            None => Ok(None),
        }
    }

    fn available_models(&self) -> Vec<&str> {
        vec!["opus", "sonnet", "haiku"]
    }

    fn default_model(&self) -> &str {
        "sonnet"
    }

    fn supports_resume(&self) -> bool {
        true
    }

    fn has_local_sessions(&self) -> bool {
        true
    }
}

// ============ Claude session file parsing (from claude_session.rs) ============

#[derive(Deserialize)]
struct SessionEntry {
    #[serde(rename = "type")]
    entry_type: String,
    message: Option<MessageContent>,
}

#[derive(Deserialize)]
struct MessageContent {
    content: Option<String>,
}

/// Convert worktree path to Claude project folder name
/// /home/lee/study/rust/source/amux/ -> -home-lee-study-rust-source-amux
fn path_to_claude_folder(path: &Path) -> String {
    let path_str = path.to_string_lossy();
    // Trim trailing slash, then replace / with -
    let trimmed = path_str.trim_end_matches('/');
    trimmed.replace('/', "-")
}

/// Get first user message from Claude session
pub fn get_first_user_message(worktree_path: &Path, claude_session_id: &str) -> Option<String> {
    let folder = path_to_claude_folder(worktree_path);
    let claude_home = dirs::home_dir()?.join(".claude/projects").join(&folder);
    let session_file = claude_home.join(format!("{}.jsonl", claude_session_id));

    let file = File::open(&session_file).ok()?;
    let reader = BufReader::new(file);

    for line in reader.lines().map_while(Result::ok) {
        if let Ok(entry) = serde_json::from_str::<SessionEntry>(&line) {
            if entry.entry_type == "user" {
                if let Some(msg) = entry.message {
                    if let Some(content) = msg.content {
                        // Skip system reminders and empty messages
                        if content.is_empty() || content.starts_with("<system-reminder>") {
                            continue;
                        }
                        // Truncate and clean up
                        let clean = content.lines().next().unwrap_or(&content);
                        let truncated = if clean.chars().count() > 35 {
                            format!("{}...", clean.chars().take(35).collect::<String>())
                        } else {
                            clean.to_string()
                        };
                        return Some(truncated);
                    }
                }
            }
        }
    }
    None
}
