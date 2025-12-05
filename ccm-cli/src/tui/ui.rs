//! TUI rendering - Tab + Sidebar + Terminal layout

use super::app::{App, Focus, InputMode, TerminalMode};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Tabs},
    Frame,
};

/// Draw the TUI
pub fn draw(f: &mut Frame, app: &App) {
    // Main layout: Tab bar + Main content + Status bar
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Tab bar
            Constraint::Min(0),    // Main content
            Constraint::Length(3), // Status bar
        ])
        .split(f.area());

    draw_tab_bar(f, chunks[0], app);
    draw_main_content(f, chunks[1], app);
    draw_status_bar(f, chunks[2], app);
}

/// Draw repo tabs at the top
fn draw_tab_bar(f: &mut Frame, area: Rect, app: &App) {
    let titles: Vec<Line> = app
        .repos
        .iter()
        .enumerate()
        .map(|(i, repo)| {
            let num = if i < 9 {
                format!("{}:", i + 1)
            } else {
                String::new()
            };
            Line::from(format!("{}{}", num, repo.name))
        })
        .collect();

    let tabs = Tabs::new(titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" CCM - Claude Code Manager "),
        )
        .select(app.repo_idx)
        .style(Style::default().fg(Color::White))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .divider(" | ");

    f.render_widget(tabs, area);
}

/// Draw main content: Sidebar + Terminal
fn draw_main_content(f: &mut Frame, area: Rect, app: &App) {
    // Check for input mode overlay
    if app.input_mode == InputMode::NewBranch {
        draw_input_overlay(f, area, app);
        return;
    }

    // Fullscreen terminal mode
    if app.terminal_fullscreen && app.focus == Focus::Terminal {
        draw_terminal_fullscreen(f, area, app);
        return;
    }

    // Split into sidebar and terminal
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25), // Sidebar
            Constraint::Percentage(75), // Terminal
        ])
        .split(area);

    draw_sidebar(f, chunks[0], app);
    draw_terminal(f, chunks[1], app);
}

/// Draw sidebar with branches and sessions
fn draw_sidebar(f: &mut Frame, area: Rect, app: &App) {
    // Split sidebar into branches and sessions
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(50), // Branches
            Constraint::Percentage(50), // Sessions
        ])
        .split(area);

    draw_branches(f, chunks[0], app);
    draw_sessions(f, chunks[1], app);
}

