//! Application state management

use crate::repo::Repo;
use crate::session::Session;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Shared application state
pub type SharedState = Arc<RwLock<AppState>>;

/// Application state containing repos and sessions
#[derive(Default)]
pub struct AppState {
    /// Repos indexed by ID
    pub repos: HashMap<String, Repo>,
    /// Sessions indexed by ID
    pub sessions: HashMap<String, Session>,
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get CCM data directory (~/.ccm/)
    pub fn data_dir() -> PathBuf {
        dirs::home_dir()
            .expect("Cannot find home directory")
            .join(".ccm")
    }

    /// Get repos.json path
    pub fn repos_file() -> PathBuf {
        Self::data_dir().join("repos.json")
    }

    /// Get daemon socket path
    pub fn socket_path() -> PathBuf {
        Self::data_dir().join("daemon.sock")
    }

    /// Get daemon PID file path
    pub fn pid_file() -> PathBuf {
        Self::data_dir().join("daemon.pid")
    }

    /// Ensure data directory exists
    pub fn ensure_data_dir() -> anyhow::Result<()> {
        let dir = Self::data_dir();
        if !dir.exists() {
            std::fs::create_dir_all(&dir)?;
        }
        Ok(())
    }
}
