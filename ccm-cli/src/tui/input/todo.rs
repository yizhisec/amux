//! TODO popup and input handling
//!
//! Uses common input handling utilities from utils module to reduce duplication.

use super::super::app::App;
use super::super::state::{AsyncAction, InputMode};
use super::resolver;
use super::utils::{handle_confirmation_with_enter, handle_text_input, TextInputResult};
use ccm_config::Action;
use crossterm::event::{KeyCode, KeyEvent};

/// Handle TODO popup mode (main TODO list view)
pub fn handle_todo_popup_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    // Try to resolve the key to an action using the todo context
    if let Some(pattern_str) = resolver::key_event_to_pattern_string(key) {
        if let Some(action) = app
            .keybinds
            .resolve(&pattern_str, ccm_config::BindingContext::Todo)
        {
            return execute_todo_action(app, action);
        }
    }

    // Fallback for keys not in keybinds
    match key.code {
        // Esc or q = close popup
        KeyCode::Esc | KeyCode::Char('q') => {
            app.input_mode = InputMode::Normal;
            app.todo.cursor = 0;
            app.todo.scroll_offset = 0;
            None
        }

        _ => None,
    }
}

/// Execute a TODO popup action
fn execute_todo_action(app: &mut App, action: Action) -> Option<AsyncAction> {
    match action {
        Action::MoveDown => {
            if app.todo.cursor < app.todo.display_order.len().saturating_sub(1) {
                app.todo.cursor += 1;
            }
            None
        }

        Action::MoveUp => {
            if app.todo.cursor > 0 {
                app.todo.cursor -= 1;
            }
            None
        }

        Action::GotoTop => {
            app.todo.cursor = 0;
            None
        }

        Action::GotoBottom => {
            app.todo.cursor = app.todo.display_order.len().saturating_sub(1);
            None
        }

        Action::ToggleTodoComplete => {
            if let Some(&item_idx) = app.todo.display_order.get(app.todo.cursor) {
                if let Some(item) = app.todo.items.get(item_idx) {
                    return Some(AsyncAction::ToggleTodo {
                        todo_id: item.id.clone(),
                    });
                }
            }
            None
        }

        Action::AddTodo => {
            app.input_mode = InputMode::AddTodo { parent_id: None };
            app.input_buffer.clear();
            None
        }

        Action::AddChildTodo => {
            if let Some(&item_idx) = app.todo.display_order.get(app.todo.cursor) {
                if let Some(item) = app.todo.items.get(item_idx) {
                    app.input_mode = InputMode::AddTodo {
                        parent_id: Some(item.id.clone()),
                    };
                    app.input_buffer.clear();
                }
            }
            None
        }

        Action::EditTodoTitle => {
            if let Some(&item_idx) = app.todo.display_order.get(app.todo.cursor) {
                if let Some(item) = app.todo.items.get(item_idx) {
                    app.input_mode = InputMode::EditTodo {
                        todo_id: item.id.clone(),
                    };
                    app.input_buffer = item.title.clone();
                }
            }
            None
        }

        Action::EditTodoDescription => {
            if let Some(&item_idx) = app.todo.display_order.get(app.todo.cursor) {
                if let Some(item) = app.todo.items.get(item_idx) {
                    app.input_mode = InputMode::EditTodoDescription {
                        todo_id: item.id.clone(),
                    };
                    app.input_buffer = item.description.clone().unwrap_or_default();
                }
            }
            None
        }

        Action::DeleteTodo => {
            if let Some(&item_idx) = app.todo.display_order.get(app.todo.cursor) {
                if let Some(item) = app.todo.items.get(item_idx) {
                    app.input_mode = InputMode::ConfirmDeleteTodo {
                        todo_id: item.id.clone(),
                        title: item.title.clone(),
                    };
                }
            }
            None
        }

        Action::ToggleShowCompleted => {
            app.todo.show_completed = !app.todo.show_completed;
            Some(AsyncAction::LoadTodos)
        }

        // Unhandled or context-inappropriate actions
        _ => None,
    }
}

/// Handle add TODO mode (entering new TODO title)
pub fn handle_add_todo_mode_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    match handle_text_input(&key, &mut app.input_buffer) {
        TextInputResult::Cancel => {
            app.input_mode = InputMode::TodoPopup;
            app.input_buffer.clear();
            None
        }
        TextInputResult::Submit => {
            if app.input_buffer.is_empty() {
                app.input_mode = InputMode::TodoPopup;
                return None;
            }

            let title = app.input_buffer.clone();
            app.input_buffer.clear();

            // Extract parent_id before changing mode
            if let InputMode::AddTodo { parent_id } =
                std::mem::replace(&mut app.input_mode, InputMode::TodoPopup)
            {
                return Some(AsyncAction::CreateTodo {
                    title,
                    description: None,
                    parent_id,
                });
            }
            None
        }
        TextInputResult::Handled | TextInputResult::Unhandled => None,
    }
}

/// Handle edit TODO mode (editing title)
pub fn handle_edit_todo_mode_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    match handle_text_input(&key, &mut app.input_buffer) {
        TextInputResult::Cancel => {
            app.input_mode = InputMode::TodoPopup;
            app.input_buffer.clear();
            None
        }
        TextInputResult::Submit => {
            if app.input_buffer.is_empty() {
                app.input_mode = InputMode::TodoPopup;
                return None;
            }

            let title = app.input_buffer.clone();
            app.input_buffer.clear();

            if let InputMode::EditTodo { todo_id } =
                std::mem::replace(&mut app.input_mode, InputMode::TodoPopup)
            {
                return Some(AsyncAction::UpdateTodo {
                    todo_id,
                    title: Some(title),
                    description: None,
                });
            }
            None
        }
        TextInputResult::Handled | TextInputResult::Unhandled => None,
    }
}

/// Handle edit TODO description mode
pub fn handle_edit_todo_description_mode_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    match handle_text_input(&key, &mut app.input_buffer) {
        TextInputResult::Cancel => {
            app.input_mode = InputMode::TodoPopup;
            app.input_buffer.clear();
            None
        }
        TextInputResult::Submit => {
            let description = if app.input_buffer.is_empty() {
                None
            } else {
                Some(app.input_buffer.clone())
            };
            app.input_buffer.clear();

            if let InputMode::EditTodoDescription { todo_id } =
                std::mem::replace(&mut app.input_mode, InputMode::TodoPopup)
            {
                return Some(AsyncAction::UpdateTodo {
                    todo_id,
                    title: None,
                    description,
                });
            }
            None
        }
        TextInputResult::Handled | TextInputResult::Unhandled => None,
    }
}

/// Handle confirm delete TODO mode
pub fn handle_confirm_delete_todo_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    // Extract todo_id first to avoid borrow issues
    let todo_id = if let InputMode::ConfirmDeleteTodo { ref todo_id, .. } = app.input_mode {
        Some(todo_id.clone())
    } else {
        None
    };

    handle_confirmation_with_enter(
        app,
        &key,
        |a| a.input_mode = InputMode::TodoPopup,
        todo_id
            .map(|id| AsyncAction::DeleteTodo { todo_id: id })
            .unwrap_or(AsyncAction::RefreshAll), // Fallback (shouldn't happen)
    )
}
