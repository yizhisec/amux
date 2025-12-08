//! Overlay and dialog rendering (popups, confirmations, inputs)

use super::super::app::App;
use super::super::state::{DeleteTarget, ExitCleanupAction, InputMode};
use ccm_proto::daemon::TodoItem;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

/// Draw input overlay for new branch
pub fn draw_input_overlay(f: &mut Frame, area: Rect, app: &App) {
    // Center the input box
    let popup_width = 60.min(area.width.saturating_sub(4));
    let popup_height = 5;
    let x = (area.width.saturating_sub(popup_width)) / 2 + area.x;
    let y = (area.height.saturating_sub(popup_height)) / 2 + area.y;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    // Draw input box with background to cover underlying content
    let input = Paragraph::new(app.input_buffer.as_str())
        .style(Style::default().fg(Color::Yellow).bg(Color::Black))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan).bg(Color::Black))
                .style(Style::default().bg(Color::Black))
                .title(" New Branch (Enter=create, Esc=cancel) "),
        );
    f.render_widget(input, popup_area);

    // Show cursor
    f.set_cursor_position((
        popup_area.x + app.input_buffer.len() as u16 + 1,
        popup_area.y + 1,
    ));
}

/// Draw rename session overlay
pub fn draw_rename_session_overlay(f: &mut Frame, area: Rect, app: &App) {
    // Center the input box
    let popup_width = 60.min(area.width.saturating_sub(4));
    let popup_height = 5;
    let x = (area.width.saturating_sub(popup_width)) / 2 + area.x;
    let y = (area.height.saturating_sub(popup_height)) / 2 + area.y;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    // Draw input box with background to cover underlying content
    let input = Paragraph::new(app.input_buffer.as_str())
        .style(Style::default().fg(Color::Yellow).bg(Color::Black))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Green).bg(Color::Black))
                .style(Style::default().bg(Color::Black))
                .title(" Rename Session (Enter=save, Esc=cancel) "),
        );
    f.render_widget(input, popup_area);

    // Show cursor
    f.set_cursor_position((
        popup_area.x + app.input_buffer.len() as u16 + 1,
        popup_area.y + 1,
    ));
}

/// Draw confirm delete overlay
pub fn draw_confirm_delete_overlay(f: &mut Frame, area: Rect, app: &App, target: &DeleteTarget) {
    let (title, lines) = match target {
        DeleteTarget::Session { name, .. } => {
            // Session deletion: show two options (Destroy/Stop)
            let mut lines = vec![
                Line::from(format!("Delete session '{}'?", name)),
                Line::from(""),
            ];

            // Option 1: Destroy
            let destroy_indicator = if app.session_delete_action == ExitCleanupAction::Destroy {
                "▸ "
            } else {
                "  "
            };
            lines.push(Line::from(vec![
                Span::raw(destroy_indicator),
                Span::styled(
                    "[d] Destroy",
                    if app.session_delete_action == ExitCleanupAction::Destroy {
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::Red)
                    },
                ),
                Span::raw(" (delete all data)"),
            ]));

            // Option 2: Stop
            let stop_indicator = if app.session_delete_action == ExitCleanupAction::Stop {
                "▸ "
            } else {
                "  "
            };
            lines.push(Line::from(vec![
                Span::raw(stop_indicator),
                Span::styled(
                    "[s] Stop",
                    if app.session_delete_action == ExitCleanupAction::Stop {
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::Blue)
                    },
                ),
                Span::raw(" (stop PTY, keep metadata)"),
            ]));

            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("[Enter]", Style::default().fg(Color::Green)),
                Span::raw(" Confirm  "),
                Span::styled("[Esc/n]", Style::default().fg(Color::Red)),
                Span::raw(" Cancel"),
            ]));

            (" Delete Session ", lines)
        }
        DeleteTarget::Worktree { branch, .. } => {
            // Worktree deletion: simple Yes/No
            let lines = vec![
                Line::from(format!("Delete worktree '{}'?", branch)),
                Line::from(""),
                Line::from(vec![
                    Span::styled("[y/Enter]", Style::default().fg(Color::Green)),
                    Span::raw(" Yes  "),
                    Span::styled("[n/Esc]", Style::default().fg(Color::Red)),
                    Span::raw(" No"),
                ]),
            ];
            (" Delete Worktree ", lines)
        }
    };

    // Center dialog
    let popup_width = 60.min(area.width.saturating_sub(4));
    let popup_height = (lines.len() as u16 + 2).min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(popup_width)) / 2 + area.x;
    let y = (area.height.saturating_sub(popup_height)) / 2 + area.y;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    let confirm = Paragraph::new(lines)
        .style(Style::default().bg(Color::Black))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow).bg(Color::Black))
                .style(Style::default().bg(Color::Black))
                .title(title),
        );

    f.render_widget(confirm, popup_area);
}

