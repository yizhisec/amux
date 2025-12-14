//! Git operations wrapper

use crate::error::GitError;
use git2::{Cred, FetchOptions, PushOptions, RemoteCallbacks, Repository};
use std::path::{Path, PathBuf};

/// Git repository operations
pub struct GitOps;

impl GitOps {
    /// Check if a path is a git repository
    pub fn is_git_repo(path: &Path) -> bool {
        Repository::open(path).is_ok()
    }

    /// Check if a path is a git worktree (not the main repository)
    /// A worktree has a .git file (not directory) pointing to the main repo's .git/worktrees/
    #[allow(dead_code)]
    pub fn is_worktree(path: &Path) -> bool {
        let git_path = path.join(".git");
        git_path.is_file()
    }

    /// Find the main repository path for a given path.
    /// If the path is a worktree, returns the main repository path.
    /// If it's already the main repository, returns it as-is.
    /// Returns None if not a git repository.
    /// Note: Returns canonicalized path to ensure consistent repo IDs.
    pub fn find_main_repo_path(path: &Path) -> Option<PathBuf> {
        let git_path = path.join(".git");

        if git_path.is_dir() {
            // Regular repository - .git is a directory
            // Canonicalize to ensure consistent path representation
            path.canonicalize().ok()
        } else if git_path.is_file() {
            // Worktree - .git is a file containing: "gitdir: /path/to/.git/worktrees/name"
            if let Ok(content) = std::fs::read_to_string(&git_path) {
                if let Some(gitdir) = content.strip_prefix("gitdir: ") {
                    let gitdir_path = PathBuf::from(gitdir.trim());
                    // Navigate from .git/worktrees/name to .git to repo_root
                    // gitdir_path is like /path/to/main/repo/.git/worktrees/branch-name
                    if let Some(git_dir) = gitdir_path
                        .ancestors()
                        .find(|p| p.file_name().map(|n| n == ".git").unwrap_or(false))
                    {
                        // Canonicalize to ensure consistent path representation
                        return git_dir.parent().and_then(|p| p.canonicalize().ok());
                    }
                }
            }
            None
        } else {
            None
        }
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
        wt_repo
            .workdir()
            .map(|p| p.to_path_buf())
            .ok_or(GitError::NoWorkdir)
    }

    /// Create a new worktree for a branch
    ///
    /// If `base_branch` is provided and `branch` doesn't exist, the new branch
    /// will be created from `base_branch`. Otherwise falls back to HEAD.
    pub fn create_worktree(
        repo: &Repository,
        branch: &str,
        base_path: &Path,
        base_branch: Option<&str>,
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
            // Create branch from base_branch if provided, otherwise from HEAD
            let commit = if let Some(base) = base_branch {
                let base_ref = repo
                    .find_branch(base, git2::BranchType::Local)
                    .map_err(|_| GitError::BranchNotFound(base.to_string()))?;
                base_ref.get().peel_to_commit()?
            } else {
                let head = repo.head()?;
                head.peel_to_commit()?
            };
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

/// File status in git
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitFileStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
}

/// A file with its git status
#[derive(Debug, Clone)]
pub struct GitStatusFile {
    pub path: String,
    pub status: GitFileStatus,
}

/// Git status result with categorized files
#[derive(Debug, Clone, Default)]
pub struct GitStatusResult {
    pub staged: Vec<GitStatusFile>,
    pub unstaged: Vec<GitStatusFile>,
    pub untracked: Vec<GitStatusFile>,
}

impl GitOps {
    /// Get the git status for a repository (worktree)
    pub fn get_status(repo: &Repository) -> Result<GitStatusResult, GitError> {
        let mut opts = git2::StatusOptions::new();
        opts.include_untracked(true)
            .recurse_untracked_dirs(true)
            .include_ignored(false);

        let statuses = repo.statuses(Some(&mut opts))?;
        let mut result = GitStatusResult::default();

        for entry in statuses.iter() {
            let path = entry.path().unwrap_or("").to_string();
            let status = entry.status();

            // Check INDEX status (staged)
            if status.is_index_new() {
                result.staged.push(GitStatusFile {
                    path: path.clone(),
                    status: GitFileStatus::Added,
                });
            } else if status.is_index_modified() {
                result.staged.push(GitStatusFile {
                    path: path.clone(),
                    status: GitFileStatus::Modified,
                });
            } else if status.is_index_deleted() {
                result.staged.push(GitStatusFile {
                    path: path.clone(),
                    status: GitFileStatus::Deleted,
                });
            } else if status.is_index_renamed() {
                result.staged.push(GitStatusFile {
                    path: path.clone(),
                    status: GitFileStatus::Renamed,
                });
            }

            // Check WT status (unstaged/untracked)
            if status.is_wt_new() {
                result.untracked.push(GitStatusFile {
                    path: path.clone(),
                    status: GitFileStatus::Untracked,
                });
            } else if status.is_wt_modified() {
                result.unstaged.push(GitStatusFile {
                    path: path.clone(),
                    status: GitFileStatus::Modified,
                });
            } else if status.is_wt_deleted() {
                result.unstaged.push(GitStatusFile {
                    path: path.clone(),
                    status: GitFileStatus::Deleted,
                });
            } else if status.is_wt_renamed() {
                result.unstaged.push(GitStatusFile {
                    path: path.clone(),
                    status: GitFileStatus::Renamed,
                });
            }
        }

        Ok(result)
    }

