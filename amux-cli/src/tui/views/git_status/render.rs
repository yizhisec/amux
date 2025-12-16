//! Git status panel rendering

use crate::tui::app::App;
use crate::tui::state::{Focus, GitSection};
use crate::tui::theme::{GitFileStatus, GitSection as ThemeGitSection};
use crate::tui::widgets::VirtualList;
use amux_proto::daemon::FileStatus;
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, Paragraph},
    Frame,
};

/// Draw git status panel
pub fn draw_git_status_panel(f: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let icons = &app.icons;
    let is_focused = app.focus == Focus::GitStatus;

    let border_style = if is_focused {
        theme.focused_border_style()
    } else {
        theme.unfocused_border_style()
    };

    let _current_item = app.current_git_panel_item();
    let mut items: Vec<ListItem> = Vec::new();
    let mut cursor_pos = 0;

    let Some(git) = app.git() else {
        // No git state
        let list = List::new(vec![ListItem::new(Line::from(vec![Span::styled(
            "  No git state",
            Style::default().fg(theme.text_disabled),
        )]))])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(border_style)
                .title(" Git Status "),
        );
        f.render_widget(list, area);
        return;
    };

    let sections = [
        (
            GitSection::Staged,
            ThemeGitSection::Staged,
            icons.staged_indicator(),
            "Staged",
        ),
        (
            GitSection::Unstaged,
            ThemeGitSection::Unstaged,
            icons.unstaged_indicator(),
            "Unstaged",
        ),
        (
            GitSection::Untracked,
            ThemeGitSection::Untracked,
            icons.untracked_indicator(),
            "Untracked",
        ),
    ];

    for (section, theme_section, section_icon, section_name) in sections {
        let section_color = theme.git_section_color(theme_section);
        let files: Vec<_> = git
            .files
            .iter()
            .enumerate()
            .filter(|(_, f)| f.section == section)
            .collect();

        if files.is_empty() {
            continue;
        }

        let is_expanded = git.expanded_sections.contains(&section);
        let is_cursor = cursor_pos == git.cursor;

        // Section header style
        let section_style = if is_cursor && is_focused {
            Style::default()
                .fg(section_color)
                .add_modifier(Modifier::BOLD)
        } else if is_cursor {
            theme.selection_unfocused_style()
        } else {
            theme.normal_style()
        };

        let expand_char = if is_expanded {
            icons.collapse()
        } else {
            icons.expand()
        };

        items.push(ListItem::new(Line::from(vec![
            Span::styled(
                icons.cursor(),
                if is_cursor {
                    section_style
                } else {
                    Style::default()
                },
            ),
            Span::styled(
                format!(" {} ", expand_char),
                Style::default().fg(theme.text_tertiary),
            ),
            Span::styled(
                format!("{} {} ({})", section_icon, section_name, files.len()),
                section_style,
            ),
        ])));
        cursor_pos += 1;

        // Files in section (if expanded)
        if is_expanded {
            for (_file_idx, file) in files {
                let is_file_cursor = cursor_pos == git.cursor;

                let file_style = if is_file_cursor && is_focused {
                    theme.selection_style()
                } else if is_file_cursor {
                    theme.selection_unfocused_style()
                } else {
                    theme.normal_style()
                };

                // Status indicator
                let file_status =
                    match FileStatus::try_from(file.status).unwrap_or(FileStatus::Modified) {
                        FileStatus::Modified => GitFileStatus::Modified,
                        FileStatus::Added => GitFileStatus::Added,
                        FileStatus::Deleted => GitFileStatus::Deleted,
                        FileStatus::Renamed => GitFileStatus::Renamed,
                        FileStatus::Untracked => GitFileStatus::Untracked,
                        FileStatus::Unspecified => GitFileStatus::Unknown,
                    };

                let status_char = match file_status {
                    GitFileStatus::Modified => icons.git_modified(),
                    GitFileStatus::Added => icons.git_added(),
                    GitFileStatus::Deleted => icons.git_deleted(),
                    GitFileStatus::Renamed => icons.git_renamed(),
                    GitFileStatus::Untracked => icons.git_untracked(),
                    GitFileStatus::Unknown => "?",
                };

                let status_color = theme.git_status_color(file_status);

                // Comment count badge
                let comment_count = app.count_file_comments(&file.path);
                let comment_badge = if comment_count > 0 {
                    Span::styled(
                        format!(" {}{}", icons.comment(), comment_count),
                        Style::default()
                            .fg(theme.neon_yellow)
                            .add_modifier(Modifier::BOLD),
                    )
                } else {
                    Span::raw("")
                };

                items.push(ListItem::new(Line::from(vec![
                    Span::styled(
                        icons.cursor(),
                        if is_file_cursor {
                            file_style
                        } else {
                            Style::default()
                        },
                    ),
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
            Style::default().fg(theme.text_disabled),
        )])));
    }

    let total_files = git.files.len();
    let total_items = items.len();

    // Calculate visible area (subtract 2 for borders)
    let visible_height = area.height.saturating_sub(2) as usize;

    // Calculate scroll offset using VirtualList trait
    let scroll_offset = git.scroll_offset(visible_height);

    // Slice items to visible range
    let visible_items: Vec<ListItem> = items
        .into_iter()
        .skip(scroll_offset)
        .take(visible_height)
        .collect();

    let title = if is_focused {
        format!(" Git Status ({}) [*] ", total_files)
    } else {
        format!(" Git Status ({}) ", total_files)
    };

    let list = List::new(visible_items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(border_style)
            .title(title),
    );

    f.render_widget(list, area);

    // Show scroll indicator if needed
    if total_items > visible_height {
        let scroll_info = format!(" {}/{} ", git.cursor + 1, total_items);
        let scroll_len = scroll_info.len() as u16;
        let scroll_span = Span::styled(scroll_info, Style::default().fg(theme.text_tertiary));
        let scroll_x = area.x + area.width.saturating_sub(scroll_len + 1);
        let scroll_y = area.y;
        f.render_widget(
            Paragraph::new(scroll_span),
            Rect::new(scroll_x, scroll_y, scroll_len, 1),
        );
    }
}
