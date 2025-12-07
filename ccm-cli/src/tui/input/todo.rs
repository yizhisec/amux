//! TODO popup and input handling

use super::super::app::App;
use super::super::state::{AsyncAction, InputMode};
use crossterm::event::{KeyCode, KeyEvent};

/// Handle TODO popup mode (main TODO list view)
pub fn handle_todo_popup_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    match key.code {
        // Esc or q = close popup
        KeyCode::Esc | KeyCode::Char('q') => {
            app.input_mode = InputMode::Normal;
            app.todo_cursor = 0;
            app.todo_scroll_offset = 0;
            None
        }

        // j/Down = move down
        KeyCode::Char('j') | KeyCode::Down => {
            if app.todo_cursor < app.todo_display_order.len().saturating_sub(1) {
                app.todo_cursor += 1;
            }
            None
        }

        // k/Up = move up
        KeyCode::Char('k') | KeyCode::Up => {
            if app.todo_cursor > 0 {
                app.todo_cursor -= 1;
            }
            None
        }

        // g = go to top
        KeyCode::Char('g') => {
            app.todo_cursor = 0;
            None
        }

        // G = go to bottom
        KeyCode::Char('G') => {
            app.todo_cursor = app.todo_display_order.len().saturating_sub(1);
            None
        }

        // Space = toggle completion
        KeyCode::Char(' ') => {
            if let Some(&item_idx) = app.todo_display_order.get(app.todo_cursor) {
                if let Some(item) = app.todo_items.get(item_idx) {
                    return Some(AsyncAction::ToggleTodo {
                        todo_id: item.id.clone(),
                    });
                }
            }
            None
        }

        // a = add new TODO (top-level)
        KeyCode::Char('a') => {
            app.input_mode = InputMode::AddTodo { parent_id: None };
            app.input_buffer.clear();
            None
        }

        // A = add child TODO
        KeyCode::Char('A') => {
            if let Some(&item_idx) = app.todo_display_order.get(app.todo_cursor) {
                if let Some(item) = app.todo_items.get(item_idx) {
                    app.input_mode = InputMode::AddTodo {
                        parent_id: Some(item.id.clone()),
                    };
                    app.input_buffer.clear();
                }
            }
            None
        }

        // e = edit TODO title
        KeyCode::Char('e') => {
            if let Some(&item_idx) = app.todo_display_order.get(app.todo_cursor) {
                if let Some(item) = app.todo_items.get(item_idx) {
                    app.input_mode = InputMode::EditTodo {
                        todo_id: item.id.clone(),
                    };
                    app.input_buffer = item.title.clone();
                }
            }
            None
        }

        // E = edit TODO description
        KeyCode::Char('E') => {
            if let Some(&item_idx) = app.todo_display_order.get(app.todo_cursor) {
                if let Some(item) = app.todo_items.get(item_idx) {
                    app.input_mode = InputMode::EditTodoDescription {
                        todo_id: item.id.clone(),
                    };
                    app.input_buffer = item.description.clone().unwrap_or_default();
                }
            }
            None
        }

        // d = delete TODO
        KeyCode::Char('d') => {
            if let Some(&item_idx) = app.todo_display_order.get(app.todo_cursor) {
                if let Some(item) = app.todo_items.get(item_idx) {
                    app.input_mode = InputMode::ConfirmDeleteTodo {
                        todo_id: item.id.clone(),
                        title: item.title.clone(),
                    };
                }
            }
            None
        }

        // c = toggle show completed
        KeyCode::Char('c') => {
            app.todo_show_completed = !app.todo_show_completed;
            Some(AsyncAction::LoadTodos)
        }

        // h/l = expand/collapse (for future tree view)
        KeyCode::Char('h') => {
            if let Some(&item_idx) = app.todo_display_order.get(app.todo_cursor) {
                if let Some(item) = app.todo_items.get(item_idx) {
                    app.expanded_todos.remove(&item.id);
                }
            }
            None
        }

        KeyCode::Char('l') => {
            if let Some(&item_idx) = app.todo_display_order.get(app.todo_cursor) {
                if let Some(item) = app.todo_items.get(item_idx) {
                    app.expanded_todos.insert(item.id.clone());
                }
            }
            None
        }

        _ => None,
    }
}

/// Handle add TODO mode (entering new TODO title)
pub fn handle_add_todo_mode_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    match key.code {
        KeyCode::Esc => {
            app.input_mode = InputMode::TodoPopup;
            app.input_buffer.clear();
            None
        }

        KeyCode::Enter => {
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

        KeyCode::Char(c) => {
            app.input_buffer.push(c);
            None
        }

        KeyCode::Backspace => {
            app.input_buffer.pop();
            None
        }

        _ => None,
    }
}

/// Handle edit TODO mode (editing title)
pub fn handle_edit_todo_mode_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    match key.code {
        KeyCode::Esc => {
            app.input_mode = InputMode::TodoPopup;
            app.input_buffer.clear();
            None
        }

        KeyCode::Enter => {
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

        KeyCode::Char(c) => {
            app.input_buffer.push(c);
            None
        }

        KeyCode::Backspace => {
            app.input_buffer.pop();
            None
        }

        _ => None,
    }
}

/// Handle edit TODO description mode
pub fn handle_edit_todo_description_mode_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    match key.code {
        KeyCode::Esc => {
            app.input_mode = InputMode::TodoPopup;
            app.input_buffer.clear();
            None
        }

        KeyCode::Enter => {
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

        KeyCode::Char(c) => {
            app.input_buffer.push(c);
            None
        }

        KeyCode::Backspace => {
            app.input_buffer.pop();
            None
        }

        _ => None,
    }
}

/// Handle confirm delete TODO mode
pub fn handle_confirm_delete_todo_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    match key.code {
        KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
            app.input_mode = InputMode::TodoPopup;
            None
        }

        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
            if let InputMode::ConfirmDeleteTodo { todo_id, .. } =
                std::mem::replace(&mut app.input_mode, InputMode::TodoPopup)
            {
                return Some(AsyncAction::DeleteTodo { todo_id });
            }
            None
        }

        _ => None,
    }
}
