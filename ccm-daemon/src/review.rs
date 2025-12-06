//! Review/comment persistence module
//!
//! Stores line comments in ~/.ccm/reviews/{repo_id}/{branch}/comments.json

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

/// Comment storage path: ~/.ccm/reviews/{repo_id}/{branch}/comments.json
fn get_review_dir(repo_id: &str, branch: &str) -> Result<PathBuf> {
    let ccm_dir = dirs::home_dir()
        .context("Failed to get home directory")?
        .join(".ccm")
        .join("reviews")
        .join(repo_id)
        .join(branch);
    Ok(ccm_dir)
}

fn get_comments_file(repo_id: &str, branch: &str) -> Result<PathBuf> {
    Ok(get_review_dir(repo_id, branch)?.join("comments.json"))
}

/// Line type for a comment
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommentLineType {
    Context,
    Addition,
    Deletion,
    Header,
}

impl From<i32> for CommentLineType {
    fn from(value: i32) -> Self {
        match value {
            1 => CommentLineType::Header,
            2 => CommentLineType::Context,
            3 => CommentLineType::Addition,
            4 => CommentLineType::Deletion,
            _ => CommentLineType::Context,
        }
    }
}

impl From<CommentLineType> for i32 {
    fn from(value: CommentLineType) -> Self {
        match value {
            CommentLineType::Header => 1,
            CommentLineType::Context => 2,
            CommentLineType::Addition => 3,
            CommentLineType::Deletion => 4,
        }
    }
}

/// A comment on a diff line
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineComment {
    pub id: String,
    pub file_path: String,
    pub line_number: i32,
    pub line_type: CommentLineType,
    pub comment: String,
    pub created_at: DateTime<Utc>,
}

/// Comments for a branch
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BranchComments {
    pub comments: Vec<LineComment>,
}

/// Operations for managing review comments
pub struct ReviewOps;

impl ReviewOps {
    /// Load comments for a repo/branch
    pub fn load_comments(repo_id: &str, branch: &str) -> Result<BranchComments> {
        let path = get_comments_file(repo_id, branch)?;
        if !path.exists() {
            return Ok(BranchComments::default());
        }
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read comments file: {:?}", path))?;
        let comments: BranchComments = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse comments file: {:?}", path))?;
        Ok(comments)
    }

    /// Save comments for a repo/branch
    pub fn save_comments(repo_id: &str, branch: &str, comments: &BranchComments) -> Result<()> {
        let dir = get_review_dir(repo_id, branch)?;
        fs::create_dir_all(&dir)
            .with_context(|| format!("Failed to create review directory: {:?}", dir))?;
        let path = get_comments_file(repo_id, branch)?;
        let content = serde_json::to_string_pretty(comments)?;
        fs::write(&path, content)
            .with_context(|| format!("Failed to write comments file: {:?}", path))?;
        Ok(())
    }

    /// Create a new comment
    pub fn create_comment(
        repo_id: &str,
        branch: &str,
        file_path: &str,
        line_number: i32,
        line_type: CommentLineType,
        comment: &str,
    ) -> Result<LineComment> {
        let mut comments = Self::load_comments(repo_id, branch)?;

        let new_comment = LineComment {
            id: Uuid::new_v4().to_string(),
            file_path: file_path.to_string(),
            line_number,
            line_type,
            comment: comment.to_string(),
            created_at: Utc::now(),
        };

        comments.comments.push(new_comment.clone());
        Self::save_comments(repo_id, branch, &comments)?;

        Ok(new_comment)
    }

    /// Update an existing comment
    pub fn update_comment(
        repo_id: &str,
        branch: &str,
        comment_id: &str,
        new_comment: &str,
    ) -> Result<LineComment> {
        let mut comments = Self::load_comments(repo_id, branch)?;

        let comment = comments
            .comments
            .iter_mut()
            .find(|c| c.id == comment_id)
            .context("Comment not found")?;

        comment.comment = new_comment.to_string();
        let updated = comment.clone();

        Self::save_comments(repo_id, branch, &comments)?;

        Ok(updated)
    }

    /// Delete a comment
    pub fn delete_comment(repo_id: &str, branch: &str, comment_id: &str) -> Result<()> {
        let mut comments = Self::load_comments(repo_id, branch)?;

        let initial_len = comments.comments.len();
        comments.comments.retain(|c| c.id != comment_id);

        if comments.comments.len() == initial_len {
            anyhow::bail!("Comment not found");
        }

        Self::save_comments(repo_id, branch, &comments)?;

        Ok(())
    }

    /// List comments for a repo/branch, optionally filtered by file
    pub fn list_comments(
        repo_id: &str,
        branch: &str,
        file_path: Option<&str>,
    ) -> Result<Vec<LineComment>> {
        let comments = Self::load_comments(repo_id, branch)?;

        let filtered: Vec<LineComment> = match file_path {
            Some(path) => comments
                .comments
                .into_iter()
                .filter(|c| c.file_path == path)
                .collect(),
            None => comments.comments,
        };

        Ok(filtered)
    }

    /// Get comments grouped by file
    #[allow(dead_code)]
    pub fn get_comments_by_file(
        repo_id: &str,
        branch: &str,
    ) -> Result<HashMap<String, Vec<LineComment>>> {
        let comments = Self::load_comments(repo_id, branch)?;

        let mut by_file: HashMap<String, Vec<LineComment>> = HashMap::new();
        for comment in comments.comments {
            by_file
                .entry(comment.file_path.clone())
                .or_default()
                .push(comment);
        }

        Ok(by_file)
    }
}