    /// Stage a file (add to index)
    pub fn stage_file(repo: &Repository, path: &str) -> Result<(), GitError> {
        let mut index = repo.index()?;
        let file_path = Path::new(path);

        // Check if file exists - if deleted, remove from index
        let workdir = repo.workdir().ok_or(GitError::NoWorkdir)?;
        let full_path = workdir.join(file_path);

        if full_path.exists() {
            index.add_path(file_path)?;
        } else {
            // File was deleted, remove from index
            index.remove_path(file_path)?;
        }

        index.write()?;
        Ok(())
    }

    /// Unstage a file (reset to HEAD)
    pub fn unstage_file(repo: &Repository, path: &str) -> Result<(), GitError> {
        let head = repo.head()?;
        let head_commit = head.peel_to_commit()?;
        let file_path = Path::new(path);

        repo.reset_default(Some(&head_commit.into_object()), [file_path])?;
        Ok(())
    }

    /// Stage all files
    pub fn stage_all(repo: &Repository) -> Result<(), GitError> {
        let mut index = repo.index()?;

        // Add all tracked files
        index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)?;

        // Handle deleted files
        let statuses = repo.statuses(None)?;
        for entry in statuses.iter() {
            if entry.status().is_wt_deleted() {
                if let Some(path) = entry.path() {
                    index.remove_path(Path::new(path))?;
                }
            }
        }

        index.write()?;
        Ok(())
    }

    /// Unstage all files
    pub fn unstage_all(repo: &Repository) -> Result<(), GitError> {
        let head = repo.head()?;
        let head_commit = head.peel_to_commit()?;

        // Get all staged files
        let mut opts = git2::StatusOptions::new();
        opts.include_untracked(false);
        let statuses = repo.statuses(Some(&mut opts))?;

        // Collect paths as owned PathBufs to avoid lifetime issues
        let paths: Vec<PathBuf> = statuses
            .iter()
            .filter_map(|e| {
                let status = e.status();
                if status.is_index_new()
                    || status.is_index_modified()
                    || status.is_index_deleted()
                    || status.is_index_renamed()
                {
                    e.path().map(PathBuf::from)
                } else {
                    None
                }
            })
            .collect();

        if !paths.is_empty() {
            let path_refs: Vec<&Path> = paths.iter().map(|p| p.as_path()).collect();
            repo.reset_default(Some(&head_commit.into_object()), &path_refs)?;
        }

        Ok(())
    }

