//! TUI rendering - Tab + Sidebar + Terminal layout

use super::app::App;
use super::state::{
    DeleteTarget, Focus, GitSection, InputMode, PrefixMode, RightPanelView, TerminalMode,
};
use super::highlight::Highlighter;
use ccm_proto::daemon::{DiffLine, FileStatus, LineType, TodoItem};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Tabs},
    Frame,
};
use std::sync::OnceLock;

/// Global highlighter instance (lazy initialized)
fn get_highlighter() -> &'static Highlighter {
    static HIGHLIGHTER: OnceLock<Highlighter> = OnceLock::new();
    HIGHLIGHTER.get_or_init(Highlighter::new)
}

// ========== Word-level diff highlighting ==========

/// Token for word diff
#[derive(Debug, Clone, PartialEq)]
enum DiffToken {
    Same(String),
    Changed(String),
}

/// Compute word-level diff between two strings
/// Returns tokens for the "new" side (addition line)
fn compute_word_diff(old_line: &str, new_line: &str) -> Vec<DiffToken> {
    let old_words: Vec<&str> = old_line.split_whitespace().collect();
    let new_words: Vec<&str> = new_line.split_whitespace().collect();

    if old_words.is_empty() || new_words.is_empty() {
        // If either is empty, everything is changed
        return vec![DiffToken::Changed(new_line.to_string())];
    }

    // Simple LCS-based word diff
    let mut result = Vec::new();
    let lcs = compute_lcs(&old_words, &new_words);

    let mut old_idx = 0;
    let mut new_idx = 0;
    let mut lcs_idx = 0;

    while new_idx < new_words.len() {
        if lcs_idx < lcs.len() && new_idx < new_words.len() && new_words[new_idx] == lcs[lcs_idx] {
            // This word is in LCS - it's the same
            // Skip any old words that aren't in the match
            while old_idx < old_words.len() && old_words[old_idx] != lcs[lcs_idx] {
                old_idx += 1;
            }
            result.push(DiffToken::Same(new_words[new_idx].to_string()));
            new_idx += 1;
            old_idx += 1;
            lcs_idx += 1;
        } else {
            // This word is not in LCS - it's changed/added
            result.push(DiffToken::Changed(new_words[new_idx].to_string()));
            new_idx += 1;
        }
    }

    result
}

