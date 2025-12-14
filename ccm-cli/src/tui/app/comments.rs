//! Line comment operations

use super::super::state::{DiffItem, InputMode};
use super::super::App;
use crate::error::TuiError;
use ccm_proto::daemon::LineCommentInfo;

type Result<T> = std::result::Result<T, TuiError>;

impl App {
    /// Start adding a line comment (only works when cursor is on a diff line)
    pub fn start_add_line_comment(&mut self) {
        if let DiffItem::Line(file_idx, line_idx) = self.current_diff_item() {
            let Some(diff) = self.diff() else { return };
            if let (Some(file), Some(lines)) = (
                diff.files.get(file_idx).cloned(),
                diff.file_lines.get(&file_idx),
            ) {
                if let Some(diff_line) = lines.get(line_idx).cloned() {
                    // Get actual line number from diff info
                    let line_number = diff_line
                        .new_lineno
                        .unwrap_or(diff_line.old_lineno.unwrap_or(line_idx as i32));

                    self.save_focus();
                    self.input_mode = InputMode::AddLineComment {
                        file_path: file.path.clone(),
                        line_number,
                        line_type: diff_line.line_type,
                    };
                    self.text_input.clear();
                }
            }
        } else {
            self.status_message = Some("Move cursor to a diff line to add comment".to_string());
        }
    }

    /// Submit the current line comment
    pub async fn submit_line_comment(&mut self) -> Result<()> {
        let (file_path, line_number, line_type) = match &self.input_mode {
            InputMode::AddLineComment {
                file_path,
                line_number,
                line_type,
            } => (file_path.clone(), *line_number, *line_type),
            _ => return Ok(()),
        };

        let comment_text = self.text_input.trim().to_string();
        self.input_mode = InputMode::Normal;
        self.text_input.clear();
        self.restore_focus();

        if comment_text.is_empty() {
            self.status_message = Some("Comment cannot be empty".to_string());
            return Ok(());
        }

        // Get current repo and branch
        let ids = self
            .current_repo()
            .map(|r| r.info.id.clone())
            .zip(self.current_worktree().map(|w| w.branch.clone()));

        if let Some((repo_id, branch)) = ids {
            match self
                .client
                .create_line_comment(
                    &repo_id,
                    &branch,
                    &file_path,
                    line_number,
                    line_type,
                    &comment_text,
                )
                .await
            {
                Ok(comment) => {
                    if let Some(repo) = self.current_repo_mut() {
                        repo.line_comments.push(comment);
                    }
                    self.status_message = Some("Comment added".to_string());
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to add comment: {}", e));
                }
            }
        }

        Ok(())
    }

