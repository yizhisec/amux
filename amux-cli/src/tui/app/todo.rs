//! TODO operations

use super::super::App;
use crate::error::TuiError;
use amux_proto::daemon::TodoItem;
use std::collections::HashMap;

type Result<T> = std::result::Result<T, TuiError>;

impl App {
    /// Load TODO items for current repository
    pub async fn load_todos(&mut self) -> Result<()> {
        let repo_id = self.current_repo().map(|r| r.info.id.clone());
        if let Some(repo_id) = repo_id {
            let show_completed = self.todo.show_completed;
            self.todo.items = self.client.list_todos(&repo_id, show_completed).await?;
            self.rebuild_todo_display_order();
        }
        Ok(())
    }

    /// Rebuild display order for TODO items (tree structure)
    pub fn rebuild_todo_display_order(&mut self) {
        // Build parent-to-children mapping
        let mut items_by_parent: HashMap<Option<String>, Vec<usize>> = HashMap::new();
        for (i, item) in self.todo.items.iter().enumerate() {
            items_by_parent
                .entry(item.parent_id.clone())
                .or_default()
                .push(i);
        }

        // Recursively build display order
        fn build_order(
            items: &[TodoItem],
            items_by_parent: &HashMap<Option<String>, Vec<usize>>,
            parent_id: Option<String>,
            order: &mut Vec<usize>,
        ) {
            if let Some(children) = items_by_parent.get(&parent_id) {
                let mut sorted_children = children.clone();
                sorted_children.sort_by_key(|&idx| items[idx].order);

                for &idx in &sorted_children {
                    order.push(idx);
                    let item = &items[idx];
                    build_order(items, items_by_parent, Some(item.id.clone()), order);
                }
            }
        }

        self.todo.display_order.clear();
        build_order(
            &self.todo.items,
            &items_by_parent,
            None,
            &mut self.todo.display_order,
        );
    }

    /// Create a new TODO item
    pub async fn create_todo(
        &mut self,
        title: String,
        description: Option<String>,
        parent_id: Option<String>,
    ) -> Result<()> {
        let repo_id = self.current_repo().map(|r| r.info.id.clone());
        if let Some(repo_id) = repo_id {
            self.client
                .create_todo(&repo_id, title, description, parent_id)
                .await?;
            self.load_todos().await?;
        }
        Ok(())
    }

    /// Toggle TODO completion status
    pub async fn toggle_todo(&mut self, todo_id: &str) -> Result<()> {
        self.client.toggle_todo(todo_id).await?;
        self.load_todos().await?;
        Ok(())
    }

    /// Delete a TODO item
    pub async fn delete_todo(&mut self, todo_id: &str) -> Result<()> {
        self.client.delete_todo(todo_id).await?;
        self.load_todos().await?;
        Ok(())
    }

    /// Update a TODO item
    pub async fn update_todo(
        &mut self,
        todo_id: &str,
        title: Option<String>,
        description: Option<String>,
    ) -> Result<()> {
        self.client
            .update_todo(todo_id, title, description, None, None)
            .await?;
        self.load_todos().await?;
        Ok(())
    }

    /// Reorder a TODO item
    pub async fn reorder_todo(
        &mut self,
        todo_id: &str,
        new_order: i32,
        new_parent_id: Option<String>,
    ) -> Result<()> {
        self.client
            .reorder_todo(todo_id, new_order, new_parent_id)
            .await?;
        self.load_todos().await?;
        Ok(())
    }
}