/// Compute Longest Common Subsequence of word slices
fn compute_lcs<'a>(a: &[&'a str], b: &[&'a str]) -> Vec<&'a str> {
    let m = a.len();
    let n = b.len();

    if m == 0 || n == 0 {
        return Vec::new();
    }

    // DP table
    let mut dp = vec![vec![0usize; n + 1]; m + 1];

    for i in 1..=m {
        for j in 1..=n {
            if a[i - 1] == b[j - 1] {
                dp[i][j] = dp[i - 1][j - 1] + 1;
            } else {
                dp[i][j] = dp[i - 1][j].max(dp[i][j - 1]);
            }
        }
    }

    // Backtrack to find LCS
    let mut lcs = Vec::new();
    let mut i = m;
    let mut j = n;

    while i > 0 && j > 0 {
        if a[i - 1] == b[j - 1] {
            lcs.push(a[i - 1]);
            i -= 1;
            j -= 1;
        } else if dp[i - 1][j] > dp[i][j - 1] {
            i -= 1;
        } else {
            j -= 1;
        }
    }

    lcs.reverse();
    lcs
}

/// Find paired deletion line for an addition line
/// Returns the index of the most recent deletion line before this addition
fn find_paired_deletion(lines: &[DiffLine], addition_idx: usize) -> Option<usize> {
    // Look backwards for a deletion line
    for i in (0..addition_idx).rev() {
        let line_type = LineType::try_from(lines[i].line_type).unwrap_or(LineType::Context);
        match line_type {
            LineType::Deletion => return Some(i),
            LineType::Addition => continue, // Skip other additions
            LineType::Context | LineType::Header | LineType::Unspecified => {
                // Hit a context line, stop looking
                return None;
            }
        }
    }
    None
}

/// Find paired addition line for a deletion line
/// Returns the index of the next addition line after this deletion
fn find_paired_addition(lines: &[DiffLine], deletion_idx: usize) -> Option<usize> {
    // Look forwards for an addition line
    for (i, line) in lines.iter().enumerate().skip(deletion_idx + 1) {
        let line_type = LineType::try_from(line.line_type).unwrap_or(LineType::Context);
        match line_type {
            LineType::Addition => return Some(i),
            LineType::Deletion => continue, // Skip other deletions
            LineType::Context | LineType::Header | LineType::Unspecified => {
                // Hit a context line, stop looking
                return None;
            }
        }
    }
    None
}

/// Render a diff line with word-level highlighting and syntax highlighting
fn render_word_diff_line<'a>(
    content: &str,
    paired_content: Option<&str>,
    is_addition: bool,
    is_selected: bool,
    is_focused: bool,
    file_path: &str,
) -> Vec<Span<'a>> {
    let base_color = if is_addition {
        Color::Green
    } else {
        Color::Red
    };

    let highlight_color = if is_addition {
        Color::LightGreen
    } else {
        Color::LightRed
    };

    // Get syntax-highlighted spans first
    let highlighter = get_highlighter();
    let syntax_spans = highlighter.highlight_line(content, file_path);

    match paired_content {
        Some(paired) => {
            // We have a paired line, compute word diff
            let tokens = if is_addition {
                compute_word_diff(paired, content)
            } else {
                // For deletion, compute against the addition
                compute_word_diff(content, paired)
                    .into_iter()
                    .map(|t| match t {
                        DiffToken::Same(s) => DiffToken::Same(s),
                        DiffToken::Changed(s) => DiffToken::Changed(s),
                    })
                    .collect()
            };

            // Build spans with word diff + syntax highlighting
            let mut spans = Vec::new();
            let mut content_pos = 0;

            for token in tokens {
                let word = match &token {
                    DiffToken::Same(w) | DiffToken::Changed(w) => w.as_str(),
                };

                // Find word position in content
                if let Some(word_start) = content[content_pos..].find(word) {
                    let abs_start = content_pos + word_start;
                    let abs_end = abs_start + word.len();

                    // Add any whitespace/chars before this word
                    if abs_start > content_pos {
                        let prefix = &content[content_pos..abs_start];
                        let prefix_style = if is_selected && is_focused {
                            Style::default()
                                .fg(base_color)
                                .add_modifier(Modifier::REVERSED)
                        } else {
                            Style::default().fg(base_color)
                        };
                        spans.push(Span::styled(prefix.to_string(), prefix_style));
                    }

                    // Add the word with appropriate styling
                    let is_changed = matches!(token, DiffToken::Changed(_));

                    // Try to get syntax color for this word
                    let syntax_style =
                        find_syntax_style_for_range(&syntax_spans, abs_start, abs_end);

                    let word_style = if is_changed {
                        if is_selected && is_focused {
                            Style::default()
                                .fg(highlight_color)
                                .add_modifier(Modifier::BOLD | Modifier::REVERSED)
                        } else {
                            Style::default()
                                .fg(highlight_color)
                                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
                        }
                    } else {
                        // Use syntax highlighting color with diff tint
                        let fg_color = syntax_style.and_then(|s| s.fg).unwrap_or(base_color);
                        let tinted_color = tint_color(fg_color, is_addition);
                        if is_selected && is_focused {
                            Style::default()
                                .fg(tinted_color)
                                .add_modifier(Modifier::REVERSED)
                        } else {
                            Style::default().fg(tinted_color)
                        }
                    };

                    spans.push(Span::styled(word.to_string(), word_style));
                    content_pos = abs_end;
                }
            }

            // Add any remaining content
            if content_pos < content.len() {
                let suffix = &content[content_pos..];
                let suffix_style = if is_selected && is_focused {
                    Style::default()
                        .fg(base_color)
                        .add_modifier(Modifier::REVERSED)
                } else {
                    Style::default().fg(base_color)
                };
                spans.push(Span::styled(suffix.to_string(), suffix_style));
            }

            if spans.is_empty() {
                spans.push(Span::styled(
                    content.to_string(),
                    Style::default().fg(base_color),
                ));
            }

            spans
        }
        None => {
            // No paired line, apply syntax highlighting with diff tint
            let mut spans = Vec::new();
            for (style, text) in syntax_spans {
                let fg_color = style.fg.unwrap_or(base_color);
                let tinted_color = tint_color(fg_color, is_addition);
                let final_style = if is_selected && is_focused {
                    Style::default()
                        .fg(tinted_color)
                        .add_modifier(Modifier::REVERSED)
                } else {
                    Style::default().fg(tinted_color)
                };
                spans.push(Span::styled(text.to_string(), final_style));
            }
            if spans.is_empty() {
                let style = if is_selected && is_focused {
                    Style::default()
                        .fg(base_color)
                        .add_modifier(Modifier::REVERSED)
                } else {
                    Style::default().fg(base_color)
                };
                spans.push(Span::styled(content.to_string(), style));
            }
            spans
        }
    }
}