    /// Load comments for current branch
    pub async fn load_comments(&mut self) -> Result<()> {
        let ids = self
            .current_repo()
            .map(|r| r.info.id.clone())
            .zip(self.current_worktree().map(|w| w.branch.clone()));

        if let Some((repo_id, branch)) = ids {
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
                    if let Some(repo) = self.current_repo_mut() {
                        repo.line_comments.clear();
                    }
                }
            }
        }
        Ok(())
    }

    /// Get comments for a specific file and line
    pub fn get_line_comment(&self, file_path: &str, line_number: i32) -> Option<&LineCommentInfo> {
        self.line_comments()
            .iter()
            .find(|c| c.file_path == file_path && c.line_number == line_number)
    }

    /// Check if a line has a comment
    pub fn has_line_comment(&self, file_path: &str, line_number: i32) -> bool {
        self.get_line_comment(file_path, line_number).is_some()
    }

    /// Count comments for a specific file
    pub fn count_file_comments(&self, file_path: &str) -> usize {
        self.line_comments()
            .iter()
            .filter(|c| c.file_path == file_path)
            .count()
    }

    /// Start editing an existing comment on current line
    pub fn start_edit_line_comment(&mut self) {
        // Extract needed data first to avoid borrow conflicts
        let edit_info: Option<(String, String, i32, String)> = {
            if let DiffItem::Line(file_idx, line_idx) = self.current_diff_item() {
                let diff = self.diff();
                if let (Some(file), Some(lines)) = (
                    diff.and_then(|d| d.files.get(file_idx)),
                    diff.and_then(|d| d.file_lines.get(&file_idx)),
                ) {
                    if let Some(diff_line) = lines.get(line_idx) {
                        let line_number = diff_line
                            .new_lineno
                            .unwrap_or(diff_line.old_lineno.unwrap_or(line_idx as i32));

                        // Check if there's a comment on this line
                        self.get_line_comment(&file.path, line_number)
                            .map(|comment| {
                                (
                                    comment.id.clone(),
                                    file.path.clone(),
                                    line_number,
                                    comment.comment.clone(),
                                )
                            })
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        };

        // Now mutate self
        if let Some((comment_id, file_path, line_number, comment_text)) = edit_info {
            self.save_focus();
            self.input_mode = InputMode::EditLineComment {
                comment_id,
                file_path,
                line_number,
            };
            self.text_input.set_content(comment_text);
        } else {
            self.status_message = Some("No comment on this line to edit".to_string());
        }
    }

    /// Update an existing line comment
    pub async fn update_line_comment(&mut self) -> Result<()> {
        let comment_id = match &self.input_mode {
            InputMode::EditLineComment { comment_id, .. } => comment_id.clone(),
            _ => return Ok(()),
        };

        let comment_text = self.text_input.trim().to_string();
        self.input_mode = InputMode::Normal;
        self.text_input.clear();
        self.restore_focus();

        if comment_text.is_empty() {
            self.status_message = Some("Comment cannot be empty".to_string());
            return Ok(());
        }

        match self
            .client
            .update_line_comment(&comment_id, &comment_text)
            .await
        {
            Ok(updated) => {
                // Update in local list
                if let Some(repo) = self.current_repo_mut() {
                    if let Some(comment) =
                        repo.line_comments.iter_mut().find(|c| c.id == comment_id)
                    {
                        comment.comment = updated.comment;
                    }
                }
                self.status_message = Some("Comment updated".to_string());
            }
            Err(e) => {
                self.error_message = Some(format!("Failed to update comment: {}", e));
            }
        }

        Ok(())
    }

    /// Delete comment on current line
    pub async fn delete_current_line_comment(&mut self) -> Result<()> {
        if let DiffItem::Line(file_idx, line_idx) = self.current_diff_item() {
            let Some(diff) = self.diff() else {
                return Ok(());
            };
            if let (Some(file), Some(lines)) =
                (diff.files.get(file_idx), diff.file_lines.get(&file_idx))
            {
                if let Some(diff_line) = lines.get(line_idx) {
                    let line_number = diff_line
                        .new_lineno
                        .unwrap_or(diff_line.old_lineno.unwrap_or(line_idx as i32));

                    if let Some(comment) = self
                        .line_comments()
                        .iter()
                        .find(|c| c.file_path == file.path && c.line_number == line_number)
                    {
                        let comment_id = comment.id.clone();

                        match self.client.delete_line_comment(&comment_id).await {
                            Ok(_) => {
                                if let Some(repo) = self.current_repo_mut() {
                                    repo.line_comments.retain(|c| c.id != comment_id);
                                }
                                self.status_message = Some("Comment deleted".to_string());
                            }
                            Err(e) => {
                                self.error_message =
                                    Some(format!("Failed to delete comment: {}", e));
                            }
                        }
                        return Ok(());
                    }
                }
            }
        }
        self.status_message = Some("No comment on this line to delete".to_string());
        Ok(())
    }

    /// Jump to next line with a comment
    pub fn jump_to_next_comment(&mut self) {
        let current = self.current_diff_item();
        let (current_file_idx, current_line_idx) = match current {
            DiffItem::File(f) => (f, 0),
            DiffItem::Line(f, l) => (f, l),
            DiffItem::None => return,
        };

        // Build a flat list of (file_idx, line_idx, line_number, file_path)
        let all_lines: Vec<(usize, usize, i32, String)> = {
            let Some(diff) = self.diff() else { return };
            let mut lines = Vec::new();
            for (file_idx, file) in diff.files.iter().enumerate() {
                if diff.expanded.contains(&file_idx) {
                    if let Some(diff_lines) = diff.file_lines.get(&file_idx) {
                        for (line_idx, diff_line) in diff_lines.iter().enumerate() {
                            let line_number = diff_line
                                .new_lineno
                                .unwrap_or(diff_line.old_lineno.unwrap_or(line_idx as i32));
                            lines.push((file_idx, line_idx, line_number, file.path.clone()));
                        }
                    }
                }
            }
            lines
        };

        // Find current position in flat list
        let current_pos = all_lines
            .iter()
            .position(|(f, l, _, _)| *f == current_file_idx && *l >= current_line_idx)
            .unwrap_or(0);

        // Find next comment after current position
        for (file_idx, line_idx, line_number, file_path) in all_lines.iter().skip(current_pos + 1) {
            if self.has_line_comment(file_path, *line_number) {
                let cursor = self.calculate_cursor_for_line(*file_idx, *line_idx);
                if let Some(diff) = self.diff_mut() {
                    diff.cursor = cursor;
                }
                self.status_message = Some(format!("Jumped to comment at line {}", line_number));
                return;
            }
        }

        // Wrap around - search from beginning
        for (file_idx, line_idx, line_number, file_path) in all_lines.iter().take(current_pos + 1) {
            if self.has_line_comment(file_path, *line_number) {
                let cursor = self.calculate_cursor_for_line(*file_idx, *line_idx);
                if let Some(diff) = self.diff_mut() {
                    diff.cursor = cursor;
                }
                self.status_message = Some(format!("Jumped to comment at line {}", line_number));
                return;
            }
        }

        self.status_message = Some("No comments to jump to".to_string());
    }

    /// Jump to previous line with a comment
    pub fn jump_to_prev_comment(&mut self) {
        let current = self.current_diff_item();
        let (current_file_idx, current_line_idx) = match current {
            DiffItem::File(f) => (f, 0),
            DiffItem::Line(f, l) => (f, l),
            DiffItem::None => return,
        };

        // Build a flat list of (file_idx, line_idx, line_number, file_path)
        let all_lines: Vec<(usize, usize, i32, String)> = {
            let Some(diff) = self.diff() else { return };
            let mut lines = Vec::new();
            for (file_idx, file) in diff.files.iter().enumerate() {
                if diff.expanded.contains(&file_idx) {
                    if let Some(diff_lines) = diff.file_lines.get(&file_idx) {
                        for (line_idx, diff_line) in diff_lines.iter().enumerate() {
                            let line_number = diff_line
                                .new_lineno
                                .unwrap_or(diff_line.old_lineno.unwrap_or(line_idx as i32));
                            lines.push((file_idx, line_idx, line_number, file.path.clone()));
                        }
                    }
                }
            }
            lines
        };

        // Find current position in flat list
        let current_pos = all_lines
            .iter()
            .position(|(f, l, _, _)| *f == current_file_idx && *l >= current_line_idx)
            .unwrap_or(all_lines.len());

        // Find previous comment before current position
        for (file_idx, line_idx, line_number, file_path) in all_lines.iter().take(current_pos).rev()
        {
            if self.has_line_comment(file_path, *line_number) {
                let cursor = self.calculate_cursor_for_line(*file_idx, *line_idx);
                if let Some(diff) = self.diff_mut() {
                    diff.cursor = cursor;
                }
                self.status_message = Some(format!("Jumped to comment at line {}", line_number));
                return;
            }
        }

        // Wrap around - search from end
        for (file_idx, line_idx, line_number, file_path) in all_lines.iter().skip(current_pos).rev()
        {
            if self.has_line_comment(file_path, *line_number) {
                let cursor = self.calculate_cursor_for_line(*file_idx, *line_idx);
                if let Some(diff) = self.diff_mut() {
                    diff.cursor = cursor;
                }
                self.status_message = Some(format!("Jumped to comment at line {}", line_number));
                return;
            }
        }

        self.status_message = Some("No comments to jump to".to_string());
    }

    /// Submit all comments as a review to Claude
    pub async fn submit_review_to_claude(&mut self) -> Result<()> {
        if self.line_comments().is_empty() {
            self.status_message = Some("No comments to submit".to_string());
            return Ok(());
        }

        // Build the review prompt
        let mut prompt = String::from("Please help me review the following code changes:\n\n");

        // Group comments by file
        let mut by_file: std::collections::HashMap<String, Vec<&LineCommentInfo>> =
            std::collections::HashMap::new();
        for comment in self.line_comments() {
            by_file
                .entry(comment.file_path.clone())
                .or_default()
                .push(comment);
        }

        for (file_path, comments) in by_file {
            prompt.push_str(&format!("## File: {}\n\n", file_path));

            for comment in comments {
                let line_type_str = match comment.line_type {
                    3 => "+", // Addition
                    4 => "-", // Deletion
                    _ => " ", // Context
                };

                prompt.push_str(&format!(
                    "### Line {} ({})\n",
                    comment.line_number, line_type_str
                ));
                prompt.push_str(&format!("Comment: {}\n\n", comment.comment));
            }
        }

        prompt.push_str("---\nPlease provide your suggestions for the above comments.\n");

        // Switch to terminal view and send to PTY
        self.switch_to_terminal_view();

        // Connect if needed and send
        if self.terminal_stream.is_none() && self.terminal.active_session_id.is_some() {
            self.enter_terminal().await?;
        }

        if self.terminal_stream.is_some() {
            self.send_to_terminal(prompt.into_bytes()).await?;
            self.status_message = Some("Review sent to Claude".to_string());
        } else {
            self.error_message = Some("No active session to send review".to_string());
        }

        Ok(())
    }
}
