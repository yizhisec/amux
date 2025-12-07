//! Tab bar and status bar rendering

use super::super::app::App;
use super::super::state::{Focus, PrefixMode, TerminalMode};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph, Tabs},
    Frame,
};

/// Draw repo tabs at the top
pub fn draw_tab_bar(f: &mut Frame, area: Rect, app: &App) {
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

/// Draw status bar at the bottom
pub fn draw_status_bar(f: &mut Frame, area: Rect, app: &App) {
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
            Focus::Terminal => match app.terminal.mode {
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
