//! TODO view rendering

use crate::tui::app::App;
use crate::tui::state::InputMode;
use amux_proto::daemon::TodoItem;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

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

    // Clear background
    let background = Block::default()
        .style(Style::default().bg(Color::Black))
        .borders(Borders::NONE);
    f.render_widget(background, area);

    // Draw popup
    let title = if app.todo.show_completed {
        " TODO List (All) - [c] to hide completed "
    } else {
        " TODO List (Active) - [c] to show completed "
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));
    f.render_widget(block, popup_area);

    // Inner area for content
    let inner = Rect {
        x: popup_area.x + 1,
        y: popup_area.y + 1,
        width: popup_area.width.saturating_sub(2),
        height: popup_area.height.saturating_sub(2),
    };

    // Split into list area and help area
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
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

    // Draw help text
    let help_text = "[j/k] Nav | [Space] Toggle | [a] Add | [A] Add child | [e] Edit | [E] Desc | [d] Delete | [q] Close";
    let help = Paragraph::new(help_text)
        .style(Style::default().fg(Color::DarkGray))
        .block(Block::default().borders(Borders::ALL).title(" Help "));
    f.render_widget(help, chunks[1]);
}

/// Draw add TODO overlay
pub fn draw_add_todo_overlay(f: &mut Frame, area: Rect, app: &App) {
    // Small centered input box
    let popup_width = 60;
    let popup_height = 5;
    let popup_x = (area.width - popup_width) / 2;
    let popup_y = (area.height - popup_height) / 2;
    let popup_area = Rect {
        x: area.x + popup_x,
        y: area.y + popup_y,
        width: popup_width,
        height: popup_height,
    };

    // Clear background
    let background = Block::default()
        .style(Style::default().bg(Color::Black))
        .borders(Borders::NONE);
    f.render_widget(background, area);

    // Draw input box
    let title = if matches!(app.input_mode, InputMode::AddTodo { parent_id: Some(_) }) {
        " Add Child TODO "
    } else {
        " Add TODO "
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    // Input text
    let input = Paragraph::new(app.text_input.content()).style(Style::default().fg(Color::White));
    f.render_widget(input, inner);

    // Cursor
    f.set_cursor_position((
        inner.x + app.text_input.cursor_display_offset() as u16,
        inner.y,
    ));
}

/// Draw edit TODO overlay
pub fn draw_edit_todo_overlay(f: &mut Frame, area: Rect, app: &App) {
    // Small centered input box
    let popup_width = 60;
    let popup_height = 5;
    let popup_x = (area.width - popup_width) / 2;
    let popup_y = (area.height - popup_height) / 2;
    let popup_area = Rect {
        x: area.x + popup_x,
        y: area.y + popup_y,
        width: popup_width,
        height: popup_height,
    };

    // Clear background
    let background = Block::default()
        .style(Style::default().bg(Color::Black))
        .borders(Borders::NONE);
    f.render_widget(background, area);

    // Draw input box
    let block = Block::default()
        .title(" Edit TODO Title ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    // Input text
    let input = Paragraph::new(app.text_input.content()).style(Style::default().fg(Color::White));
    f.render_widget(input, inner);

    // Cursor
    f.set_cursor_position((
        inner.x + app.text_input.cursor_display_offset() as u16,
        inner.y,
    ));
}

/// Draw edit TODO description overlay
pub fn draw_edit_todo_description_overlay(f: &mut Frame, area: Rect, app: &App) {
    // Small centered input box
    let popup_width = 60;
    let popup_height = 5;
    let popup_x = (area.width - popup_width) / 2;
    let popup_y = (area.height - popup_height) / 2;
    let popup_area = Rect {
        x: area.x + popup_x,
        y: area.y + popup_y,
        width: popup_width,
        height: popup_height,
    };

    // Clear background
    let background = Block::default()
        .style(Style::default().bg(Color::Black))
        .borders(Borders::NONE);
    f.render_widget(background, area);

    // Draw input box
    let block = Block::default()
        .title(" Edit TODO Description ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    // Input text
    let input = Paragraph::new(app.text_input.content()).style(Style::default().fg(Color::White));
    f.render_widget(input, inner);

    // Cursor
    f.set_cursor_position((
        inner.x + app.text_input.cursor_display_offset() as u16,
        inner.y,
    ));
}

/// Draw confirm delete TODO overlay
pub fn draw_confirm_delete_todo_overlay(f: &mut Frame, area: Rect, title: &str) {
    // Small centered confirmation box
    let popup_width = 60.min(area.width.saturating_sub(4));
    let popup_height = 7;
    let popup_x = (area.width - popup_width) / 2;
    let popup_y = (area.height - popup_height) / 2;
    let popup_area = Rect {
        x: area.x + popup_x,
        y: area.y + popup_y,
        width: popup_width,
        height: popup_height,
    };

    // Clear background
    let background = Block::default()
        .style(Style::default().bg(Color::Black))
        .borders(Borders::NONE);
    f.render_widget(background, area);

    // Draw confirmation box
    let block = Block::default()
        .title(" Confirm Delete ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red));

    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    // Confirmation message
    let message = vec![
        Line::from(""),
        Line::from(format!("Delete TODO: {}", title)).style(Style::default().fg(Color::White)),
        Line::from(""),
        Line::from("This will also delete all child TODOs.")
            .style(Style::default().fg(Color::Yellow)),
        Line::from(""),
        Line::from("[y] Yes    [n] No").style(Style::default().fg(Color::DarkGray)),
    ];

    let paragraph = Paragraph::new(message).alignment(ratatui::layout::Alignment::Center);
    f.render_widget(paragraph, inner);
}
