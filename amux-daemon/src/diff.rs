//! Git diff operations

use crate::error::GitError;
use git2::{Delta, Diff, DiffOptions, Repository, Status, StatusOptions};
use std::path::Path;

/// Information about a changed file
#[derive(Debug, Clone)]
pub struct DiffFileInfo {
    pub path: String,
    pub status: FileStatus,
    pub additions: i32,
    pub deletions: i32,
}

/// File change status
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FileStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
}

/// A single line in a diff
#[derive(Debug, Clone)]
pub struct DiffLine {
    pub line_type: LineType,
    pub content: String,
    pub old_lineno: Option<i32>,
    pub new_lineno: Option<i32>,
}

/// Type of diff line
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineType {
    Header,
    Context,
    Addition,
    Deletion,
}

/// Git diff operations
pub struct DiffOps;

impl DiffOps {
    /// Get list of changed files in worktree (vs HEAD)
    pub fn get_diff_files(worktree_path: &Path) -> Result<Vec<DiffFileInfo>, GitError> {
        let repo = Repository::open(worktree_path)?;
        let mut files = Vec::new();

        // Get HEAD tree for comparison
        let head_tree = match repo.head() {
            Ok(head) => Some(head.peel_to_tree()?),
            Err(_) => None, // New repo with no commits
        };

        // Get diff between HEAD and working directory (including staged)
        let mut diff_opts = DiffOptions::new();
        diff_opts.include_untracked(false); // Handle untracked separately
        diff_opts.recurse_untracked_dirs(false);

        let diff =
            repo.diff_tree_to_workdir_with_index(head_tree.as_ref(), Some(&mut diff_opts))?;

        // Collect file stats from diff
        Self::collect_diff_files(&diff, &mut files)?;

        // Handle untracked files separately
        Self::collect_untracked_files(&repo, &mut files)?;

        // Sort files by path for consistent display
        files.sort_by(|a, b| a.path.cmp(&b.path));

        Ok(files)
    }

    /// Collect files from a git2 Diff
    fn collect_diff_files(diff: &Diff, files: &mut Vec<DiffFileInfo>) -> Result<(), GitError> {
        for delta_idx in 0..diff.deltas().len() {
            let delta = diff.get_delta(delta_idx).unwrap();

            let path = delta
                .new_file()
                .path()
                .or_else(|| delta.old_file().path())
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();

            if path.is_empty() {
                continue;
            }

            let status = match delta.status() {
                Delta::Added => FileStatus::Added,
                Delta::Deleted => FileStatus::Deleted,
                Delta::Modified => FileStatus::Modified,
                Delta::Renamed => FileStatus::Renamed,
                Delta::Copied => FileStatus::Added,
                _ => FileStatus::Modified,
            };

            // We need to iterate through patches to get per-file stats
            let (additions, deletions) = Self::get_file_stats(diff, delta_idx)?;

            files.push(DiffFileInfo {
                path,
                status,
                additions,
                deletions,
            });
        }
        Ok(())
    }

    /// Get addition/deletion stats for a specific file in the diff
    fn get_file_stats(diff: &Diff, delta_idx: usize) -> Result<(i32, i32), GitError> {
        let mut additions = 0i32;
        let mut deletions = 0i32;

        if let Ok(Some(patch)) = git2::Patch::from_diff(diff, delta_idx) {
            let (_, adds, dels) = patch.line_stats()?;
            additions = adds as i32;
            deletions = dels as i32;
        }

        Ok((additions, deletions))
    }

    /// Collect untracked files
    fn collect_untracked_files(
        repo: &Repository,
        files: &mut Vec<DiffFileInfo>,
    ) -> Result<(), GitError> {
        let mut status_opts = StatusOptions::new();
        status_opts.include_untracked(true);
        status_opts.recurse_untracked_dirs(true);
        status_opts.include_ignored(false);

        let statuses = repo.statuses(Some(&mut status_opts))?;

        for entry in statuses.iter() {
            let status = entry.status();
            if status.contains(Status::WT_NEW) {
                if let Some(path) = entry.path() {
                    files.push(DiffFileInfo {
                        path: path.to_string(),
                        status: FileStatus::Untracked,
                        additions: 0,
                        deletions: 0,
                    });
                }
            }
        }

        Ok(())
    }

