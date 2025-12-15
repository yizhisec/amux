//! Dialog overlays (confirmations, inputs)
//!
//! Non-TODO dialogs and overlays for the application.
//! TODO-related overlays are in views/todo/render.rs

use crate::tui::app::App;
use crate::tui::state::{DeleteTarget, ExitCleanupAction};
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
    let input = Paragraph::new(app.text_input.content())
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
        popup_area.x + app.text_input.cursor_display_offset() as u16 + 1,
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
    let input = Paragraph::new(app.text_input.content())
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
        popup_area.x + app.text_input.cursor_display_offset() as u16 + 1,
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
    let branch_count = app.available_branches().len();
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
    let base_style = if !app.text_input.is_empty() {
        Style::default().fg(Color::Green).bg(Color::Black)
    } else {
        Style::default().fg(Color::DarkGray).bg(Color::Black)
    };
    let base_paragraph = Paragraph::new(base_info).style(base_style);
    f.render_widget(base_paragraph, chunks[1]);

    // Branch list
    if !app.available_branches().is_empty() {
        let items: Vec<ListItem> = app
            .available_branches()
            .iter()
            .enumerate()
            .map(|(i, branch)| {
                let is_selected = i == app.add_worktree_idx() && app.text_input.is_empty();
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
    let input_style = if !app.text_input.is_empty() {
        Style::default().fg(Color::Yellow).bg(Color::Black)
    } else {
        Style::default().fg(Color::DarkGray).bg(Color::Black)
    };
    let input_text = if app.text_input.is_empty() {
        "New branch: (type to create new)"
    } else {
        app.text_input.content()
    };
    let prefix = if !app.text_input.is_empty() {
        "> "
    } else {
        "  "
    };
    let input = Paragraph::new(format!("{}New: {}", prefix, input_text)).style(input_style);
    f.render_widget(input, chunks[4]);

    // Show cursor if typing
    if !app.text_input.is_empty() {
        f.set_cursor_position((
            chunks[4].x + 7 + app.text_input.cursor_display_offset() as u16, // 7 = "> New: ".len()
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
    let input_lines: Vec<&str> = app.text_input.content().lines().collect();
    let input_line_count = input_lines.len().max(1);

    // Center the input box with dynamic height
    let popup_width = 70.min(area.width.saturating_sub(4));
    let popup_height = (6 + input_line_count as u16).min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(popup_width)) / 2 + area.x;
    let y = (area.height.saturating_sub(popup_height)) / 2 + area.y;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    // Truncate file path if too long
    // Only truncate if max_path_len > 3 to ensure room for "..." plus at least one character
    let max_path_len = (popup_width as usize).saturating_sub(20);
    let display_path = if file_path.len() > max_path_len && max_path_len > 3 {
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
    let cursor_x = popup_area.x + 3 + app.text_input.cursor_display_offset() as u16; // 3 = "> " + border
    let cursor_y = popup_area.y + 4 + (input_line_count.saturating_sub(1)) as u16;
    f.set_cursor_position((cursor_x, cursor_y));
}

/// Draw select provider overlay for new session
pub fn draw_select_provider_overlay(
    f: &mut Frame,
    area: Rect,
    providers: &[String],
    selected_index: usize,
    loading: bool,
) {
    // Calculate popup size based on content
    let popup_height = (providers.len() + 4).min(15) as u16; // +4 for borders, title, instructions
    let popup_width = 50.min(area.width.saturating_sub(4));
    let x = (area.width.saturating_sub(popup_width)) / 2 + area.x;
    let y = (area.height.saturating_sub(popup_height)) / 2 + area.y;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    // Split popup into sections
    let inner = popup_area.inner(ratatui::layout::Margin::new(1, 1));
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Instructions
            Constraint::Min(1),    // Provider list
        ])
        .split(inner);

    // Draw border with background to cover underlying content
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan).bg(Color::Black))
        .style(Style::default().bg(Color::Black))
        .title(" Select Provider (j/k=select, Enter=create, Esc=cancel) ");
    f.render_widget(block, popup_area);

    // Instructions
    let instructions_text = if loading {
        "Loading providers...".to_string()
    } else {
        "Select AI provider for new session:".to_string()
    };
    let instructions = Paragraph::new(instructions_text)
        .style(Style::default().fg(Color::DarkGray).bg(Color::Black));
    f.render_widget(instructions, chunks[0]);

    // Provider list
    let items: Vec<ListItem> = providers
        .iter()
        .enumerate()
        .map(|(i, provider)| {
            let is_selected = i == selected_index;
            let style = if is_selected {
                Style::default()
                    .fg(Color::Yellow)
                    .bg(Color::Black)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White).bg(Color::Black)
            };
            let prefix = if is_selected { "> " } else { "  " };
            ListItem::new(format!("{}{}", prefix, provider)).style(style)
        })
        .collect();
    let list = List::new(items).style(Style::default().bg(Color::Black));
    f.render_widget(list, chunks[1]);
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
    let input_lines: Vec<&str> = app.text_input.content().lines().collect();
    let input_line_count = input_lines.len().max(1);

    // Center the input box with dynamic height
    let popup_width = 70.min(area.width.saturating_sub(4));
    let popup_height = (6 + input_line_count as u16).min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(popup_width)) / 2 + area.x;
    let y = (area.height.saturating_sub(popup_height)) / 2 + area.y;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    // Truncate file path if too long
    // Only truncate if max_path_len > 3 to ensure room for "..." plus at least one character
    let max_path_len = (popup_width as usize).saturating_sub(20);
    let display_path = if file_path.len() > max_path_len && max_path_len > 3 {
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
    let cursor_x = popup_area.x + 3 + app.text_input.cursor_display_offset() as u16; // 3 = "> " + border
    let cursor_y = popup_area.y + 4 + (input_line_count.saturating_sub(1)) as u16;
    f.set_cursor_position((cursor_x, cursor_y));
}
