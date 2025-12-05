//! Repository management

use crate::error::RepoError;
use crate::git::GitOps;
use crate::state::AppState;
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
    pub fn new(path: PathBuf) -> Result<Self, RepoError> {
        // Ensure it's a git repo
        if !GitOps::is_git_repo(&path) {
            return Err(RepoError::NotAGitRepo(path));
        }

        // Canonicalize path
        let path = path.canonicalize().map_err(RepoError::PathCanonicalize)?;

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
pub fn load_repos() -> Result<Vec<Repo>, RepoError> {
    let path = AppState::repos_file();
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(&path).map_err(RepoError::Load)?;
    let repos: Vec<Repo> = serde_json::from_str(&content).map_err(RepoError::Parse)?;
    Ok(repos)
}

/// Save repos to persistent storage
pub fn save_repos(repos: &[Repo]) -> Result<(), RepoError> {
    AppState::ensure_data_dir().map_err(RepoError::Save)?;
    let path = AppState::repos_file();
    let content = serde_json::to_string_pretty(repos).map_err(RepoError::Parse)?;
    std::fs::write(&path, content).map_err(RepoError::Save)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_generate_id_deterministic() {
        let path = PathBuf::from("/test/path");
        let id1 = Repo::generate_id(&path);
        let id2 = Repo::generate_id(&path);
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_generate_id_different_paths() {
        let path1 = PathBuf::from("/test/path1");
        let path2 = PathBuf::from("/test/path2");
        let id1 = Repo::generate_id(&path1);
        let id2 = Repo::generate_id(&path2);
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_new_repo_not_git_repo() {
        let path = PathBuf::from("/tmp/not-a-git-repo-12345");
        let result = Repo::new(path.clone());
        assert!(result.is_err());
        match result {
            Err(RepoError::NotAGitRepo(p)) => assert_eq!(p, path),
            _ => panic!("Expected NotAGitRepo error"),
        }
    }

    #[test]
    fn test_repo_name_extraction() {
        let name = GitOps::repo_name(Path::new("/home/user/projects/my-project"));
        assert_eq!(name, "my-project");
    }
}
