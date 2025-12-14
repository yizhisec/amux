//! Diff view operations

use super::super::state::{AsyncAction, DiffItem, Focus, RightPanelView};
use super::super::widgets::VirtualList;
use super::super::App;
use crate::error::TuiError;

type Result<T> = std::result::Result<T, TuiError>;

impl App {
    /// Switch to diff view
    pub async fn switch_to_diff_view(&mut self) -> Result<()> {
        self.right_panel_view = RightPanelView::Diff;
        self.focus = Focus::DiffFiles;
        self.load_diff_files().await?;
        self.load_comments().await?;
        Ok(())
    }

    /// Switch back to previous view (restores focus)
    pub fn switch_to_terminal_view(&mut self) {
        self.right_panel_view = RightPanelView::Terminal;
        // Restore focus to where user was before entering diff
        if !self.restore_focus() {
            // Fallback to sidebar if stack was empty
            self.focus = Focus::Sidebar;
        }
        if let Some(diff) = self.diff_mut() {
            diff.files.clear();
            diff.expanded.clear();
            diff.file_lines.clear();
            diff.cursor = 0;
            diff.scroll_offset = 0;
        }
    }

    /// Load diff files for current worktree
    pub async fn load_diff_files(&mut self) -> Result<()> {
        // Extract data before borrowing client
        let ids = self
            .current_repo()
            .map(|r| r.info.id.clone())
            .zip(self.current_worktree().map(|w| w.branch.clone()));

        if let Some((repo_id, branch)) = ids {
            match self.client.get_diff_files(&repo_id, &branch).await {
                Ok(files) => {
                    // Get pending file before modifying state
                    let pending_file = self.git_mut().and_then(|g| g.pending_diff_file.take());

                    if let Some(diff) = self.diff_mut() {
                        diff.files = files;
                        diff.expanded.clear();
                        diff.file_lines.clear();
                        diff.cursor = 0;
                        diff.scroll_offset = 0;

                        // If there's a pending file to expand, find and expand it
                        if let Some(pending_file) = pending_file {
                            if let Some(idx) =
                                diff.files.iter().position(|f| f.path == pending_file)
                            {
                                diff.cursor = idx;
                                diff.expanded.insert(idx);
                            }
                        }
                    }

                    // Load the file's diff content if we just expanded one
                    self.load_file_diff().await?;
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to load diff: {}", e));
                }
            }

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
        }
        Ok(())
    }

    /// Load diff content for the file that is being expanded
    pub async fn load_file_diff(&mut self) -> Result<()> {
        // Find which file needs loading (the one that's expanded but has no lines)
        let file_info = self.diff().and_then(|diff| {
            diff.expanded
                .iter()
                .find(|&&idx| !diff.file_lines.contains_key(&idx))
                .copied()
                .and_then(|idx| diff.files.get(idx).map(|f| (idx, f.path.clone())))
        });

        let ids = self
            .current_repo()
            .map(|r| r.info.id.clone())
            .zip(self.current_worktree().map(|w| w.branch.clone()));

        if let (Some((file_idx, file_path)), Some((repo_id, branch))) = (file_info, ids) {
            match self
                .client
                .get_file_diff(&repo_id, &branch, &file_path)
                .await
            {
                Ok(response) => {
                    if let Some(diff) = self.diff_mut() {
                        diff.file_lines.insert(file_idx, response.lines);
                    }
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to load file diff: {}", e));
                    if let Some(diff) = self.diff_mut() {
                        diff.expanded.remove(&file_idx);
                    }
                }
            }
        }
        Ok(())
    }

    /// Get current item at cursor position
    pub fn current_diff_item(&self) -> DiffItem {
        let Some(diff) = self.diff() else {
            return DiffItem::None;
        };

        if diff.files.is_empty() {
            return DiffItem::None;
        }

        let mut pos = 0;
        for (file_idx, _) in diff.files.iter().enumerate() {
            // Check if cursor is on this file
            if pos == diff.cursor {
                return DiffItem::File(file_idx);
            }
            pos += 1;

            // Check if cursor is on one of this file's lines
            if diff.expanded.contains(&file_idx) {
                if let Some(lines) = diff.file_lines.get(&file_idx) {
                    for line_idx in 0..lines.len() {
                        if pos == diff.cursor {
                            return DiffItem::Line(file_idx, line_idx);
                        }
                        pos += 1;
                    }
                }
            }
        }

        DiffItem::None
    }

    /// Move cursor up in diff view
    pub fn diff_move_up(&mut self) {
        if let Some(diff) = self.diff_mut() {
            if diff.move_up() {
                self.dirty.sidebar = true;
            }
        }
    }

    /// Move cursor down in diff view
    pub fn diff_move_down(&mut self) {
        if let Some(diff) = self.diff_mut() {
            if diff.move_down() {
                self.dirty.sidebar = true;
            }
        }
    }

    /// Jump to previous file
    pub fn diff_prev_file(&mut self) {
        let Some(diff) = self.diff_mut() else { return };

        let mut pos = 0;
        let mut last_file_pos = 0;
        for (file_idx, _) in diff.files.iter().enumerate() {
            if pos >= diff.cursor {
                // Found current or past cursor, go to last file
                break;
            }
            last_file_pos = pos;
            pos += 1;
            if diff.expanded.contains(&file_idx) {
                pos += diff.file_lines.get(&file_idx).map(|l| l.len()).unwrap_or(0);
            }
        }
        if diff.cursor > 0 {
            diff.cursor = last_file_pos;
            self.dirty.sidebar = true;
        }
    }

    /// Jump to next file
    pub fn diff_next_file(&mut self) {
        let Some(diff) = self.diff_mut() else { return };

        let mut pos = 0;
        for (file_idx, _) in diff.files.iter().enumerate() {
            if pos > diff.cursor {
                // Found next file after cursor
                diff.cursor = pos;
                self.dirty.sidebar = true;
                return;
            }
            pos += 1;
            if diff.expanded.contains(&file_idx) {
                pos += diff.file_lines.get(&file_idx).map(|l| l.len()).unwrap_or(0);
            }
        }
    }

    /// Toggle expansion of current file (only works when cursor is on a file)
    pub fn toggle_diff_expand(&mut self) -> Option<AsyncAction> {
        if let DiffItem::File(file_idx) = self.current_diff_item() {
            let diff = self.diff_mut()?;
            if diff.expanded.contains(&file_idx) {
                // Collapse
                diff.expanded.remove(&file_idx);
                diff.file_lines.remove(&file_idx);
                None
            } else {
                // Expand - need to load diff content
                diff.expanded.insert(file_idx);
                Some(AsyncAction::LoadFileDiff)
            }
        } else {
            None
        }
    }

    /// Toggle diff fullscreen mode
    pub fn toggle_diff_fullscreen(&mut self) {
        if let Some(diff) = self.diff_mut() {
            diff.fullscreen = !diff.fullscreen;
        }
    }

    /// Calculate cursor position for a specific file and line
    pub(super) fn calculate_cursor_for_line(
        &self,
        target_file_idx: usize,
        target_line_idx: usize,
    ) -> usize {
        let Some(diff) = self.diff() else { return 0 };
        let mut cursor = 0;
        for (file_idx, _) in diff.files.iter().enumerate() {
            if file_idx == target_file_idx {
                // Found the file, add the line offset
                return cursor + 1 + target_line_idx; // +1 for file header
            }
            cursor += 1; // File header
            if diff.expanded.contains(&file_idx) {
                if let Some(lines) = diff.file_lines.get(&file_idx) {
                    cursor += lines.len();
                }
            }
        }
        cursor
    }
}
