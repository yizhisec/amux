//! Repository management

use crate::git::GitOps;
use crate::state::AppState;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

/// Repository information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repo {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
}

impl Repo {
    /// Create a new repo from a path
    pub fn new(path: PathBuf) -> Result<Self> {
        // Ensure it's a git repo
        if !GitOps::is_git_repo(&path) {
            return Err(anyhow!("Path is not a git repository: {:?}", path));
        }

        // Canonicalize path
        let path = path.canonicalize()?;

        // Generate ID from path hash
        let id = Self::generate_id(&path);
        let name = GitOps::repo_name(&path);

        Ok(Self { id, name, path })
    }

    /// Generate a unique ID from path
    fn generate_id(path: &PathBuf) -> String {
        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }
}

/// Load repos from persistent storage
pub fn load_repos() -> Result<Vec<Repo>> {
    let path = AppState::repos_file();
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(&path)?;
    let repos: Vec<Repo> = serde_json::from_str(&content)?;
    Ok(repos)
}

/// Save repos to persistent storage
pub fn save_repos(repos: &[Repo]) -> Result<()> {
    AppState::ensure_data_dir()?;
    let path = AppState::repos_file();
    let content = serde_json::to_string_pretty(repos)?;
    std::fs::write(&path, content)?;
    Ok(())
}
