//! Sidebar rendering (worktrees and sessions)

use crate::tui::app::App;
use crate::tui::icons::box_drawing;
use crate::tui::state::Focus;
use crate::tui::views::git_status::draw_git_status_panel;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem},
    Frame,
};

/// Draw sidebar with worktrees and sessions
pub fn draw_sidebar(f: &mut Frame, area: Rect, app: &App) {
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
        draw_sidebar_tree(f, area, app);
    }
}

/// Draw tree view sidebar (worktrees with nested sessions)
pub fn draw_sidebar_tree(f: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let icons = &app.icons;
    let is_focused = app.focus == Focus::Sidebar;

    let border_style = if is_focused {
        theme.focused_border_style()
    } else {
        theme.unfocused_border_style()
    };

    let mut items: Vec<ListItem> = Vec::new();
    let mut cursor_pos = 0;

    let repo = app.current_repo();
    let sidebar_cursor = repo.map(|r| r.sidebar_cursor).unwrap_or(0);
    let expanded_worktrees = repo.map(|r| &r.expanded_worktrees);
    let sessions_by_worktree = repo.map(|r| &r.sessions_by_worktree);

    for (wt_idx, wt) in app.worktrees().iter().enumerate() {
        let is_expanded = expanded_worktrees
            .map(|e| e.contains(&wt_idx))
            .unwrap_or(false);
        let is_cursor = cursor_pos == sidebar_cursor;

        // Worktree row style
        let wt_style = if is_cursor && is_focused {
            theme.selection_style()
        } else if is_cursor {
            theme.selection_unfocused_style()
        } else {
            theme.normal_style()
        };

        // Expand indicator
        let expand_char = if is_expanded {
            icons.collapse()
        } else {
            icons.expand()
        };

        // Worktree indicator
        let wt_indicator = if wt.is_main {
            icons.main_worktree()
        } else {
            icons.worktree()
        };

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
            Span::styled(
                icons.cursor(),
                if is_cursor {
                    wt_style
                } else {
                    Style::default()
                },
            ),
            Span::styled(
                format!(" {} ", expand_char),
                Style::default().fg(theme.text_tertiary),
            ),
            Span::styled(
                format!("{} ", wt_indicator),
                Style::default().fg(theme.neon_cyan),
            ),
            Span::styled(&wt.branch, wt_style),
            Span::styled(session_indicator, Style::default().fg(theme.neon_green)),
        ])));
        cursor_pos += 1;

        // Render sessions if expanded
        if is_expanded {
            if let Some(sessions) = sessions_by_worktree.and_then(|sbw| sbw.get(&wt_idx)) {
                for session in sessions.iter() {
                    let is_session_cursor = cursor_pos == sidebar_cursor;
                    let is_active = app.terminal.active_session_id.as_ref() == Some(&session.id);

                    let s_style = if is_session_cursor && is_focused {
                        theme.selection_style()
                    } else if is_session_cursor {
                        theme.selection_unfocused_style()
                    } else {
                        theme.normal_style()
                    };

                    let active_indicator = if is_active {
                        icons.active_indicator()
                    } else {
                        " "
                    };
                    let status_icon = if session.status == 1 {
                        icons.running()
                    } else {
                        icons.stopped()
                    };

                    items.push(ListItem::new(Line::from(vec![
                        Span::styled(
                            icons.cursor(),
                            if is_session_cursor {
                                s_style
                            } else {
                                Style::default()
                            },
                        ),
                        Span::raw("     "), // Indent for nesting
                        Span::styled(
                            format!("{} ", active_indicator),
                            Style::default().fg(theme.neon_green),
                        ),
                        Span::styled(
                            format!("{} ", status_icon),
                            Style::default().fg(if session.status == 1 {
                                theme.success
                            } else {
                                theme.text_disabled
                            }),
                        ),
                        Span::styled(&session.name, s_style),
                    ])));
                    cursor_pos += 1;
                }
            }
        }
    }

    // Title with focus indicator and decorative elements
    let title = if is_focused {
        format!(" {} Worktrees [*] ", box_drawing::HEAVY_VERTICAL)
    } else {
        " Worktrees ".to_string()
    };

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(border_style)
            .title(title),
    );

    f.render_widget(list, area);
}
