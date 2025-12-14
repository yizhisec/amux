//! TODO popup and input handling
//!
//! Uses common input handling utilities from utils module to reduce duplication.
//! Navigation uses the VirtualList trait for consistency across components.

use crate::tui::app::App;
use crate::tui::input::resolver;
use crate::tui::input::utils::{
    handle_confirmation_with_enter, handle_text_input, TextInputResult,
};
use crate::tui::state::{AsyncAction, InputMode};
use crate::tui::widgets::virtual_list::VirtualList;
use amux_config::Action;
use crossterm::event::{KeyCode, KeyEvent};

/// Handle TODO popup mode (main TODO list view)
pub fn handle_todo_popup_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    // Try to resolve the key to an action using the todo context
    if let Some(pattern_str) = resolver::key_event_to_pattern_string(key) {
        if let Some(action) = app
            .keybinds
            .resolve(&pattern_str, amux_config::BindingContext::Todo)
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
            app.restore_focus();
            None
        }

        _ => None,
    }
}

/// Execute a TODO popup action
fn execute_todo_action(app: &mut App, action: Action) -> Option<AsyncAction> {
    match action {
        Action::ClosePopup => {
            app.input_mode = InputMode::Normal;
            app.todo.cursor = 0;
            app.todo.scroll_offset = 0;
            app.restore_focus();
            None
        }

        Action::MoveDown => {
            app.todo.move_down();
            None
        }

        Action::MoveUp => {
            app.todo.move_up();
            None
        }

        Action::GotoTop => {
            app.todo.goto_top();
            None
        }

        Action::GotoBottom => {
            app.todo.goto_bottom();
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
            app.save_focus();
            app.input_mode = InputMode::AddTodo { parent_id: None };
            app.text_input.clear();
            None
        }

        Action::AddChildTodo => {
            if let Some(&item_idx) = app.todo.display_order.get(app.todo.cursor) {
                if let Some(item) = app.todo.items.get(item_idx).cloned() {
                    app.save_focus();
                    app.input_mode = InputMode::AddTodo {
                        parent_id: Some(item.id.clone()),
                    };
                    app.text_input.clear();
                }
            }
            None
        }

        Action::EditTodoTitle => {
            if let Some(&item_idx) = app.todo.display_order.get(app.todo.cursor) {
                if let Some(item) = app.todo.items.get(item_idx).cloned() {
                    app.save_focus();
                    app.input_mode = InputMode::EditTodo {
                        todo_id: item.id.clone(),
                    };
                    app.text_input.set_content(item.title.clone());
                }
            }
            None
        }

        Action::EditTodoDescription => {
            if let Some(&item_idx) = app.todo.display_order.get(app.todo.cursor) {
                if let Some(item) = app.todo.items.get(item_idx).cloned() {
                    app.save_focus();
                    app.input_mode = InputMode::EditTodoDescription {
                        todo_id: item.id.clone(),
                    };
                    app.text_input
                        .set_content(item.description.clone().unwrap_or_default());
                }
            }
            None
        }

        Action::DeleteTodo => {
            if let Some(&item_idx) = app.todo.display_order.get(app.todo.cursor) {
                if let Some(item) = app.todo.items.get(item_idx).cloned() {
                    app.save_focus();
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
    match handle_text_input(&key, &mut app.text_input) {
        TextInputResult::Cancel => {
            app.input_mode = InputMode::TodoPopup;
            app.text_input.clear();
            app.restore_focus();
            None
        }
        TextInputResult::Submit => {
            if app.text_input.is_empty() {
                app.input_mode = InputMode::TodoPopup;
                app.restore_focus();
                return None;
            }

            let title = app.text_input.content().to_string();
            app.text_input.clear();

            // Extract parent_id before changing mode
            if let InputMode::AddTodo { parent_id } =
                std::mem::replace(&mut app.input_mode, InputMode::TodoPopup)
            {
                app.restore_focus();
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
    match handle_text_input(&key, &mut app.text_input) {
        TextInputResult::Cancel => {
            app.input_mode = InputMode::TodoPopup;
            app.text_input.clear();
            app.restore_focus();
            None
        }
        TextInputResult::Submit => {
            if app.text_input.is_empty() {
                app.input_mode = InputMode::TodoPopup;
                app.restore_focus();
                return None;
            }

            let title = app.text_input.content().to_string();
            app.text_input.clear();

            if let InputMode::EditTodo { todo_id } =
                std::mem::replace(&mut app.input_mode, InputMode::TodoPopup)
            {
                app.restore_focus();
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
    match handle_text_input(&key, &mut app.text_input) {
        TextInputResult::Cancel => {
            app.input_mode = InputMode::TodoPopup;
            app.text_input.clear();
            app.restore_focus();
            None
        }
        TextInputResult::Submit => {
            let description = if app.text_input.is_empty() {
                None
            } else {
                Some(app.text_input.content().to_string())
            };
            app.text_input.clear();

            if let InputMode::EditTodoDescription { todo_id } =
                std::mem::replace(&mut app.input_mode, InputMode::TodoPopup)
            {
                app.restore_focus();
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
        |a| {
            a.input_mode = InputMode::TodoPopup;
            a.restore_focus();
        },
        todo_id
            .map(|id| AsyncAction::DeleteTodo { todo_id: id })
            .unwrap_or(AsyncAction::RefreshAll), // Fallback (shouldn't happen)
    )
}
