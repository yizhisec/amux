//! Diff view rendering

use super::super::app::App;
use super::super::state::{DiffItem, Focus};
use super::helpers::{
    find_paired_addition, find_paired_deletion, get_highlighter, render_word_diff_line,
};
use ccm_proto::daemon::{FileStatus, LineType};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

/// Draw diff view with inline expansion
pub fn draw_diff_view(f: &mut Frame, area: Rect, app: &App) {
    draw_diff_inline(f, area, app);
}

/// Draw fullscreen diff view
pub fn draw_diff_fullscreen(f: &mut Frame, area: Rect, app: &App) {
    draw_diff_inline(f, area, app);
}

/// Draw diff with inline file expansion (unified navigation view)
pub fn draw_diff_inline(f: &mut Frame, area: Rect, app: &App) {
    let is_focused = app.focus == Focus::DiffFiles;
    let border_style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let title = if is_focused {
        format!(" Changes ({}) [*] ", app.diff.files.len())
    } else {
        format!(" Changes ({}) ", app.diff.files.len())
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(title);

    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.diff.files.is_empty() {
        let placeholder = Paragraph::new("No changes")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(ratatui::layout::Alignment::Center);
        f.render_widget(placeholder, inner);
        return;
    }

    // Get current cursor item for highlighting
    let current_item = app.current_diff_item();

    // Build list of lines: files + expanded diff content
    let mut lines: Vec<Line> = Vec::new();

    for (file_idx, file) in app.diff.files.iter().enumerate() {
        let is_file_selected = current_item == DiffItem::File(file_idx);
        let is_expanded = app.diff.expanded.contains(&file_idx);

        // File style
        let file_style = if is_file_selected && is_focused {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else if is_file_selected {
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
                FileStatus::Untracked => ("U", Color::Magenta),
                FileStatus::Unspecified => ("?", Color::DarkGray),
            };

        // Expand/collapse indicator
        let expand_indicator = if is_expanded { "▼" } else { "▶" };

        // Stats
        let stats = if file.additions > 0 || file.deletions > 0 {
            format!(" +{} -{}", file.additions, file.deletions)
        } else {
            String::new()
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

        // File line
        lines.push(Line::from(vec![
            Span::styled(if is_file_selected { ">" } else { " " }, file_style),
            Span::styled(
                format!(" {} ", expand_indicator),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                format!("{} ", status_char),
                Style::default().fg(status_color),
            ),
            Span::styled(&file.path, file_style),
            Span::styled(stats, Style::default().fg(Color::DarkGray)),
            comment_badge,
        ]));

        // If this file is expanded, show diff lines
        if is_expanded {
            if let Some(file_lines) = app.diff.file_lines.get(&file_idx) {
                for (line_idx, diff_line) in file_lines.iter().enumerate() {
                    let is_line_selected = current_item == DiffItem::Line(file_idx, line_idx);

                    let line_type =
                        LineType::try_from(diff_line.line_type).unwrap_or(LineType::Context);

                    // Check if line has a comment
                    let line_number = diff_line
                        .new_lineno
                        .unwrap_or(diff_line.old_lineno.unwrap_or(line_idx as i32));
                    let line_comment = app.get_line_comment(&file.path, line_number);
                    let comment_marker = if line_comment.is_some() {
                        Span::styled(
                            " [*]",
                            Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::BOLD),
                        )
                    } else {
                        Span::raw("")
                    };

                    let cursor_indicator = if is_line_selected { ">" } else { " " };

                    // Build the line based on type
                    let mut line_spans = vec![
                        Span::styled(
                            cursor_indicator,
                            if is_line_selected && is_focused {
                                Style::default().add_modifier(Modifier::REVERSED)
                            } else {
                                Style::default()
                            },
                        ),
                        Span::styled("   ", Style::default()), // Indent
                    ];

                    match line_type {
                        LineType::Header => {
                            let style = if is_line_selected && is_focused {
                                Style::default()
                                    .fg(Color::Cyan)
                                    .add_modifier(Modifier::BOLD | Modifier::REVERSED)
                            } else {
                                Style::default()
                                    .fg(Color::Cyan)
                                    .add_modifier(Modifier::BOLD)
                            };
                            line_spans.push(Span::styled("@@ ", style));
                            line_spans.push(Span::styled(&diff_line.content, style));
                        }
                        LineType::Addition => {
                            // Find paired deletion for word-level diff
                            let paired_content = find_paired_deletion(file_lines, line_idx)
                                .map(|del_idx| file_lines[del_idx].content.as_str());

                            let prefix_style = if is_line_selected && is_focused {
                                Style::default()
                                    .fg(Color::Green)
                                    .add_modifier(Modifier::REVERSED)
                            } else {
                                Style::default().fg(Color::Green)
                            };
                            line_spans.push(Span::styled("+ ", prefix_style));

                            // Add word-diff highlighted content
                            let content_spans = render_word_diff_line(
                                &diff_line.content,
                                paired_content,
                                true,
                                is_line_selected,
                                is_focused,
                                &file.path,
                            );
                            line_spans.extend(content_spans);
                        }
                        LineType::Deletion => {
                            // Find paired addition for word-level diff
                            let paired_content = find_paired_addition(file_lines, line_idx)
                                .map(|add_idx| file_lines[add_idx].content.as_str());

                            let prefix_style = if is_line_selected && is_focused {
                                Style::default()
                                    .fg(Color::Red)
                                    .add_modifier(Modifier::REVERSED)
                            } else {
                                Style::default().fg(Color::Red)
                            };
                            line_spans.push(Span::styled("- ", prefix_style));

                            // Add word-diff highlighted content
                            let content_spans = render_word_diff_line(
                                &diff_line.content,
                                paired_content,
                                false,
                                is_line_selected,
                                is_focused,
                                &file.path,
                            );
                            line_spans.extend(content_spans);
                        }
                        LineType::Context | LineType::Unspecified => {
                            line_spans.push(Span::styled("  ", Style::default()));
                            // Apply syntax highlighting to context lines too
                            let highlighter = get_highlighter();
                            let syntax_spans =
                                highlighter.highlight_line(&diff_line.content, &file.path);
                            for (style, text) in syntax_spans {
                                let final_style = if is_line_selected && is_focused {
                                    style.add_modifier(Modifier::REVERSED)
                                } else {
                                    style
                                };
                                line_spans.push(Span::styled(text.to_string(), final_style));
                            }
                        }
                    }

                    line_spans.push(comment_marker);
                    lines.push(Line::from(line_spans));

                    // If line has a comment, show comment box below
                    if let Some(comment) = line_comment {
                        // Truncate file path for display
                        let display_path = if file.path.len() > 30 {
                            format!("...{}", &file.path[file.path.len() - 27..])
                        } else {
                            file.path.clone()
                        };

                        // Comment box top border with file info
                        lines.push(Line::from(vec![
                            Span::raw("     "),
                            Span::styled("┌─[", Style::default().fg(Color::DarkGray)),
                            Span::styled(display_path, Style::default().fg(Color::Cyan)),
                            Span::styled(":", Style::default().fg(Color::DarkGray)),
                            Span::styled(
                                format!("{}", line_number),
                                Style::default().fg(Color::Yellow),
                            ),
                            Span::styled("]─", Style::default().fg(Color::DarkGray)),
                        ]));

                        // Comment content (wrap if needed)
                        let comment_text = &comment.comment;
                        let max_width = 50;
                        for chunk in comment_text
                            .chars()
                            .collect::<Vec<_>>()
                            .chunks(max_width)
                            .map(|c| c.iter().collect::<String>())
                        {
                            lines.push(Line::from(vec![
                                Span::raw("     "),
                                Span::styled("│ ", Style::default().fg(Color::DarkGray)),
                                Span::styled(chunk, Style::default().fg(Color::White)),
                            ]));
                        }

                        // Comment box bottom border
                        lines.push(Line::from(vec![
                            Span::raw("     "),
                            Span::styled(
                                "└".to_string() + &"─".repeat(46),
                                Style::default().fg(Color::DarkGray),
                            ),
                        ]));
                    }
                }
            }
        }
    }

    // Calculate scroll - we need to ensure cursor is visible
    let visible_height = inner.height as usize;
    let total_lines = lines.len();
    let cursor_line = app.diff.cursor;

    // Calculate scroll offset to keep cursor visible
    let scroll_offset = if cursor_line < app.diff.scroll_offset {
        cursor_line
    } else if cursor_line >= app.diff.scroll_offset + visible_height {
        cursor_line.saturating_sub(visible_height / 2)
    } else {
        app.diff.scroll_offset
    }
    .min(total_lines.saturating_sub(visible_height));

    // Render visible lines
    let visible_lines: Vec<Line> = lines
        .into_iter()
        .skip(scroll_offset)
        .take(visible_height)
        .collect();

    let paragraph = Paragraph::new(visible_lines);
    f.render_widget(paragraph, inner);

    // Show scroll indicator if needed
    if total_lines > visible_height {
        let scroll_info = format!(" {}/{} ", cursor_line + 1, total_lines);
        let scroll_len = scroll_info.len() as u16;
        let scroll_span = Span::styled(scroll_info, Style::default().fg(Color::DarkGray));
        let scroll_x = area.x + area.width.saturating_sub(scroll_len + 1);
        let scroll_y = area.y;
        f.render_widget(
            Paragraph::new(scroll_span),
            Rect::new(scroll_x, scroll_y, scroll_len, 1),
        );
    }
}
