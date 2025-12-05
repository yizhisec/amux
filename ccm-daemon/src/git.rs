//! Git operations wrapper

use crate::error::GitError;
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
    pub fn open(path: &Path) -> Result<Repository, GitError> {
        Repository::open(path).map_err(|e| GitError::OpenRepo {
            path: path.to_path_buf(),
            source: e,
        })
    }

    /// Get the repository name (directory name)
    pub fn repo_name(path: &Path) -> String {
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string()
    }

    /// List all branches (local)
    pub fn list_branches(repo: &Repository) -> Result<Vec<String>, GitError> {
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
    pub fn current_branch(repo: &Repository) -> Result<String, GitError> {
        let head = repo.head()?;
        head.shorthand()
            .map(|s| s.to_string())
            .ok_or(GitError::NoBranchName)
    }

    /// List all worktrees for a repository
    pub fn list_worktrees(repo: &Repository) -> Result<Vec<WorktreeInfo>, GitError> {
        let mut worktrees = Vec::new();

        // Main worktree
        let main_path = repo.workdir().ok_or(GitError::NoWorkdir)?;
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
                // Get the actual working directory path
                if let Ok(workdir) = Self::worktree_workdir(&wt) {
                    // Get branch from worktree
                    let branch = Self::worktree_branch(&wt).unwrap_or_else(|_| name.to_string());
                    worktrees.push(WorktreeInfo {
                        path: workdir,
                        branch,
                        is_main: false,
                    });
                }
            }
        }

        Ok(worktrees)
    }

    /// Get the branch associated with a worktree
    fn worktree_branch(wt: &git2::Worktree) -> Result<String, GitError> {
        // wt.path() returns the gitdir path (e.g., .git/worktrees/branch-name)
        // We need to open it as a repository to get HEAD
        let wt_repo = Repository::open(wt.path())?;
        Self::current_branch(&wt_repo)
    }

    /// Get the working directory path for a worktree
    fn worktree_workdir(wt: &git2::Worktree) -> Result<PathBuf, GitError> {
        // wt.path() returns the gitdir path, open it to get the workdir
        let wt_repo = Repository::open(wt.path())?;
        wt_repo.workdir().map(|p| p.to_path_buf()).ok_or(GitError::NoWorkdir)
    }

    /// Create a new worktree for a branch
    pub fn create_worktree(
        repo: &Repository,
        branch: &str,
        base_path: &Path,
    ) -> Result<PathBuf, GitError> {
        // Worktree path: {base_path}--{branch}
        let wt_name = branch.replace('/', "-");
        let wt_path = base_path.with_file_name(format!(
            "{}--{}",
            base_path.file_name().unwrap().to_str().unwrap(),
            &wt_name
        ));

        // Check if worktree already exists in git
        if let Ok(wt) = repo.find_worktree(&wt_name) {
            // Worktree exists in git, return its path
            if let Some(path) = wt.path().parent() {
                return Ok(path.to_path_buf());
            }
        }

        // Check if path exists but git doesn't know about it
        if wt_path.exists() {
            // Check if it's a valid git worktree
            if wt_path.join(".git").exists() {
                // It's a git directory, might be an orphaned worktree
                // Try to repair by removing and recreating
                std::fs::remove_dir_all(&wt_path)?;
            } else {
                return Err(GitError::PathNotWorktree(wt_path));
            }
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
        let mut opts = git2::WorktreeAddOptions::new();
        opts.reference(Some(&reference));
        repo.worktree(&wt_name, &wt_path, Some(&opts))?;

        Ok(wt_path)
    }

    /// Remove a worktree
    pub fn remove_worktree(repo: &Repository, branch: &str) -> Result<(), GitError> {
        let wt_name = branch.replace('/', "-");

        // Find and prune the worktree
        if let Ok(wt) = repo.find_worktree(&wt_name) {
            // Get the working directory path before pruning
            // wt.path() returns the gitdir path, need to get actual workdir
            let wt_path = Self::worktree_workdir(&wt).ok();

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

    /// Delete a local branch
    pub fn delete_branch(repo: &Repository, branch: &str) -> Result<(), GitError> {
        // Check if branch has a worktree
        let worktrees = Self::list_worktrees(repo)?;
        if worktrees.iter().any(|wt| wt.branch == branch) {
            return Err(GitError::CannotDeleteBranch {
                branch: branch.to_string(),
                reason: "it has an active worktree".to_string(),
            });
        }

        // Find and delete the branch
        let mut branch_ref = repo
            .find_branch(branch, git2::BranchType::Local)
            .map_err(|_| GitError::BranchNotFound(branch.to_string()))?;

        // Check if it's the current branch in main worktree
        if let Ok(head) = repo.head() {
            if let Some(head_name) = head.shorthand() {
                if head_name == branch {
                    return Err(GitError::CannotDeleteBranch {
                        branch: branch.to_string(),
                        reason: "it is the current branch".to_string(),
                    });
                }
            }
        }

        branch_ref.delete()?;
        Ok(())
    }
}

/// Information about a worktree
#[derive(Debug, Clone)]
pub struct WorktreeInfo {
    pub path: PathBuf,
    pub branch: String,
    pub is_main: bool,
}