/// Find syntax style for a character range
fn find_syntax_style_for_range(
    spans: &[(Style, &str)],
    start: usize,
    _end: usize,
) -> Option<Style> {
    let mut pos = 0;
    for (style, text) in spans {
        let span_end = pos + text.len();
        if start >= pos && start < span_end {
            return Some(*style);
        }
        pos = span_end;
    }
    None
}

/// Apply a green/red tint to a color for diff highlighting
fn tint_color(color: Color, is_addition: bool) -> Color {
    match color {
        Color::Rgb(r, g, b) => {
            if is_addition {
                // Green tint
                Color::Rgb(
                    r.saturating_sub(20),
                    (g as u16 + 30).min(255) as u8,
                    b.saturating_sub(20),
                )
            } else {
                // Red tint
                Color::Rgb(
                    (r as u16 + 30).min(255) as u8,
                    g.saturating_sub(20),
                    b.saturating_sub(20),
                )
            }
        }
        _ => {
            // For non-RGB colors, use standard diff colors
            if is_addition {
                Color::Green
            } else {
                Color::Red
            }
        }
    }
}

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

    // Check for add worktree overlay
    if let InputMode::AddWorktree { ref base_branch } = app.input_mode {
        draw_add_worktree_overlay(f, area, app, base_branch.as_deref());
        return;
    }

    // Check for rename session overlay
    if matches!(app.input_mode, InputMode::RenameSession { .. }) {
        draw_rename_session_overlay(f, area, app);
        return;
    }

    // Check for confirm delete overlay
    if let InputMode::ConfirmDelete(ref target) = app.input_mode {
        draw_confirm_delete_overlay(f, area, target);
        return;
    }

    // Check for confirm delete branch overlay
    if let InputMode::ConfirmDeleteBranch(ref branch) = app.input_mode {
        draw_confirm_delete_branch_overlay(f, area, branch);
        return;
    }

    // Check for confirm delete worktree sessions overlay
    if let InputMode::ConfirmDeleteWorktreeSessions {
        ref branch,
        session_count,
        ..
    } = app.input_mode
    {
        draw_confirm_delete_worktree_sessions_overlay(f, area, branch, session_count);
        return;
    }

    // Check for add line comment overlay
    if let InputMode::AddLineComment {
        ref file_path,
        line_number,
        ..
    } = app.input_mode
    {
        draw_add_line_comment_overlay(f, area, app, file_path, line_number);
        return;
    }

    // Check for edit line comment overlay
    if let InputMode::EditLineComment {
        ref file_path,
        line_number,
        ..
    } = app.input_mode
    {
        draw_edit_line_comment_overlay(f, area, app, file_path, line_number);
        return;
    }

    // Check for TODO popup
    if app.input_mode == InputMode::TodoPopup {
        draw_todo_popup(f, area, app);
        return;
    }

    // Check for add TODO overlay
    if matches!(app.input_mode, InputMode::AddTodo { .. }) {
        draw_add_todo_overlay(f, area, app);
        return;
    }

    // Check for edit TODO overlay
    if matches!(app.input_mode, InputMode::EditTodo { .. }) {
        draw_edit_todo_overlay(f, area, app);
        return;
    }

    // Check for edit TODO description overlay
    if matches!(app.input_mode, InputMode::EditTodoDescription { .. }) {
        draw_edit_todo_description_overlay(f, area, app);
        return;
    }

    // Check for confirm delete TODO overlay
    if let InputMode::ConfirmDeleteTodo { ref title, .. } = app.input_mode {
        draw_confirm_delete_todo_overlay(f, area, title);
        return;
    }

    // Fullscreen terminal mode
    if app.terminal_fullscreen && app.focus == Focus::Terminal {
        draw_terminal_fullscreen(f, area, app);
        return;
    }

    // Fullscreen diff mode
    if app.diff_fullscreen && app.focus == Focus::DiffFiles {
        draw_diff_fullscreen(f, area, app);
        return;
    }

    // Split into sidebar and main content
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25), // Sidebar
            Constraint::Percentage(75), // Main content (Terminal or Diff)
        ])
        .split(area);

    draw_sidebar(f, chunks[0], app);

    // Draw right panel based on view mode
    match app.right_panel_view {
        RightPanelView::Terminal => draw_terminal(f, chunks[1], app),
        RightPanelView::Diff => draw_diff_view(f, chunks[1], app),
    }
}

