//! Git status panel rendering

use crate::tui::app::App;
use crate::tui::state::{Focus, GitSection};
use ccm_proto::daemon::FileStatus;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};

/// Draw git status panel
pub fn draw_git_status_panel(f: &mut Frame, area: Rect, app: &App) {
    let is_focused = app.focus == Focus::GitStatus;
    let border_style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let _current_item = app.current_git_panel_item();
    let mut items: Vec<ListItem> = Vec::new();
    let mut cursor_pos = 0;

    let sections = [
        (GitSection::Staged, "◆ Staged", Color::Green),
        (GitSection::Unstaged, "◇ Unstaged", Color::Yellow),
        (GitSection::Untracked, "? Untracked", Color::Magenta),
    ];

    for (section, section_name, section_color) in sections {
        let files: Vec<_> = app
            .git
            .files
            .iter()
            .enumerate()
            .filter(|(_, f)| f.section == section)
            .collect();

        if files.is_empty() {
            continue;
        }

        let is_expanded = app.git.expanded_sections.contains(&section);
        let is_cursor = cursor_pos == app.git.cursor;

        // Section header style
        let section_style = if is_cursor && is_focused {
            Style::default()
                .fg(section_color)
                .add_modifier(Modifier::BOLD)
        } else if is_cursor {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let expand_char = if is_expanded { "▼" } else { "▶" };

        items.push(ListItem::new(Line::from(vec![
            Span::styled(if is_cursor { ">" } else { " " }, section_style),
            Span::styled(
                format!(" {} ", expand_char),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(format!("{} ({})", section_name, files.len()), section_style),
        ])));
        cursor_pos += 1;

        // Files in section (if expanded)
        if is_expanded {
            for (_file_idx, file) in files {
                let is_file_cursor = cursor_pos == app.git.cursor;

                let file_style = if is_file_cursor && is_focused {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else if is_file_cursor {
                    Style::default().fg(Color::White)
                } else {
                    Style::default().fg(Color::DarkGray)
                };

                // Status indicator
                let (status_char, status_color) =
                    match FileStatus::try_from(file.status).unwrap_or(FileStatus::Modified) {
                        FileStatus::Modified => ("M", Color::Yellow),
                        FileStatus::Added => ("A", Color::Green),
                        FileStatus::Deleted => ("D", Color::Red),
                        FileStatus::Renamed => ("R", Color::Cyan),
                        FileStatus::Untracked => ("?", Color::Magenta),
                        FileStatus::Unspecified => ("?", Color::DarkGray),
                    };

                // Comment count badge
                let comment_count = app.count_file_comments(&file.path);
                let comment_badge = if comment_count > 0 {
                    Span::styled(
                        format!(" 💬{}", comment_count),
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    )
                } else {
                    Span::raw("")
                };

                items.push(ListItem::new(Line::from(vec![
                    Span::styled(if is_file_cursor { ">" } else { " " }, file_style),
                    Span::raw("   "), // Indent
                    Span::styled(
                        format!("{} ", status_char),
                        Style::default().fg(status_color),
                    ),
                    Span::styled(&file.path, file_style),
                    comment_badge,
                ])));
                cursor_pos += 1;
            }
        }
    }

    // Show empty message if no files
    if items.is_empty() {
        items.push(ListItem::new(Line::from(vec![Span::styled(
            "  No changes",
            Style::default().fg(Color::DarkGray),
        )])));
    }

    let total_files = app.git.files.len();
    let title = if is_focused {
        format!(" Git Status ({}) [*] ", total_files)
    } else {
        format!(" Git Status ({}) ", total_files)
    };

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(title),
    );

    f.render_widget(list, area);
}
