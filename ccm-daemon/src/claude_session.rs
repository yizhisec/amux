//! Claude Code session parsing

use serde::Deserialize;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

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
/// /home/lee/study/rust/source/ccman/ -> -home-lee-study-rust-source-ccman
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
