//! Sidebar rendering (worktrees and sessions)

use crate::tui::app::App;
use crate::tui::state::Focus;
use crate::tui::views::git_status::draw_git_status_panel;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};

/// Draw sidebar with worktrees and sessions
pub fn draw_sidebar(f: &mut Frame, area: Rect, app: &App) {
    if app.sidebar.tree_view_enabled {
        // Tree view with git status panel
        if app.sidebar.git_panel_enabled {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(60), // Worktrees
                    Constraint::Percentage(40), // Git Status
                ])
                .split(area);

            draw_sidebar_tree(f, chunks[0], app);
            draw_git_status_panel(f, chunks[1], app);
        } else {
            // Tree view: single list with worktrees and nested sessions
            draw_sidebar_tree(f, area, app);
        }
    } else {
        // Legacy view: split sidebar into worktrees and sessions
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(50), // Worktrees
                Constraint::Percentage(50), // Sessions
            ])
            .split(area);

        draw_worktrees(f, chunks[0], app);
        draw_sessions(f, chunks[1], app);
    }
}

/// Draw tree view sidebar (worktrees with nested sessions)
pub fn draw_sidebar_tree(f: &mut Frame, area: Rect, app: &App) {
    let is_focused = app.focus == Focus::Sidebar;
    let border_style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let mut items: Vec<ListItem> = Vec::new();
    let mut cursor_pos = 0;

    let repo = app.current_repo();
    let expanded_worktrees = repo.map(|r| &r.expanded_worktrees);
    let sessions_by_worktree = repo.map(|r| &r.sessions_by_worktree);

    for (wt_idx, wt) in app.worktrees().iter().enumerate() {
        let is_expanded = expanded_worktrees
            .map(|e| e.contains(&wt_idx))
            .unwrap_or(false);
        let is_cursor = cursor_pos == app.sidebar.cursor;

        // Worktree row style
        let wt_style = if is_cursor && is_focused {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else if is_cursor {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        // Expand indicator
        let expand_char = if is_expanded { "▼" } else { "▶" };

        // Worktree indicator: ◆ for main, ● for others
        let wt_indicator = if wt.is_main { "◆" } else { "●" };

        // Session count indicator
        let session_count = sessions_by_worktree
            .and_then(|sbw| sbw.get(&wt_idx))
            .map(|s| s.len())
            .unwrap_or(wt.session_count as usize);
        let session_indicator = if session_count > 0 {
            format!(" ({})", session_count)
        } else {
            String::new()
        };

        items.push(ListItem::new(Line::from(vec![
            Span::styled(if is_cursor { ">" } else { " " }, wt_style),
            Span::styled(
                format!(" {} ", expand_char),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                format!("{} ", wt_indicator),
                Style::default().fg(Color::Cyan),
            ),
            Span::styled(&wt.branch, wt_style),
            Span::styled(session_indicator, Style::default().fg(Color::Green)),
        ])));
        cursor_pos += 1;

        // Render sessions if expanded
        if is_expanded {
            if let Some(sessions) = sessions_by_worktree.and_then(|sbw| sbw.get(&wt_idx)) {
                for session in sessions.iter() {
                    let is_session_cursor = cursor_pos == app.sidebar.cursor;
                    let is_active = app.terminal.active_session_id.as_ref() == Some(&session.id);

                    let s_style = if is_session_cursor && is_focused {
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD)
                    } else if is_session_cursor {
                        Style::default().fg(Color::White)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    };

                    let active_indicator = if is_active { "▶" } else { " " };
                    let status_char = match session.status {
                        1 => "●", // Running
                        _ => "○", // Stopped
                    };

                    items.push(ListItem::new(Line::from(vec![
                        Span::styled(if is_session_cursor { ">" } else { " " }, s_style),
                        Span::raw("     "), // Indent for nesting
                        Span::styled(
                            format!("{} ", active_indicator),
                            Style::default().fg(Color::Green),
                        ),
                        Span::styled(
                            format!("{} ", status_char),
                            Style::default().fg(if session.status == 1 {
                                Color::Green
                            } else {
                                Color::DarkGray
                            }),
                        ),
                        Span::styled(&session.name, s_style),
                    ])));
                    cursor_pos += 1;
                }
            }
        }
    }

    let title = if is_focused {
        " Worktrees [*] "
    } else {
        " Worktrees "
    };
    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(title),
    );

    f.render_widget(list, area);
}

/// Draw worktrees list (only branches with worktrees)
pub fn draw_worktrees(f: &mut Frame, area: Rect, app: &App) {
    let is_focused = app.focus == Focus::Branches;
    let border_style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let items: Vec<ListItem> = app
        .worktrees()
        .iter()
        .enumerate()
        .map(|(i, wt)| {
            let is_selected = i == app.branch_idx();
            let style = if is_selected && is_focused {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else if is_selected {
                Style::default().fg(Color::White)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            // Worktree indicator: ◆ for main, ● for others
            let indicator = if wt.is_main { "◆" } else { "●" };

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
        " Worktrees [*] "
    } else {
        " Worktrees "
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
pub fn draw_sessions(f: &mut Frame, area: Rect, app: &App) {
    let is_focused = app.focus == Focus::Sessions;
    let border_style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let current_branch = app
        .worktrees()
        .get(app.branch_idx())
        .map(|b| b.branch.as_str())
        .unwrap_or("?");

    let items: Vec<ListItem> = app
        .sessions()
        .iter()
        .enumerate()
        .map(|(i, session)| {
            let is_selected = i == app.session_idx();
            let is_active = app.terminal.active_session_id.as_ref() == Some(&session.id);

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
