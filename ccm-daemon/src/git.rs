//! Git operations wrapper

use anyhow::{anyhow, Context, Result};
use git2::Repository;
use std::path::{Path, PathBuf};

/// Git repository operations
pub struct GitOps;

impl GitOps {
    /// Check if a path is a git repository
    pub fn is_git_repo(path: &Path) -> bool {
        Repository::open(path).is_ok()
    }

    /// Open a repository at the given path
    pub fn open(path: &Path) -> Result<Repository> {
        Repository::open(path).with_context(|| format!("Failed to open git repo at {:?}", path))
    }

    /// Get the repository name (directory name)
    pub fn repo_name(path: &Path) -> String {
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string()
    }

    /// List all branches (local)
    pub fn list_branches(repo: &Repository) -> Result<Vec<String>> {
        let branches = repo.branches(Some(git2::BranchType::Local))?;
        let mut result = Vec::new();
        for branch in branches {
            let (branch, _) = branch?;
            if let Some(name) = branch.name()? {
                result.push(name.to_string());
            }
        }
        Ok(result)
    }

    /// Get current branch name
    pub fn current_branch(repo: &Repository) -> Result<String> {
        let head = repo.head()?;
        head.shorthand()
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow!("Cannot get current branch name"))
    }

    /// List all worktrees for a repository
    pub fn list_worktrees(repo: &Repository) -> Result<Vec<WorktreeInfo>> {
        let mut worktrees = Vec::new();

        // Main worktree
        let main_path = repo
            .workdir()
            .ok_or_else(|| anyhow!("Repository has no working directory"))?;
        let main_branch = Self::current_branch(repo).unwrap_or_else(|_| "HEAD".to_string());
        worktrees.push(WorktreeInfo {
            path: main_path.to_path_buf(),
            branch: main_branch,
            is_main: true,
        });

        // Additional worktrees
        let wt_names = repo.worktrees()?;
        for name in wt_names.iter().flatten() {
            if let Ok(wt) = repo.find_worktree(name) {
                if let Some(path) = wt.path().parent() {
                    // Get branch from worktree
                    let branch = Self::worktree_branch(&wt).unwrap_or_else(|_| name.to_string());
                    worktrees.push(WorktreeInfo {
                        path: path.to_path_buf(),
                        branch,
                        is_main: false,
                    });
                }
            }
        }

        Ok(worktrees)
    }

    /// Get the branch associated with a worktree
    fn worktree_branch(wt: &git2::Worktree) -> Result<String> {
        // Open the worktree as a repository to get its HEAD
        let wt_path = wt
            .path()
            .parent()
            .ok_or_else(|| anyhow!("Invalid worktree path"))?;
        let wt_repo = Repository::open(wt_path)?;
        Self::current_branch(&wt_repo)
    }

    /// Create a new worktree for a branch
    pub fn create_worktree(repo: &Repository, branch: &str, base_path: &Path) -> Result<PathBuf> {
        // Worktree path: {base_path}--{branch}
        let wt_path = base_path.with_file_name(format!(
            "{}--{}",
            base_path.file_name().unwrap().to_str().unwrap(),
            branch.replace('/', "-")
        ));

        // Check if worktree already exists
        if wt_path.exists() {
            return Err(anyhow!("Worktree path already exists: {:?}", wt_path));
        }

        // Find or create the branch
        let reference = if let Ok(branch_ref) = repo.find_branch(branch, git2::BranchType::Local) {
            branch_ref.into_reference()
        } else {
            // Create branch from HEAD if it doesn't exist
            let head = repo.head()?;
            let commit = head.peel_to_commit()?;
            repo.branch(branch, &commit, false)?.into_reference()
        };

        // Create worktree
        let wt_name = branch.replace('/', "-");
        let mut opts = git2::WorktreeAddOptions::new();
        opts.reference(Some(&reference));
        repo.worktree(&wt_name, &wt_path, Some(&opts))?;

        Ok(wt_path)
    }

    /// Remove a worktree
    pub fn remove_worktree(repo: &Repository, branch: &str) -> Result<()> {
        let wt_name = branch.replace('/', "-");

        // Find and prune the worktree
        if let Ok(wt) = repo.find_worktree(&wt_name) {
            // Get the path before pruning
            let wt_path = wt.path().parent().map(|p| p.to_path_buf());

            // Prune the worktree (remove from git's tracking)
            wt.prune(Some(
                &mut git2::WorktreePruneOptions::new()
                    .valid(true)
                    .working_tree(true),
            ))?;

            // Remove the directory if it exists
            if let Some(path) = wt_path {
                if path.exists() {
                    std::fs::remove_dir_all(&path)?;
                }
            }
        }

        Ok(())
    }

    /// Find worktree path for a branch
    pub fn find_worktree_path(repo: &Repository, branch: &str) -> Option<PathBuf> {
        let worktrees = Self::list_worktrees(repo).ok()?;
        worktrees
            .into_iter()
            .find(|wt| wt.branch == branch)
            .map(|wt| wt.path)
    }
}

/// Information about a worktree
#[derive(Debug, Clone)]
pub struct WorktreeInfo {
    pub path: PathBuf,
    pub branch: String,
    pub is_main: bool,
}
