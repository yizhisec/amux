//! Session persistence - save and restore sessions across daemon restarts

use crate::session::Session;
use crate::state::AppState;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{info, warn};

/// Serializable session metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub id: String,
    pub name: String,
    pub repo_id: String,
    pub branch: String,
    pub worktree_path: PathBuf,
    pub created_at: u64,
    pub updated_at: u64,
}

impl SessionMeta {
    /// Create metadata from a session
    pub fn from_session(session: &Session) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            id: session.id.clone(),
            name: session.name.clone(),
            repo_id: session.repo_id.clone(),
            branch: session.branch.clone(),
            worktree_path: session.worktree_path.clone(),
            created_at: now,
            updated_at: now,
        }
    }
}

/// Get sessions directory (~/.ccm/sessions/)
pub fn sessions_dir() -> PathBuf {
    AppState::data_dir().join("sessions")
}

/// Get session directory (~/.ccm/sessions/<session_id>/)
pub fn session_dir(session_id: &str) -> PathBuf {
    sessions_dir().join(session_id)
}

/// Get session metadata file path
pub fn session_meta_file(session_id: &str) -> PathBuf {
    session_dir(session_id).join("meta.json")
}

/// Get session history file path
pub fn session_history_file(session_id: &str) -> PathBuf {
    session_dir(session_id).join("history.bin")
}

/// Ensure sessions directory exists
pub fn ensure_sessions_dir() -> Result<()> {
    let dir = sessions_dir();
    if !dir.exists() {
        std::fs::create_dir_all(&dir)?;
    }
    Ok(())
}

/// Save session metadata
pub fn save_session_meta(session: &Session) -> Result<()> {
    ensure_sessions_dir()?;

    let dir = session_dir(&session.id);
    if !dir.exists() {
        std::fs::create_dir_all(&dir)?;
    }

    let meta = SessionMeta::from_session(session);
    let path = session_meta_file(&session.id);
    let content = serde_json::to_string_pretty(&meta)?;
    std::fs::write(&path, content)?;

    Ok(())
}

/// Save session terminal history
pub fn save_session_history(session: &Session) -> Result<()> {
    let path = session_history_file(&session.id);

    // Get raw output buffer
    let history = session.get_screen_state();
    if history.is_empty() {
        return Ok(());
    }

    std::fs::write(&path, &history)?;
    Ok(())
}

/// Load session metadata
pub fn load_session_meta(session_id: &str) -> Result<SessionMeta> {
    let path = session_meta_file(session_id);
    let content = std::fs::read_to_string(&path)?;
    let meta: SessionMeta = serde_json::from_str(&content)?;
    Ok(meta)
}

/// Load session terminal history
pub fn load_session_history(session_id: &str) -> Result<Vec<u8>> {
    let path = session_history_file(session_id);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let history = std::fs::read(&path)?;
    Ok(history)
}

/// Load all persisted sessions
pub fn load_all_sessions() -> Result<Vec<SessionMeta>> {
    let dir = sessions_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut sessions = Vec::new();

    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let session_id = entry.file_name().to_string_lossy().to_string();
            match load_session_meta(&session_id) {
                Ok(meta) => sessions.push(meta),
                Err(e) => {
                    warn!("Failed to load session {}: {}", session_id, e);
                }
            }
        }
    }

    info!("Loaded {} sessions from disk", sessions.len());
    Ok(sessions)
}

/// Delete session persistence data
pub fn delete_session_data(session_id: &str) -> Result<()> {
    let dir = session_dir(session_id);
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    Ok(())
}

/// Save session (metadata + history)
#[allow(dead_code)]
pub fn save_session(session: &Session) -> Result<()> {
    save_session_meta(session)?;
    save_session_history(session)?;
    Ok(())
}