/// Draw sidebar with worktrees and sessions
fn draw_sidebar(f: &mut Frame, area: Rect, app: &App) {
    if app.tree_view_enabled {
        // Tree view with git status panel
        if app.git_panel_enabled {
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
fn draw_sidebar_tree(f: &mut Frame, area: Rect, app: &App) {
    let is_focused = app.focus == Focus::Sidebar;
    let border_style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let mut items: Vec<ListItem> = Vec::new();
    let mut cursor_pos = 0;

    for (wt_idx, wt) in app.worktrees.iter().enumerate() {
        let is_expanded = app.expanded_worktrees.contains(&wt_idx);
        let is_cursor = cursor_pos == app.sidebar_cursor;

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
        let session_count = app
            .sessions_by_worktree
            .get(&wt_idx)
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
            if let Some(sessions) = app.sessions_by_worktree.get(&wt_idx) {
                for session in sessions.iter() {
                    let is_session_cursor = cursor_pos == app.sidebar_cursor;
                    let is_active = app.active_session_id.as_ref() == Some(&session.id);

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
fn draw_worktrees(f: &mut Frame, area: Rect, app: &App) {
    let is_focused = app.focus == Focus::Branches;
    let border_style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let items: Vec<ListItem> = app
        .worktrees
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
fn draw_sessions(f: &mut Frame, area: Rect, app: &App) {
    let is_focused = app.focus == Focus::Sessions;
    let border_style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let current_branch = app
        .worktrees
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

/// Draw git status panel
fn draw_git_status_panel(f: &mut Frame, area: Rect, app: &App) {
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
            .git_status_files
            .iter()
            .enumerate()
            .filter(|(_, f)| f.section == section)
            .collect();

        if files.is_empty() {
            continue;
        }

        let is_expanded = app.expanded_git_sections.contains(&section);
        let is_cursor = cursor_pos == app.git_status_cursor;

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
                let is_file_cursor = cursor_pos == app.git_status_cursor;

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

    let total_files = app.git_status_files.len();
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

/// Draw diff view with inline expansion
fn draw_diff_view(f: &mut Frame, area: Rect, app: &App) {
    draw_diff_inline(f, area, app);
}

/// Draw fullscreen diff view
fn draw_diff_fullscreen(f: &mut Frame, area: Rect, app: &App) {
    draw_diff_inline(f, area, app);
}

/// Draw diff with inline file expansion (unified navigation view)
fn draw_diff_inline(f: &mut Frame, area: Rect, app: &App) {
    use super::state::DiffItem;

    let is_focused = app.focus == Focus::DiffFiles;
    let border_style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let title = if is_focused {
        format!(" Changes ({}) [*] ", app.diff_files.len())
    } else {
        format!(" Changes ({}) ", app.diff_files.len())
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(title);

    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.diff_files.is_empty() {
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

    for (file_idx, file) in app.diff_files.iter().enumerate() {
        let is_file_selected = current_item == DiffItem::File(file_idx);
        let is_expanded = app.diff_expanded.contains(&file_idx);

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
            if let Some(file_lines) = app.diff_file_lines.get(&file_idx) {
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
    let cursor_line = app.diff_cursor;

    // Calculate scroll offset to keep cursor visible
    let scroll_offset = if cursor_line < app.diff_scroll_offset {
        cursor_line
    } else if cursor_line >= app.diff_scroll_offset + visible_height {
        cursor_line.saturating_sub(visible_height / 2)
    } else {
        app.diff_scroll_offset
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

/// Draw input overlay for new branch
fn draw_input_overlay(f: &mut Frame, area: Rect, app: &App) {
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
fn draw_rename_session_overlay(f: &mut Frame, area: Rect, app: &App) {
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
fn draw_confirm_delete_overlay(f: &mut Frame, area: Rect, target: &DeleteTarget) {
    // Center the confirm box
    let popup_width = 50.min(area.width.saturating_sub(4));
    let popup_height = 5;
    let x = (area.width.saturating_sub(popup_width)) / 2 + area.x;
    let y = (area.height.saturating_sub(popup_height)) / 2 + area.y;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    // Build message based on target
    let (title, message) = match target {
        DeleteTarget::Worktree { branch, .. } => (
            " Delete Worktree ",
            format!("Delete worktree '{}'?", branch),
        ),
        DeleteTarget::Session { name, .. } => {
            (" Delete Session ", format!("Delete session '{}'?", name))
        }
    };

    let text = vec![
        Line::from(message),
        Line::from(""),
        Line::from(vec![
            Span::styled("[y/Enter]", Style::default().fg(Color::Green)),
            Span::raw(" Yes  "),
            Span::styled("[n/Esc]", Style::default().fg(Color::Red)),
            Span::raw(" No"),
        ]),
    ];

    let confirm = Paragraph::new(text)
        .alignment(ratatui::layout::Alignment::Center)
        .style(Style::default().bg(Color::Black))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Red).bg(Color::Black))
                .style(Style::default().bg(Color::Black))
                .title(title),
        );
    f.render_widget(confirm, popup_area);
}

/// Draw add worktree overlay (select branch or type new name)
fn draw_add_worktree_overlay(f: &mut Frame, area: Rect, app: &App, base_branch: Option<&str>) {
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
fn draw_confirm_delete_branch_overlay(f: &mut Frame, area: Rect, branch: &str) {
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
fn draw_confirm_delete_worktree_sessions_overlay(
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
fn draw_add_line_comment_overlay(
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
fn draw_edit_line_comment_overlay(
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

/// Draw status bar at the bottom
fn draw_status_bar(f: &mut Frame, area: Rect, app: &App) {
    // Prefix mode takes priority - show available commands
    if app.prefix_mode == PrefixMode::WaitingForCommand {
        let prefix_help = "Prefix: [b] Branches | [s] Sessions | [t] Terminal | [[] Normal | [n] New | [a] Add | [d] Delete | [r] Refresh | [f] Fullscreen | [1-9] Repo | [q] Quit";
        let paragraph = Paragraph::new(prefix_help)
            .style(
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            )
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Magenta)),
            );
        f.render_widget(paragraph, area);
        return;
    }

    let (message, color) = if let Some(err) = &app.error_message {
        (err.clone(), Color::Red)
    } else if let Some(status) = &app.status_message {
        (status.clone(), Color::Green)
    } else {
        let help = match app.focus {
            Focus::Sidebar => "[Ctrl+s] Prefix | [j/k] Move | [o/Enter] Expand | [g] Git | [Tab] Terminal | [n] New | [a] Add | [R] Rename | [x] Delete | [d] Diff | [q] Quit",
            Focus::GitStatus => "[j/k] Move | [o] Expand | [s] Stage | [u] Unstage | [S] Stage All | [U] Unstage All | [r] Refresh | [Tab] Diff | [Esc] Back",
            Focus::Branches => "[Ctrl+s] Prefix | [1-9] Repo | [Tab] Sessions | [j/k] Move | [a] Add | [x] Delete | [d] Diff | [q] Quit",
            Focus::Sessions => "[Ctrl+s] Prefix | [Tab] Terminal | [j/k] Move | [Enter] Terminal | [n] New | [R] Rename | [x] Delete | [d] Diff | [q] Quit",
            Focus::Terminal => match app.terminal_mode {
                TerminalMode::Normal => "[Ctrl+s] Prefix | [j/k] Scroll | [Ctrl+d/u] Page | [G/g] Top/Bottom | [i] Insert | [f] Fullscreen | [d] Diff | [Esc] Exit",
                TerminalMode::Insert => "[Esc] Normal mode | Keys sent to terminal",
            },
            Focus::DiffFiles => "[j/k] Nav | [o] Expand | [c] Add | [C] Edit | [x] Del | [n/N] Jump | [S] Send | [Esc] Back",
        };
        (help.to_string(), Color::DarkGray)
    };

    let paragraph = Paragraph::new(message)
        .style(Style::default().fg(color))
        .block(Block::default().borders(Borders::ALL));

    f.render_widget(paragraph, area);
}

// ============ TODO Rendering ============

/// Draw TODO popup (main TODO list)
fn draw_todo_popup(f: &mut Frame, area: Rect, app: &App) {
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
    let title = if app.todo_show_completed {
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
    // Calculate depth for each item
    fn calculate_depth(items: &[TodoItem], item_idx: usize) -> usize {
        let item = &items[item_idx];
        if let Some(ref parent_id) = item.parent_id {
            if let Some(parent_idx) = items.iter().position(|i| &i.id == parent_id) {
                return 1 + calculate_depth(items, parent_idx);
            }
        }
        0
    }

    // Convert to ListItems with indentation
    let items: Vec<ListItem> = app
        .todo_display_order
        .iter()
        .enumerate()
        .map(|(display_idx, &item_idx)| {
            let item = &app.todo_items[item_idx];
            let checkbox = if item.completed { "[x]" } else { "[ ]" };
            let depth = calculate_depth(&app.todo_items, item_idx);
            let indent = "  ".repeat(depth);

            let style = if display_idx == app.todo_cursor {
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
fn draw_add_todo_overlay(f: &mut Frame, area: Rect, app: &App) {
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
fn draw_edit_todo_overlay(f: &mut Frame, area: Rect, app: &App) {
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
fn draw_edit_todo_description_overlay(f: &mut Frame, area: Rect, app: &App) {
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
fn draw_confirm_delete_todo_overlay(f: &mut Frame, area: Rect, title: &str) {
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
