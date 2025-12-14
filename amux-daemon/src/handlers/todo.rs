//! TODO operations handlers

use crate::error::DaemonError;
use crate::state::SharedState;
use crate::todo::TodoOps;
use amux_proto::daemon::*;
use tonic::{Response, Status};

/// Helper to convert internal todo to proto TodoItem
fn to_proto_item(todo: crate::todo::TodoItem) -> TodoItem {
    TodoItem {
        id: todo.id,
        repo_id: todo.repo_id,
        title: todo.title,
        description: todo.description,
        completed: todo.completed,
        parent_id: todo.parent_id,
        order: todo.order,
        created_at: todo.created_at.timestamp(),
        updated_at: todo.updated_at.timestamp(),
    }
}

/// Helper to find repo_id for a todo item
async fn find_repo_id_for_todo(state: &SharedState, todo_id: &str) -> Option<String> {
    let state = state.read().await;
    for repo in state.repos.keys() {
        if let Ok(Some(item)) = TodoOps::find_todo(repo, todo_id) {
            return Some(item.repo_id);
        }
    }
    None
}

/// Create a new TODO item
pub async fn create_todo(req: CreateTodoRequest) -> Result<Response<TodoItem>, Status> {
    let todo = TodoOps::create_todo(&req.repo_id, req.title, req.description, req.parent_id)
        .map_err(|e| Status::from(DaemonError::from(e)))?;

    Ok(Response::new(to_proto_item(todo)))
}

/// Update a TODO item
pub async fn update_todo(
    state: &SharedState,
    req: UpdateTodoRequest,
) -> Result<Response<TodoItem>, Status> {
    let repo_id = find_repo_id_for_todo(state, &req.todo_id)
        .await
        .ok_or_else(|| Status::not_found("TODO item not found"))?;

    let todo = TodoOps::update_todo(
        &repo_id,
        &req.todo_id,
        req.title,
        req.description.map(Some),
        req.completed,
        req.order,
    )
    .map_err(|e| Status::from(DaemonError::from(e)))?;

    Ok(Response::new(to_proto_item(todo)))
}

/// Delete a TODO item
pub async fn delete_todo(
    state: &SharedState,
    req: DeleteTodoRequest,
) -> Result<Response<Empty>, Status> {
    let repo_id = find_repo_id_for_todo(state, &req.todo_id)
        .await
        .ok_or_else(|| Status::not_found("TODO item not found"))?;

    TodoOps::delete_todo(&repo_id, &req.todo_id).map_err(|e| Status::from(DaemonError::from(e)))?;

    Ok(Response::new(Empty {}))
}

/// List all TODO items
pub async fn list_todos(req: ListTodosRequest) -> Result<Response<ListTodosResponse>, Status> {
    let include_completed = req.include_completed.unwrap_or(true);
    let items = TodoOps::list_todos(&req.repo_id, include_completed)
        .map_err(|e| Status::from(DaemonError::from(e)))?;

    let proto_items: Vec<TodoItem> = items.into_iter().map(to_proto_item).collect();

    Ok(Response::new(ListTodosResponse { items: proto_items }))
}

/// Toggle a TODO item's completion status
pub async fn toggle_todo(
    state: &SharedState,
    req: ToggleTodoRequest,
) -> Result<Response<TodoItem>, Status> {
    let repo_id = find_repo_id_for_todo(state, &req.todo_id)
        .await
        .ok_or_else(|| Status::not_found("TODO item not found"))?;

    let todo = TodoOps::toggle_todo(&repo_id, &req.todo_id)
        .map_err(|e| Status::from(DaemonError::from(e)))?;

    Ok(Response::new(to_proto_item(todo)))
}

/// Reorder a TODO item
pub async fn reorder_todo(
    state: &SharedState,
    req: ReorderTodoRequest,
) -> Result<Response<TodoItem>, Status> {
    let repo_id = find_repo_id_for_todo(state, &req.todo_id)
        .await
        .ok_or_else(|| Status::not_found("TODO item not found"))?;

    let todo = TodoOps::reorder_todo(&repo_id, &req.todo_id, req.new_order, req.new_parent_id)
        .map_err(|e| Status::from(DaemonError::from(e)))?;

    Ok(Response::new(to_proto_item(todo)))
}
