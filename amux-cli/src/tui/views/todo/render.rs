//! TODO view rendering

use crate::tui::app::App;
use crate::tui::state::InputMode;
use amux_config::actions::Action;
use amux_config::keybind::BindingContext;
use amux_proto::daemon::TodoItem;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, BorderType, Borders, List, ListItem, Paragraph},
    Frame,
};

/// Helper to format key binding for display
fn key(app: &App, action: Action) -> String {
    app.keybinds.key_display(action, BindingContext::Todo)
}

/// Calculate depth for TODO item in tree structure
fn calculate_depth(items: &[TodoItem], item_idx: usize) -> usize {
    let item = &items[item_idx];
    if let Some(ref parent_id) = item.parent_id {
        if let Some(parent_idx) = items.iter().position(|i| &i.id == parent_id) {
            return 1 + calculate_depth(items, parent_idx);
        }
    }
    0
}

/// Check if we're in a TODO input mode
fn is_todo_input_mode(app: &App) -> bool {
    matches!(
        app.input_mode,
        InputMode::AddTodo { .. }
            | InputMode::EditTodo { .. }
            | InputMode::EditTodoDescription { .. }
    )
}

/// Get the input mode title
fn get_input_title(app: &App) -> &'static str {
    match app.input_mode {
        InputMode::AddTodo { parent_id: Some(_) } => "Add Child TODO:",
        InputMode::AddTodo { parent_id: None } => "Add TODO:",
        InputMode::EditTodo { .. } => "Edit Title:",
        InputMode::EditTodoDescription { .. } => "Edit Description:",
        _ => "",
    }
}

/// Draw TODO popup (main TODO list)
pub fn draw_todo_popup(f: &mut Frame, area: Rect, app: &App) {
    // Create centered popup (70% width, 80% height)
    let popup_width = (area.width * 70) / 100;
    let popup_height = (area.height * 80) / 100;
    let popup_x = (area.width - popup_width) / 2;
    let popup_y = (area.height - popup_height) / 2;
    let popup_area = Rect {
        x: area.x + popup_x,
        y: area.y + popup_y,
        width: popup_width,
        height: popup_height,
    };

    // Draw popup with background (only covers popup area, not entire screen)
    let title = if app.todo.show_completed {
        " TODO List (All) - [c] to hide completed "
    } else {
        " TODO List (Active) - [c] to show completed "
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Yellow))
        .style(Style::default().bg(Color::Black));
    f.render_widget(block, popup_area);

    // Inner area for content
    let inner = Rect {
        x: popup_area.x + 1,
        y: popup_area.y + 1,
        width: popup_area.width.saturating_sub(2),
        height: popup_area.height.saturating_sub(2),
    };

    // Determine if we need an input area
    let in_input_mode = is_todo_input_mode(app);

    // Split into list area and input/help area
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),    // TODO list
            Constraint::Length(3), // Input area or help text
        ])
        .split(inner);

    // Draw TODO list with tree structure using pre-computed display order
    let items: Vec<ListItem> = app
        .todo
        .display_order
        .iter()
        .enumerate()
        .map(|(display_idx, &item_idx)| {
            let item = &app.todo.items[item_idx];
            let checkbox = if item.completed { "[x]" } else { "[ ]" };
            let depth = calculate_depth(&app.todo.items, item_idx);
            let indent = "  ".repeat(depth);

            let style = if display_idx == app.todo.cursor {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else if item.completed {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(Color::White)
            };

            let text = if let Some(desc) = &item.description {
                if desc.is_empty() {
                    format!("{}{} {}", indent, checkbox, item.title)
                } else {
                    format!("{}{} {} ({})", indent, checkbox, item.title, desc)
                }
            } else {
                format!("{}{} {}", indent, checkbox, item.title)
            };

            ListItem::new(text).style(style)
        })
        .collect();

    let list = List::new(items).block(Block::default());
    f.render_widget(list, chunks[0]);

    // Draw input area or help text
    if in_input_mode {
        // Draw input box
        let input_title = get_input_title(app);
        let input_block = Block::default()
            .title(input_title)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan));

        let input_inner = input_block.inner(chunks[1]);
        f.render_widget(input_block, chunks[1]);

        // Input text
        let input =
            Paragraph::new(app.text_input.content()).style(Style::default().fg(Color::Yellow));
        f.render_widget(input, input_inner);

        // Cursor
        f.set_cursor_position((
            input_inner.x + app.text_input.cursor_display_offset() as u16,
            input_inner.y,
        ));
    } else {
        // Draw help text
        let help_text = format!(
            "{} Nav | {} Toggle | {} Add | {} Add child | {} Edit | {} Desc | {} Delete | {} Close",
            format!(
                "{}/{}",
                key(app, Action::MoveUp),
                key(app, Action::MoveDown)
            )
            .replace("[]", ""),
            key(app, Action::ToggleTodoComplete),
            key(app, Action::AddTodo),
            key(app, Action::AddChildTodo),
            key(app, Action::EditTodoTitle),
            key(app, Action::EditTodoDescription),
            key(app, Action::DeleteTodo),
            key(app, Action::ClosePopup),
        );
        let help = Paragraph::new(help_text)
            .style(Style::default().fg(Color::DarkGray))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title(" Help "),
            );
        f.render_widget(help, chunks[1]);
    }
}

/// Draw add TODO overlay - now just redirects to main popup with input mode
pub fn draw_add_todo_overlay(f: &mut Frame, area: Rect, app: &App) {
    // This is now handled within draw_todo_popup when in AddTodo input mode
    draw_todo_popup(f, area, app);
}

/// Draw edit TODO overlay - now just redirects to main popup with input mode
pub fn draw_edit_todo_overlay(f: &mut Frame, area: Rect, app: &App) {
    // This is now handled within draw_todo_popup when in EditTodo input mode
    draw_todo_popup(f, area, app);
}

/// Draw edit description overlay - now just redirects to main popup with input mode
pub fn draw_edit_description_overlay(f: &mut Frame, area: Rect, app: &App) {
    // This is now handled within draw_todo_popup when in EditTodoDescription input mode
    draw_todo_popup(f, area, app);
}

/// Draw confirm delete TODO overlay
pub fn draw_confirm_delete_todo_overlay(f: &mut Frame, area: Rect, title: &str) {
    let popup_width = 50.min(area.width.saturating_sub(4));
    let popup_height = 5;
    let x = (area.width.saturating_sub(popup_width)) / 2 + area.x;
    let y = (area.height.saturating_sub(popup_height)) / 2 + area.y;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    let text = format!(
        "Delete \"{}\"?\n\n[y] Yes  [n] No",
        if title.len() > 30 {
            format!("{}...", &title[..27])
        } else {
            title.to_string()
        }
    );

    let confirm = Paragraph::new(text)
        .style(Style::default().bg(Color::Black))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Red).bg(Color::Black))
                .style(Style::default().bg(Color::Black))
                .title(" Confirm Delete "),
        );

    f.render_widget(confirm, popup_area);
}
