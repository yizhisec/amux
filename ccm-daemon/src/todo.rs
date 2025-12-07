//! TODO persistence - save and restore TODO items for repositories

use crate::error::PersistenceError;
use crate::state::AppState;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::info;
use uuid::Uuid;

/// A TODO item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub id: String,
    pub repo_id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub completed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub order: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Repository TODO list
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RepoTodos {
    pub items: Vec<TodoItem>,
}

/// TODO operations
pub struct TodoOps;

impl TodoOps {
    /// Get todos directory (~/.ccm/todos/)
    pub fn todos_dir() -> PathBuf {
        AppState::data_dir().join("todos")
    }

    /// Get repo todos directory (~/.ccm/todos/{repo_id}/)
    pub fn repo_todos_dir(repo_id: &str) -> PathBuf {
        Self::todos_dir().join(repo_id)
    }

    /// Get todos file path
    pub fn todos_file(repo_id: &str) -> PathBuf {
        Self::repo_todos_dir(repo_id).join("todos.json")
    }

    /// Ensure todos directory exists
    pub fn ensure_todos_dir(repo_id: &str) -> Result<(), PersistenceError> {
        let dir = Self::repo_todos_dir(repo_id);
        if !dir.exists() {
            std::fs::create_dir_all(&dir).map_err(PersistenceError::CreateDir)?;
        }
        Ok(())
    }

    /// Load todos for a repository
    pub fn load_todos(repo_id: &str) -> Result<RepoTodos, PersistenceError> {
        let path = Self::todos_file(repo_id);
        if !path.exists() {
            return Ok(RepoTodos::default());
        }

        let content = std::fs::read_to_string(&path).map_err(|e| PersistenceError::ReadFile {
            path: path.clone(),
            source: e,
        })?;

        let todos: RepoTodos = serde_json::from_str(&content)?;
        Ok(todos)
    }

    /// Save todos for a repository
    pub fn save_todos(repo_id: &str, todos: &RepoTodos) -> Result<(), PersistenceError> {
        Self::ensure_todos_dir(repo_id)?;

        let path = Self::todos_file(repo_id);
        let content = serde_json::to_string_pretty(todos)?;
        std::fs::write(&path, &content).map_err(|e| PersistenceError::WriteFile {
            path: path.clone(),
            source: e,
        })?;

        Ok(())
    }

    /// Create a new TODO item
    pub fn create_todo(
        repo_id: &str,
        title: String,
        description: Option<String>,
        parent_id: Option<String>,
    ) -> Result<TodoItem, PersistenceError> {
        let mut todos = Self::load_todos(repo_id)?;

        // Calculate next order within parent scope
        let next_order = todos
            .items
            .iter()
            .filter(|item| item.parent_id == parent_id)
            .map(|item| item.order)
            .max()
            .unwrap_or(-1)
            + 1;

        let now = Utc::now();
        let todo = TodoItem {
            id: Uuid::new_v4().to_string(),
            repo_id: repo_id.to_string(),
            title,
            description,
            completed: false,
            parent_id,
            order: next_order,
            created_at: now,
            updated_at: now,
        };

        todos.items.push(todo.clone());
        Self::save_todos(repo_id, &todos)?;

        info!("Created TODO '{}' in repo {}", todo.title, repo_id);
        Ok(todo)
    }

    /// Update a TODO item
    pub fn update_todo(
        repo_id: &str,
        todo_id: &str,
        title: Option<String>,
        description: Option<Option<String>>,
        completed: Option<bool>,
        order: Option<i32>,
    ) -> Result<TodoItem, PersistenceError> {
        let mut todos = Self::load_todos(repo_id)?;

        let todo = todos
            .items
            .iter_mut()
            .find(|item| item.id == todo_id)
            .ok_or_else(|| PersistenceError::ReadFile {
                path: Self::todos_file(repo_id),
                source: std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("TODO not found: {}", todo_id),
                ),
            })?;

        if let Some(title) = title {
            todo.title = title;
        }
        if let Some(description) = description {
            todo.description = description;
        }
        if let Some(completed) = completed {
            todo.completed = completed;
        }
        if let Some(order) = order {
            todo.order = order;
        }
        todo.updated_at = Utc::now();

        let updated = todo.clone();
        Self::save_todos(repo_id, &todos)?;