/// Draw add worktree overlay (select branch or type new name)
pub fn draw_add_worktree_overlay(f: &mut Frame, area: Rect, app: &App, base_branch: Option<&str>) {
    // Calculate popup size based on content
    let branch_count = app.available_branches.len();
    let popup_height = (branch_count + 7).min(20) as u16; // +7 for borders, title, input, instructions, base info
    let popup_width = 60.min(area.width.saturating_sub(4));
    let x = (area.width.saturating_sub(popup_width)) / 2 + area.x;
    let y = (area.height.saturating_sub(popup_height)) / 2 + area.y;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    // Split popup into sections
    let inner = popup_area.inner(ratatui::layout::Margin::new(1, 1));
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Instructions
            Constraint::Length(1), // Base branch info
            Constraint::Min(1),    // Branch list
            Constraint::Length(1), // Spacer
            Constraint::Length(1), // Input field
        ])
        .split(inner);

    // Draw border with background to cover underlying content
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan).bg(Color::Black))
        .style(Style::default().bg(Color::Black))
        .title(" Add Worktree (j/k=select, Enter=add, Esc=cancel) ");
    f.render_widget(block, popup_area);

    // Instructions
    let instructions = Paragraph::new("Select existing branch or type new name:")
        .style(Style::default().fg(Color::DarkGray).bg(Color::Black));
    f.render_widget(instructions, chunks[0]);

    // Base branch info (only shown when typing new branch name)
    let base_info = match base_branch {
        Some(branch) => format!("Base: {} (new branch will be created from here)", branch),
        None => "Base: HEAD (new branch will be created from HEAD)".to_string(),
    };
    let base_style = if !app.input_buffer.is_empty() {
        Style::default().fg(Color::Green).bg(Color::Black)
    } else {
        Style::default().fg(Color::DarkGray).bg(Color::Black)
    };
    let base_paragraph = Paragraph::new(base_info).style(base_style);
    f.render_widget(base_paragraph, chunks[1]);

    // Branch list
    if !app.available_branches.is_empty() {
        let items: Vec<ListItem> = app
            .available_branches
            .iter()
            .enumerate()
            .map(|(i, branch)| {
                let is_selected = i == app.add_worktree_idx && app.input_buffer.is_empty();
                let style = if is_selected {
                    Style::default()
                        .fg(Color::Yellow)
                        .bg(Color::Black)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White).bg(Color::Black)
                };
                let prefix = if is_selected { "> " } else { "  " };
                ListItem::new(format!("{}○ {}", prefix, branch.branch)).style(style)
            })
            .collect();
        let list = List::new(items).style(Style::default().bg(Color::Black));
        f.render_widget(list, chunks[2]);
    } else {
        let empty = Paragraph::new("No available branches without worktree")
            .style(Style::default().fg(Color::DarkGray).bg(Color::Black));
        f.render_widget(empty, chunks[2]);
    }

    // Input field
    let input_style = if !app.input_buffer.is_empty() {
        Style::default().fg(Color::Yellow).bg(Color::Black)
    } else {
        Style::default().fg(Color::DarkGray).bg(Color::Black)
    };
    let input_text = if app.input_buffer.is_empty() {
        "New branch: (type to create new)"
    } else {
        &app.input_buffer
    };
    let prefix = if !app.input_buffer.is_empty() {
        "> "
    } else {
        "  "
    };
    let input = Paragraph::new(format!("{}New: {}", prefix, input_text)).style(input_style);
    f.render_widget(input, chunks[4]);

    // Show cursor if typing
    if !app.input_buffer.is_empty() {
        f.set_cursor_position((
            chunks[4].x + 7 + app.input_buffer.len() as u16, // 7 = "> New: ".len()
            chunks[4].y,
        ));
    }
}

