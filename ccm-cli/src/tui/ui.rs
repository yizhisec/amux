//! TUI rendering - Tab + Sidebar + Terminal layout

use super::app::{
    App, DeleteTarget, Focus, GitSection, InputMode, PrefixMode, RightPanelView, TerminalMode,
};
use ccm_proto::daemon::{DiffLine, FileStatus, LineType};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Tabs},
    Frame,
};

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

/// Render a diff line with word-level highlighting
fn render_word_diff_line<'a>(
    content: &str,
    paired_content: Option<&str>,
    is_addition: bool,
    is_selected: bool,
    is_focused: bool,
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

            // Build spans with highlighting
            let mut spans = Vec::new();
            let mut first = true;

            for token in tokens {
                if !first {
                    spans.push(Span::raw(" "));
                }
                first = false;

                match token {
                    DiffToken::Same(word) => {
                        let style = if is_selected && is_focused {
                            Style::default()
                                .fg(base_color)
                                .add_modifier(Modifier::REVERSED)
                        } else {
                            Style::default().fg(base_color)
                        };
                        spans.push(Span::styled(word, style));
                    }
                    DiffToken::Changed(word) => {
                        let style = if is_selected && is_focused {
                            Style::default()
                                .fg(highlight_color)
                                .add_modifier(Modifier::BOLD | Modifier::REVERSED)
                        } else {
                            Style::default()
                                .fg(highlight_color)
                                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
                        };
                        spans.push(Span::styled(word, style));
                    }
                }
            }

            spans
        }
        None => {
            // No paired line, render normally
            let style = if is_selected && is_focused {
                Style::default()
                    .fg(base_color)
                    .add_modifier(Modifier::REVERSED)
            } else {
                Style::default().fg(base_color)
            };
            vec![Span::styled(content.to_string(), style)]
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

                items.push(ListItem::new(Line::from(vec![
                    Span::styled(if is_file_cursor { ">" } else { " " }, file_style),
                    Span::raw("   "), // Indent
                    Span::styled(
                        format!("{} ", status_char),
                        Style::default().fg(status_color),
                    ),
                    Span::styled(&file.path, file_style),
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
    use super::app::DiffItem;

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
                    let has_comment = app.has_line_comment(&file.path, line_number);
                    let comment_marker = if has_comment {
                        Span::styled(" [C]", Style::default().fg(Color::Yellow))
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
                            );
                            line_spans.extend(content_spans);
                        }
                        LineType::Context | LineType::Unspecified => {
                            let style = if is_line_selected && is_focused {
                                Style::default()
                                    .fg(Color::White)
                                    .add_modifier(Modifier::REVERSED)
                            } else {
                                Style::default().fg(Color::White)
                            };
                            line_spans.push(Span::styled("  ", style));
                            line_spans.push(Span::styled(&diff_line.content, style));
                        }
                    }

                    line_spans.push(comment_marker);
                    lines.push(Line::from(line_spans));
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

/// Draw rename session overlay
fn draw_rename_session_overlay(f: &mut Frame, area: Rect, app: &App) {
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
                .border_style(Style::default().fg(Color::Green))
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

    // Clear background
    let clear = Block::default().style(Style::default().bg(Color::Black));
    f.render_widget(clear, area);

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
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Red))
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

    // Clear background
    let clear = Block::default().style(Style::default().bg(Color::Black));
    f.render_widget(clear, area);

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

    // Draw border
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Add Worktree (j/k=select, Enter=add, Esc=cancel) ");
    f.render_widget(block, popup_area);

    // Instructions
    let instructions = Paragraph::new("Select existing branch or type new name:")
        .style(Style::default().fg(Color::DarkGray));
    f.render_widget(instructions, chunks[0]);

    // Base branch info (only shown when typing new branch name)
    let base_info = match base_branch {
        Some(branch) => format!("Base: {} (new branch will be created from here)", branch),
        None => "Base: HEAD (new branch will be created from HEAD)".to_string(),
    };
    let base_style = if !app.input_buffer.is_empty() {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::DarkGray)
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
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                let prefix = if is_selected { "> " } else { "  " };
                ListItem::new(format!("{}○ {}", prefix, branch.branch)).style(style)
            })
            .collect();
        let list = List::new(items);
        f.render_widget(list, chunks[2]);
    } else {
        let empty = Paragraph::new("No available branches without worktree")
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(empty, chunks[2]);
    }

    // Input field
    let input_style = if !app.input_buffer.is_empty() {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
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

    // Clear background
    let clear = Block::default().style(Style::default().bg(Color::Black));
    f.render_widget(clear, area);

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
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow))
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

    // Clear background
    let clear = Block::default().style(Style::default().bg(Color::Black));
    f.render_widget(clear, area);

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
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow))
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

    // Clear background
    let clear = Block::default().style(Style::default().bg(Color::Black));
    f.render_widget(clear, area);

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

    let input = Paragraph::new(text).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" Add Comment (Enter=save, Shift+Enter=newline, Esc=cancel) "),
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
            Focus::DiffFiles => "[j/k] Nav | [o] Expand | [{/}] Files | [c] Comment | [S] Send | [r] Refresh | [Esc] Back",
        };
        (help.to_string(), Color::DarkGray)
    };

    let paragraph = Paragraph::new(message)
        .style(Style::default().fg(color))
        .block(Block::default().borders(Borders::ALL));

    f.render_widget(paragraph, area);
}