        info!("Updated TODO '{}' in repo {}", updated.title, repo_id);
        Ok(updated)
    }

    /// Delete a TODO item
    pub fn delete_todo(repo_id: &str, todo_id: &str) -> Result<(), PersistenceError> {
        let mut todos = Self::load_todos(repo_id)?;

        // Find and collect all descendant IDs (recursive)
        let mut to_delete = vec![todo_id.to_string()];
        let mut i = 0;
        while i < to_delete.len() {
            let parent = to_delete[i].clone();
            for item in &todos.items {
                if item.parent_id.as_ref() == Some(&parent) && !to_delete.contains(&item.id) {
                    to_delete.push(item.id.clone());
                }
            }
            i += 1;
        }

        // Remove all items
        todos.items.retain(|item| !to_delete.contains(&item.id));
        Self::save_todos(repo_id, &todos)?;

        info!(
            "Deleted {} TODO item(s) from repo {}",
            to_delete.len(),
            repo_id
        );
        Ok(())
    }

    /// Toggle TODO completion status
    pub fn toggle_todo(repo_id: &str, todo_id: &str) -> Result<TodoItem, PersistenceError> {
        let mut todos = Self::load_todos(repo_id)?;

        let todo = todos
            .items
            .iter_mut()
            .find(|item| item.id == todo_id)
            .ok_or_else(|| PersistenceError::ReadFile {
                path: Self::todos_file(repo_id),
                source: std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("TODO not found: {}", todo_id),
                ),
            })?;

        todo.completed = !todo.completed;
        todo.updated_at = Utc::now();

        let updated = todo.clone();
        Self::save_todos(repo_id, &todos)?;

        info!(
            "Toggled TODO '{}' to {} in repo {}",
            updated.title, updated.completed, repo_id
        );
        Ok(updated)
    }

    /// Reorder a TODO item
    pub fn reorder_todo(
        repo_id: &str,
        todo_id: &str,
        new_order: i32,
        new_parent_id: Option<String>,
    ) -> Result<TodoItem, PersistenceError> {
        let mut todos = Self::load_todos(repo_id)?;

        // Find the item to move
        let (old_parent_id, old_order) = {
            let todo = todos
                .items
                .iter()
                .find(|item| item.id == todo_id)
                .ok_or_else(|| PersistenceError::ReadFile {
                    path: Self::todos_file(repo_id),
                    source: std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        format!("TODO not found: {}", todo_id),
                    ),
                })?;
            (todo.parent_id.clone(), todo.order)
        };

        // Update orders for siblings in old parent
        if old_parent_id == new_parent_id {
            // Same parent - reordering within siblings
            for item in todos.items.iter_mut() {
                if item.parent_id == old_parent_id && item.id != todo_id {
                    if old_order < new_order {
                        // Moving down: shift items between old and new position up
                        if item.order > old_order && item.order <= new_order {
                            item.order -= 1;
                        }
                    } else {
                        // Moving up: shift items between new and old position down
                        if item.order >= new_order && item.order < old_order {
                            item.order += 1;
                        }
                    }
                }
            }
        } else {
            // Different parent - moving to different parent
            // Adjust orders in old parent
            for item in todos.items.iter_mut() {
                if item.parent_id == old_parent_id && item.order > old_order {
                    item.order -= 1;
                }
            }
            // Adjust orders in new parent
            for item in todos.items.iter_mut() {
                if item.parent_id == new_parent_id && item.order >= new_order {
                    item.order += 1;
                }
            }
        }

        // Update the moved item
        let todo = todos
            .items
            .iter_mut()
            .find(|item| item.id == todo_id)
            .unwrap();
        todo.parent_id = new_parent_id;
        todo.order = new_order;
        todo.updated_at = Utc::now();

        let updated = todo.clone();
        Self::save_todos(repo_id, &todos)?;

        info!("Reordered TODO '{}' in repo {}", updated.title, repo_id);
        Ok(updated)
    }

    /// List todos for a repository
    pub fn list_todos(
        repo_id: &str,
        include_completed: bool,
    ) -> Result<Vec<TodoItem>, PersistenceError> {
        let todos = Self::load_todos(repo_id)?;

        let items: Vec<TodoItem> = if include_completed {
            todos.items
        } else {
            todos
                .items
                .into_iter()
                .filter(|item| !item.completed)
                .collect()
        };

        Ok(items)
    }

    /// Find a specific TODO item
    pub fn find_todo(repo_id: &str, todo_id: &str) -> Result<Option<TodoItem>, PersistenceError> {
        let todos = Self::load_todos(repo_id)?;
        Ok(todos.items.into_iter().find(|item| item.id == todo_id))
    }
}