/// Draw confirm delete branch overlay (after worktree deletion)
pub fn draw_confirm_delete_branch_overlay(f: &mut Frame, area: Rect, branch: &str) {
    // Center the confirm box
    let popup_width = 55.min(area.width.saturating_sub(4));
    let popup_height = 6;
    let x = (area.width.saturating_sub(popup_width)) / 2 + area.x;
    let y = (area.height.saturating_sub(popup_height)) / 2 + area.y;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    let text = vec![
        Line::from(format!("Worktree deleted. Delete branch '{}'?", branch)),
        Line::from(""),
        Line::from(vec![
            Span::styled("[y]", Style::default().fg(Color::Red)),
            Span::raw(" Yes, delete branch  "),
            Span::styled("[n/Esc]", Style::default().fg(Color::Green)),
            Span::raw(" No, keep branch"),
        ]),
    ];

    let confirm = Paragraph::new(text)
        .alignment(ratatui::layout::Alignment::Center)
        .style(Style::default().bg(Color::Black))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow).bg(Color::Black))
                .style(Style::default().bg(Color::Black))
                .title(" Delete Branch? "),
        );
    f.render_widget(confirm, popup_area);
}

/// Draw confirm delete worktree sessions overlay
pub fn draw_confirm_delete_worktree_sessions_overlay(
    f: &mut Frame,
    area: Rect,
    branch: &str,
    session_count: i32,
) {
    // Center the confirm box
    let popup_width = 60.min(area.width.saturating_sub(4));
    let popup_height = 7;
    let x = (area.width.saturating_sub(popup_width)) / 2 + area.x;
    let y = (area.height.saturating_sub(popup_height)) / 2 + area.y;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    let session_word = if session_count == 1 {
        "session"
    } else {
        "sessions"
    };
    let text = vec![
        Line::from(format!(
            "Worktree '{}' has {} active {}.",
            branch, session_count, session_word
        )),
        Line::from("Delete sessions first to remove worktree?"),
        Line::from(""),
        Line::from(vec![
            Span::styled("[y]", Style::default().fg(Color::Red)),
            Span::raw(" Yes, delete sessions  "),
            Span::styled("[n/Esc]", Style::default().fg(Color::Green)),
            Span::raw(" Cancel"),
        ]),
    ];

    let confirm = Paragraph::new(text)
        .alignment(ratatui::layout::Alignment::Center)
        .style(Style::default().bg(Color::Black))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow).bg(Color::Black))
                .style(Style::default().bg(Color::Black))
                .title(" Delete Sessions? "),
        );
    f.render_widget(confirm, popup_area);
}

/// Draw add line comment overlay
pub fn draw_add_line_comment_overlay(
    f: &mut Frame,
    area: Rect,
    app: &App,
    file_path: &str,
    line_number: i32,
) {
    // Calculate input lines for dynamic height
    let input_lines: Vec<&str> = app.input_buffer.lines().collect();
    let input_line_count = input_lines.len().max(1);

    // Center the input box with dynamic height
    let popup_width = 70.min(area.width.saturating_sub(4));
    let popup_height = (6 + input_line_count as u16).min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(popup_width)) / 2 + area.x;
    let y = (area.height.saturating_sub(popup_height)) / 2 + area.y;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    // Truncate file path if too long
    let max_path_len = (popup_width as usize).saturating_sub(20);
    let display_path = if file_path.len() > max_path_len {
        format!("...{}", &file_path[file_path.len() - max_path_len + 3..])
    } else {
        file_path.to_string()
    };

    // Build text with multiline input support
    let mut text = vec![
        Line::from(vec![
            Span::styled("File: ", Style::default().fg(Color::DarkGray)),
            Span::styled(display_path, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("Line: ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{}", line_number), Style::default().fg(Color::Cyan)),
        ]),
        Line::from(""),
    ];

    // Add input lines
    for (i, line) in input_lines.iter().enumerate() {
        let prefix = if i == 0 { "> " } else { "  " };
        text.push(Line::from(vec![
            Span::styled(prefix, Style::default().fg(Color::Yellow)),
            Span::styled(*line, Style::default().fg(Color::Yellow)),
        ]));
    }
    // Handle empty input
    if input_lines.is_empty() {
        text.push(Line::from(vec![Span::styled(
            "> ",
            Style::default().fg(Color::Yellow),
        )]));
    }

    let input = Paragraph::new(text)
        .style(Style::default().bg(Color::Black))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan).bg(Color::Black))
                .style(Style::default().bg(Color::Black))
                .title(" Add Comment (Enter=save, Shift+Enter=newline, Esc=cancel) "),
        );
    f.render_widget(input, popup_area);

    // Calculate cursor position for multiline input
    let last_line = input_lines.last().copied().unwrap_or("");
    let cursor_x = popup_area.x + 3 + last_line.len() as u16; // 3 = "> " + border
    let cursor_y = popup_area.y + 4 + (input_line_count.saturating_sub(1)) as u16;
    f.set_cursor_position((cursor_x, cursor_y));
}