    /// Get diff content for a specific file
    pub fn get_file_diff(worktree_path: &Path, file_path: &str) -> Result<Vec<DiffLine>, GitError> {
        let repo = Repository::open(worktree_path)?;
        let mut lines = Vec::new();

        // Check if file is untracked
        let mut status_opts = StatusOptions::new();
        status_opts.pathspec(file_path);
        let statuses = repo.statuses(Some(&mut status_opts))?;

        if let Some(entry) = statuses.iter().next() {
            if entry.status().contains(Status::WT_NEW) {
                // Untracked file - show entire content as additions
                return Self::get_untracked_file_diff(worktree_path, file_path);
            }
        }

        // Get HEAD tree
        let head_tree = match repo.head() {
            Ok(head) => Some(head.peel_to_tree()?),
            Err(_) => None,
        };

        // Get diff for this specific file
        let mut diff_opts = DiffOptions::new();
        diff_opts.pathspec(file_path);

        let diff =
            repo.diff_tree_to_workdir_with_index(head_tree.as_ref(), Some(&mut diff_opts))?;

        // Iterate through patches
        for delta_idx in 0..diff.deltas().len() {
            if let Ok(Some(patch)) = git2::Patch::from_diff(&diff, delta_idx) {
                // Iterate through hunks
                for hunk_idx in 0..patch.num_hunks() {
                    let (hunk, _) = patch.hunk(hunk_idx)?;

                    // Add hunk header
                    let header = String::from_utf8_lossy(hunk.header()).to_string();
                    lines.push(DiffLine {
                        line_type: LineType::Header,
                        content: header.trim_end().to_string(),
                        old_lineno: None,
                        new_lineno: None,
                    });

                    // Get lines in this hunk
                    let num_lines = patch.num_lines_in_hunk(hunk_idx)?;
                    for line_idx in 0..num_lines {
                        let line = patch.line_in_hunk(hunk_idx, line_idx)?;

                        let line_type = match line.origin() {
                            '+' => LineType::Addition,
                            '-' => LineType::Deletion,
                            ' ' => LineType::Context,
                            _ => continue, // Skip other markers like '\' for no newline
                        };

                        let content = String::from_utf8_lossy(line.content()).to_string();

                        lines.push(DiffLine {
                            line_type,
                            content: content.trim_end_matches('\n').to_string(),
                            old_lineno: line.old_lineno().map(|n| n as i32),
                            new_lineno: line.new_lineno().map(|n| n as i32),
                        });
                    }
                }
            }
        }

        Ok(lines)
    }

    /// Get diff for an untracked file (show all lines as additions)
    fn get_untracked_file_diff(
        worktree_path: &Path,
        file_path: &str,
    ) -> Result<Vec<DiffLine>, GitError> {
        let full_path = worktree_path.join(file_path);
        let content = std::fs::read_to_string(&full_path)?;

        let mut lines = Vec::new();

        // Add a header
        lines.push(DiffLine {
            line_type: LineType::Header,
            content: format!("@@ -0,0 +1,{} @@ (new file)", content.lines().count()),
            old_lineno: None,
            new_lineno: None,
        });

        // Add all lines as additions
        for (idx, line) in content.lines().enumerate() {
            lines.push(DiffLine {
                line_type: LineType::Addition,
                content: line.to_string(),
                old_lineno: None,
                new_lineno: Some((idx + 1) as i32),
            });
        }

        Ok(lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_repo() -> (TempDir, Repository) {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();

        // Configure user for commits
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Test").unwrap();
        config.set_str("user.email", "test@test.com").unwrap();

        (dir, repo)
    }

    #[test]
    fn test_empty_repo() {
        let (dir, _repo) = create_test_repo();
        let files = DiffOps::get_diff_files(dir.path()).unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn test_untracked_file() {
        let (dir, _repo) = create_test_repo();

        // Create an untracked file
        fs::write(dir.path().join("test.txt"), "hello\n").unwrap();

        let files = DiffOps::get_diff_files(dir.path()).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "test.txt");
        assert_eq!(files[0].status, FileStatus::Untracked);
    }

    #[test]
    fn test_modified_file() {
        let (dir, repo) = create_test_repo();

        // Create and commit a file
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "original\n").unwrap();

        let mut index = repo.index().unwrap();
        index.add_path(Path::new("test.txt")).unwrap();
        index.write().unwrap();

        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = repo.signature().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "Initial", &tree, &[])
            .unwrap();

        // Modify the file
        fs::write(&file_path, "modified\n").unwrap();

        let files = DiffOps::get_diff_files(dir.path()).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "test.txt");
        assert_eq!(files[0].status, FileStatus::Modified);
    }
}