/// Draw branches list
fn draw_branches(f: &mut Frame, area: Rect, app: &App) {
    let is_focused = app.focus == Focus::Branches;
    let border_style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let items: Vec<ListItem> = app
        .branches
        .iter()
        .enumerate()
        .map(|(i, wt)| {
            let is_selected = i == app.branch_idx;
            let style = if is_selected && is_focused {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else if is_selected {
                Style::default().fg(Color::White)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            // Worktree indicator
            let indicator = if wt.path.is_empty() {
                "○" // No worktree
            } else if wt.is_main {
                "◆" // Main worktree
            } else {
                "●" // Has worktree
            };

            // Session count indicator
            let session_indicator = if wt.session_count > 0 {
                format!(" ({})", wt.session_count)
            } else {
                String::new()
            };

            ListItem::new(Line::from(vec![
                Span::styled(if is_selected { ">" } else { " " }, style),
                Span::styled(format!(" {} ", indicator), Style::default().fg(Color::Cyan)),
                Span::styled(&wt.branch, style),
                Span::styled(session_indicator, Style::default().fg(Color::Green)),
            ]))
        })
        .collect();

    let title = if is_focused {
        " Branches [*] "
    } else {
        " Branches "
    };
    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(title),
    );

    f.render_widget(list, area);
}

/// Draw sessions list
fn draw_sessions(f: &mut Frame, area: Rect, app: &App) {
    let is_focused = app.focus == Focus::Sessions;
    let border_style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let current_branch = app
        .branches
        .get(app.branch_idx)
        .map(|b| b.branch.as_str())
        .unwrap_or("?");

    let items: Vec<ListItem> = app
        .sessions
        .iter()
        .enumerate()
        .map(|(i, session)| {
            let is_selected = i == app.session_idx;
            let is_active = app.active_session_id.as_ref() == Some(&session.id);

            let style = if is_selected && is_focused {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else if is_selected {
                Style::default().fg(Color::White)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            // Active indicator
            let active_indicator = if is_active { "▶" } else { " " };

            ListItem::new(Line::from(vec![
                Span::styled(if is_selected { ">" } else { " " }, style),
                Span::styled(
                    format!(" {} ", active_indicator),
                    Style::default().fg(Color::Green),
                ),
                Span::styled(&session.name, style),
            ]))
        })
        .collect();

    let title = if is_focused {
        format!(" Sessions ({}) [*] ", current_branch)
    } else {
        format!(" Sessions ({}) ", current_branch)
    };

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(title),
    );

    f.render_widget(list, area);
}

/// Draw terminal preview/interaction area
fn draw_terminal(f: &mut Frame, area: Rect, app: &App) {
    let is_terminal_focused = app.focus == Focus::Terminal;
    let border_color = if is_terminal_focused {
        match app.terminal_mode {
            TerminalMode::Insert => Color::Green,
            TerminalMode::Normal => Color::Yellow,
        }
    } else {
        Color::DarkGray
    };

    let title = if is_terminal_focused {
        match app.terminal_mode {
            TerminalMode::Insert => " Terminal [INSERT] ",
            TerminalMode::Normal => " Terminal [NORMAL] ",
        }
    } else if app.active_session_id.is_some() {
        " Terminal [Preview] "
    } else {
        " Terminal [No session] "
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(title);

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Render terminal content using vt100
    if app.active_session_id.is_some() {
        let lines = app.get_terminal_lines(inner.height, inner.width);
        let paragraph = Paragraph::new(lines);
        f.render_widget(paragraph, inner);
    } else {
        // Show placeholder
        let placeholder = Paragraph::new("Select a session to see terminal output")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(ratatui::layout::Alignment::Center);
        f.render_widget(placeholder, inner);
    }
}

/// Draw fullscreen terminal
fn draw_terminal_fullscreen(f: &mut Frame, area: Rect, app: &App) {
    let border_color = match app.terminal_mode {
        TerminalMode::Insert => Color::Green,
        TerminalMode::Normal => Color::Yellow,
    };

    let title = match app.terminal_mode {
        TerminalMode::Insert => " Terminal [INSERT - FULLSCREEN] ",
        TerminalMode::Normal => " Terminal [NORMAL - FULLSCREEN] ",
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(title);

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Render terminal content
    let lines = app.get_terminal_lines(inner.height, inner.width);
    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, inner);
}

/// Draw input overlay for new branch
fn draw_input_overlay(f: &mut Frame, area: Rect, app: &App) {
    // Center the input box
    let popup_width = 60.min(area.width.saturating_sub(4));
    let popup_height = 5;
    let x = (area.width.saturating_sub(popup_width)) / 2 + area.x;
    let y = (area.height.saturating_sub(popup_height)) / 2 + area.y;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    // Clear background
    let clear = Block::default().style(Style::default().bg(Color::Black));
    f.render_widget(clear, area);

    // Draw input box
    let input = Paragraph::new(app.input_buffer.as_str())
        .style(Style::default().fg(Color::Yellow))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(" New Branch (Enter=create, Esc=cancel) "),
        );
    f.render_widget(input, popup_area);

    // Show cursor
    f.set_cursor_position((
        popup_area.x + app.input_buffer.len() as u16 + 1,
        popup_area.y + 1,
    ));
}

/// Draw status bar at the bottom
fn draw_status_bar(f: &mut Frame, area: Rect, app: &App) {
    let (message, color) = if let Some(err) = &app.error_message {
        (err.clone(), Color::Red)
    } else if let Some(status) = &app.status_message {
        (status.clone(), Color::Green)
    } else {
        let help = match app.focus {
            Focus::Branches => "[1-9] Repo | [Tab] Sessions | [j/k] Move | [n] New | [d] Delete | [q] Quit",
            Focus::Sessions => "[Tab] Terminal | [j/k] Move | [Enter] Terminal | [n] New | [d] Delete | [q] Quit",
            Focus::Terminal => match app.terminal_mode {
                TerminalMode::Normal => "[j/k] Scroll | [Ctrl+d/u] Page | [G] Bottom | [g] Top | [i] Insert | [f] Fullscreen | [Esc] Exit",
                TerminalMode::Insert => "[Esc] Normal mode | Keys sent to terminal",
            },
        };
        (help.to_string(), Color::DarkGray)
    };

    let paragraph = Paragraph::new(message)
        .style(Style::default().fg(color))
        .block(Block::default().borders(Borders::ALL));

    f.render_widget(paragraph, area);
}