/// Draw edit line comment overlay
pub fn draw_edit_line_comment_overlay(
    f: &mut Frame,
    area: Rect,
    app: &App,
    file_path: &str,
    line_number: i32,
) {
    // Calculate input lines for dynamic height
    let input_lines: Vec<&str> = app.input_buffer.lines().collect();
    let input_line_count = input_lines.len().max(1);

    // Center the input box with dynamic height
    let popup_width = 70.min(area.width.saturating_sub(4));
    let popup_height = (6 + input_line_count as u16).min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(popup_width)) / 2 + area.x;
    let y = (area.height.saturating_sub(popup_height)) / 2 + area.y;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    // Truncate file path if too long
    let max_path_len = (popup_width as usize).saturating_sub(20);
    let display_path = if file_path.len() > max_path_len {
        format!("...{}", &file_path[file_path.len() - max_path_len + 3..])
    } else {
        file_path.to_string()
    };

    // Build text with multiline input support
    let mut text = vec![
        Line::from(vec![
            Span::styled("File: ", Style::default().fg(Color::DarkGray)),
            Span::styled(display_path, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("Line: ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{}", line_number), Style::default().fg(Color::Cyan)),
        ]),
        Line::from(""),
    ];

    // Add input lines
    for (i, line) in input_lines.iter().enumerate() {
        let prefix = if i == 0 { "> " } else { "  " };
        text.push(Line::from(vec![
            Span::styled(prefix, Style::default().fg(Color::Yellow)),
            Span::styled(*line, Style::default().fg(Color::Yellow)),
        ]));
    }
    // Handle empty input
    if input_lines.is_empty() {
        text.push(Line::from(vec![Span::styled(
            "> ",
            Style::default().fg(Color::Yellow),
        )]));
    }

    let input = Paragraph::new(text)
        .style(Style::default().bg(Color::Black))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Green).bg(Color::Black))
                .style(Style::default().bg(Color::Black))
                .title(" Edit Comment (Enter=save, Shift+Enter=newline, Esc=cancel) "),
        );
    f.render_widget(input, popup_area);

    // Calculate cursor position for multiline input
    let last_line = input_lines.last().copied().unwrap_or("");
    let cursor_x = popup_area.x + 3 + last_line.len() as u16; // 3 = "> " + border
    let cursor_y = popup_area.y + 4 + (input_line_count.saturating_sub(1)) as u16;
    f.set_cursor_position((cursor_x, cursor_y));
}

// ============ TODO Rendering ============

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
    // Convert to ListItems with indentation
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
    let input = Paragraph::new(app.input_buffer.as_str()).style(Style::default().fg(Color::White));
    f.render_widget(input, inner);

    // Cursor
    f.set_cursor_position((inner.x + app.input_buffer.len() as u16, inner.y));
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
    let input = Paragraph::new(app.input_buffer.as_str()).style(Style::default().fg(Color::White));
    f.render_widget(input, inner);

    // Cursor
    f.set_cursor_position((inner.x + app.input_buffer.len() as u16, inner.y));
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
    let input = Paragraph::new(app.input_buffer.as_str()).style(Style::default().fg(Color::White));
    f.render_widget(input, inner);

    // Cursor
    f.set_cursor_position((inner.x + app.input_buffer.len() as u16, inner.y));
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