    /// Create remote callbacks with SSH agent authentication
    fn create_remote_callbacks<'a>() -> RemoteCallbacks<'a> {
        let mut callbacks = RemoteCallbacks::new();
        callbacks.credentials(|_url, username_from_url, allowed_types| {
            // Try SSH agent first
            if allowed_types.contains(git2::CredentialType::SSH_KEY) {
                let username = username_from_url.unwrap_or("git");
                // Try SSH agent
                if let Ok(cred) = Cred::ssh_key_from_agent(username) {
                    return Ok(cred);
                }
                // Fallback: try default SSH key locations
                let home = std::env::var("HOME").unwrap_or_default();
                for key_name in &["id_ed25519", "id_rsa", "id_ecdsa"] {
                    let key_path = PathBuf::from(&home).join(".ssh").join(key_name);
                    if key_path.exists() {
                        if let Ok(cred) = Cred::ssh_key(username, None, &key_path, None) {
                            return Ok(cred);
                        }
                    }
                }
            }
            // Try default credentials (for HTTPS with credential helper)
            if allowed_types.contains(git2::CredentialType::DEFAULT) {
                return Cred::default();
            }
            Err(git2::Error::from_str("no valid credentials available"))
        });
        callbacks
    }

    /// Push current branch to remote
    pub fn push(repo: &Repository, remote_name: &str) -> Result<String, GitError> {
        let head = repo.head()?;
        let branch_name = head
            .shorthand()
            .ok_or(GitError::NoBranchName)?;

        let refspec = format!("refs/heads/{}:refs/heads/{}", branch_name, branch_name);

        let mut remote = repo.find_remote(remote_name)?;
        let callbacks = Self::create_remote_callbacks();
        let mut push_opts = PushOptions::new();
        push_opts.remote_callbacks(callbacks);

        remote.push(&[&refspec], Some(&mut push_opts))?;

        Ok(format!("Pushed {} to {}", branch_name, remote_name))
    }

    /// Pull (fetch + rebase) from remote
    pub fn pull(repo: &Repository, remote_name: &str) -> Result<String, GitError> {
        let head = repo.head()?;
        let branch_name = head
            .shorthand()
            .ok_or(GitError::NoBranchName)?
            .to_string();

        // Fetch from remote
        let mut remote = repo.find_remote(remote_name)?;
        let callbacks = Self::create_remote_callbacks();
        let mut fetch_opts = FetchOptions::new();
        fetch_opts.remote_callbacks(callbacks);

        let refspec = format!("refs/heads/{}", branch_name);
        remote.fetch(&[&refspec], Some(&mut fetch_opts), None)?;

        // Get the fetch head
        let fetch_head = repo.find_reference("FETCH_HEAD")?;
        let fetch_commit = fetch_head.peel_to_commit()?;

        // Get current HEAD commit
        let head_commit = head.peel_to_commit()?;

        // Check if rebase is needed
        if head_commit.id() == fetch_commit.id() {
            return Ok("Already up to date".to_string());
        }

        // Find merge base
        let merge_base = repo.merge_base(head_commit.id(), fetch_commit.id())?;

        // If HEAD is ancestor of fetch, we can fast-forward
        if merge_base == head_commit.id() {
            // Fast-forward: just move HEAD to fetch_commit
            let refname = format!("refs/heads/{}", branch_name);
            repo.reference(
                &refname,
                fetch_commit.id(),
                true,
                &format!("pull: fast-forward to {}", fetch_commit.id()),
            )?;
            repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))?;
            return Ok(format!("Fast-forwarded to {}", &fetch_commit.id().to_string()[..7]));
        }

        // Need actual rebase - this is complex in git2, use annotated commit
        let annotated = repo.find_annotated_commit(fetch_commit.id())?;

        // Start rebase
        let mut rebase = repo.rebase(None, Some(&annotated), None, None)?;

        let signature = repo.signature()?;

        // Apply each commit
        while let Some(op) = rebase.next() {
            let _op = op?;
            // Commit the rebased changes
            if let Err(e) = rebase.commit(None, &signature, None) {
                rebase.abort()?;
                return Err(GitError::Git(e));
            }
        }

        rebase.finish(Some(&signature))?;

        Ok(format!("Rebased onto {}", &fetch_commit.id().to_string()[..7]))
    }
}
