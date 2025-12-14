//! Git status operations

use super::super::state::{GitPanelItem, GitSection, GitStatusFile};
use super::super::widgets::VirtualList;
use super::super::App;
use crate::error::TuiError;

type Result<T> = std::result::Result<T, TuiError>;

impl App {
    /// Load git status for current worktree
    pub async fn load_git_status(&mut self) -> Result<()> {
        // Get repo_id and branch first to avoid borrow issues
        let (repo_id, branch) = {
            if let Some(repo) = self.current_repo() {
                if let Some(wt) = repo.current_worktree() {
                    (repo.info.id.clone(), wt.branch.clone())
                } else {
                    return Ok(());
                }
            } else {
                return Ok(());
            }
        };

        let response = self.client.get_git_status(&repo_id, &branch).await?;

        // Update git state in repo
        if let Some(repo) = self.current_repo_mut() {
            repo.git.files.clear();

            for f in response.staged {
                repo.git.files.push(GitStatusFile {
                    path: f.path,
                    status: f.status,
                    section: GitSection::Staged,
                });
            }
            for f in response.unstaged {
                repo.git.files.push(GitStatusFile {
                    path: f.path,
                    status: f.status,
                    section: GitSection::Unstaged,
                });
            }
            for f in response.untracked {
                repo.git.files.push(GitStatusFile {
                    path: f.path,
                    status: f.status,
                    section: GitSection::Untracked,
                });
            }

            repo.git.cursor = 0;
        }
        self.dirty.sidebar = true;

        // Also load comments for this branch
        match self
            .client
            .list_line_comments(&repo_id, &branch, None)
            .await
        {
            Ok(comments) => {
                if let Some(repo) = self.current_repo_mut() {
                    repo.line_comments = comments;
                }
            }
            Err(e) => {
                tracing::warn!("Failed to load comments: {}", e);
            }
        }
        Ok(())
    }

    /// Stage a single file
    pub async fn stage_file(&mut self, file_path: &str) -> Result<()> {
        // Extract data before borrowing client
        let ids = self
            .current_repo()
            .map(|r| r.info.id.clone())
            .zip(self.current_worktree().map(|w| w.branch.clone()));

        if let Some((repo_id, branch)) = ids {
            self.client.stage_file(&repo_id, &branch, file_path).await?;
            self.load_git_status().await?;
        }
        Ok(())
    }

    /// Unstage a single file
    pub async fn unstage_file(&mut self, file_path: &str) -> Result<()> {
        // Extract data before borrowing client
        let ids = self
            .current_repo()
            .map(|r| r.info.id.clone())
            .zip(self.current_worktree().map(|w| w.branch.clone()));

        if let Some((repo_id, branch)) = ids {
            self.client
                .unstage_file(&repo_id, &branch, file_path)
                .await?;
            self.load_git_status().await?;
        }
        Ok(())
    }

    /// Stage all files
    pub async fn stage_all(&mut self) -> Result<()> {
        // Extract data before borrowing client
        let ids = self
            .current_repo()
            .map(|r| r.info.id.clone())
            .zip(self.current_worktree().map(|w| w.branch.clone()));

        if let Some((repo_id, branch)) = ids {
            self.client.stage_all(&repo_id, &branch).await?;
            self.load_git_status().await?;
        }
        Ok(())
    }

    /// Unstage all files
    pub async fn unstage_all(&mut self) -> Result<()> {
        // Extract data before borrowing client
        let ids = self
            .current_repo()
            .map(|r| r.info.id.clone())
            .zip(self.current_worktree().map(|w| w.branch.clone()));

        if let Some((repo_id, branch)) = ids {
            self.client.unstage_all(&repo_id, &branch).await?;
            self.load_git_status().await?;
        }
        Ok(())
    }

    /// Get current git panel item at cursor position
    pub fn current_git_panel_item(&self) -> GitPanelItem {
        let git = match self.git() {
            Some(g) => g,
            None => return GitPanelItem::None,
        };

        let mut pos = 0;
        let sections = [
            GitSection::Staged,
            GitSection::Unstaged,
            GitSection::Untracked,
        ];

        for section in sections {
            let files: Vec<_> = git
                .files
                .iter()
                .enumerate()
                .filter(|(_, f)| f.section == section)
                .collect();

            if files.is_empty() {
                continue;
            }

            // Section header
            if pos == git.cursor {
                return GitPanelItem::Section(section);
            }
            pos += 1;

            // Files in section (if expanded)
            if git.expanded_sections.contains(&section) {
                for (file_idx, _) in files {
                    if pos == git.cursor {
                        return GitPanelItem::File(file_idx);
                    }
                    pos += 1;
                }
            }
        }

        GitPanelItem::None
    }

    /// Toggle git section expansion
    pub fn toggle_git_section_expand(&mut self) {
        if let GitPanelItem::Section(section) = self.current_git_panel_item() {
            if let Some(git) = self.git_mut() {
                if git.expanded_sections.contains(&section) {
                    git.expanded_sections.remove(&section);
                } else {
                    git.expanded_sections.insert(section);
                }
                self.dirty.sidebar = true;
            }
        }
    }

    /// Move cursor up in git status panel
    pub fn git_status_move_up(&mut self) {
        if let Some(git) = self.git_mut() {
            if git.move_up() {
                self.dirty.sidebar = true;
            }
        }
    }

    /// Move cursor down in git status panel
    pub fn git_status_move_down(&mut self) {
        if let Some(git) = self.git_mut() {
            if git.move_down() {
                self.dirty.sidebar = true;
            }
        }
    }

    /// Get file path of currently selected git status file
    pub fn current_git_file_path(&self) -> Option<String> {
        if let GitPanelItem::File(idx) = self.current_git_panel_item() {
            self.git()?.files.get(idx).map(|f| f.path.clone())
        } else {
            None
        }
    }

    /// Check if current git item is staged
    pub fn is_current_git_item_staged(&self) -> bool {
        if let GitPanelItem::File(idx) = self.current_git_panel_item() {
            self.git()
                .and_then(|git| git.files.get(idx))
                .map(|f| f.section == GitSection::Staged)
                .unwrap_or(false)
        } else if let GitPanelItem::Section(section) = self.current_git_panel_item() {
            section == GitSection::Staged
        } else {
            false
        }
    }

    /// Push to remote
    pub async fn git_push(&mut self) -> Result<()> {
        let ids = self
            .current_repo()
            .map(|r| r.info.id.clone())
            .zip(self.current_worktree().map(|w| w.branch.clone()));

        if let Some((repo_id, branch)) = ids {
            let response = self.client.git_push(&repo_id, &branch).await?;
            if response.success {
                self.status_message = Some(response.message);
            } else {
                self.error_message = Some(response.message);
            }
        }
        Ok(())
    }

    /// Pull from remote (fetch + rebase)
    pub async fn git_pull(&mut self) -> Result<()> {
        let ids = self
            .current_repo()
            .map(|r| r.info.id.clone())
            .zip(self.current_worktree().map(|w| w.branch.clone()));

        if let Some((repo_id, branch)) = ids {
            let response = self.client.git_pull(&repo_id, &branch).await?;
            if response.success {
                self.status_message = Some(response.message);
                // Refresh git status after pull
                self.load_git_status().await?;
            } else {
                self.error_message = Some(response.message);
            }
        }
        Ok(())
    }
}
