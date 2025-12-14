//! Tab bar and status bar rendering

use crate::tui::app::App;
use crate::tui::state::{Focus, InputMode, PrefixMode, TerminalMode};
use amux_config::actions::Action;
use amux_config::keybind::BindingContext;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Tabs},
    Frame,
};

/// Draw repo tabs at the top
pub fn draw_tab_bar(f: &mut Frame, area: Rect, app: &App) {
    let titles: Vec<Line> = app
        .repos()
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
                .title(" Amux - Claude Code Manager "),
        )
        .select(app.repo_idx())
        .style(Style::default().fg(Color::White))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .divider(" | ");

    f.render_widget(tabs, area);
}

/// Helper to format key binding for display
fn key(app: &App, action: Action, context: BindingContext) -> String {
    app.keybinds.key_display(action, context)
}

/// Draw status bar at the bottom
pub fn draw_status_bar(f: &mut Frame, area: Rect, app: &App) {
    // Prefix mode takes priority - show available commands
    if app.prefix_mode == PrefixMode::WaitingForCommand {
        let ctx = BindingContext::Prefix;
        let prefix_help = format!(
            "Prefix: {} Branches | {} Sessions | {} Terminal | {} Normal | {} New | {} Add | {} Delete | {} Refresh | {} Fullscreen | [1-9] Repo | {} Quit",
            key(app, Action::FocusBranches, ctx),
            key(app, Action::FocusSessions, ctx),
            key(app, Action::FocusTerminal, ctx),
            key(app, Action::NormalMode, ctx),
            key(app, Action::CreateSession, ctx),
            key(app, Action::AddWorktree, ctx),
            key(app, Action::DeleteCurrent, ctx),
            key(app, Action::RefreshAll, ctx),
            key(app, Action::ToggleFullscreen, ctx),
            key(app, Action::Quit, ctx),
        );
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

    // CreateSessionInput mode - show input prompt in status bar
    if let InputMode::CreateSessionInput { .. } = &app.input_mode {
        // Use Unicode-safe methods to get cursor-split text
        let before = app.text_input.text_before_cursor();
        let at_cursor = app.text_input.char_at_cursor();
        let after = app.text_input.text_after_cursor();

        // Build the prompt with cursor indicator
        let prompt = "Session name (Enter=create, Esc=cancel): ";
        let line = Line::from(vec![
            Span::styled(prompt, Style::default().fg(Color::Cyan)),
            Span::styled(before, Style::default().fg(Color::White)),
            Span::styled(
                at_cursor.map(|c| c.to_string()).unwrap_or_else(|| " ".to_string()),
                Style::default().fg(Color::Black).bg(Color::White),
            ),
            Span::styled(after, Style::default().fg(Color::White)),
        ]);

        let paragraph = Paragraph::new(line).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
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
            Focus::Sidebar => {
                let ctx = BindingContext::Sidebar;
                format!(
                    "{} Prefix | {} Move | {} Expand | {} Git | {} Term | {} New | {} Add | {} Rename | {} Del | {} Diff | {} Quit",
                    app.keybinds.prefix_key_display(),
                    format!("{}/{}", key(app, Action::MoveUp, ctx), key(app, Action::MoveDown, ctx)).replace("[]", ""),
                    key(app, Action::ToggleExpand, ctx),
                    key(app, Action::FocusGitStatus, ctx),
                    key(app, Action::FocusTerminal, ctx),
                    key(app, Action::CreateSession, ctx),
                    key(app, Action::AddWorktree, ctx),
                    key(app, Action::RenameSession, ctx),
                    key(app, Action::DeleteCurrent, ctx),
                    key(app, Action::ToggleDiffView, ctx),
                    key(app, Action::Quit, ctx),
                )
            },
            Focus::GitStatus => {
                let ctx = BindingContext::GitStatus;
                format!(
                    "{} Move | {} Expand | {} Stage | {} Unstage | {} Stage All | {} Unstage All | {} Refresh | {} Diff | {} Back",
                    format!("{}/{}", key(app, Action::MoveUp, ctx), key(app, Action::MoveDown, ctx)).replace("[]", ""),
                    key(app, Action::ToggleExpand, ctx),
                    key(app, Action::StageFile, ctx),
                    key(app, Action::UnstageFile, ctx),
                    key(app, Action::StageAll, ctx),
                    key(app, Action::UnstageAll, ctx),
                    key(app, Action::RefreshStatus, ctx),
                    key(app, Action::ToggleDiffView, ctx),
                    key(app, Action::FocusSidebar, ctx),
                )
            },
            Focus::Terminal => match app.terminal.mode {
                TerminalMode::Normal => {
                    let ctx = BindingContext::TerminalNormal;
                    format!(
                        "{} Prefix | {} Scroll | {} Page | {} Top/Bottom | {} Insert | {} Full | {} Diff | {} Exit",
                        app.keybinds.prefix_key_display(),
                        format!("{}/{}", key(app, Action::ScrollUp, ctx), key(app, Action::ScrollDown, ctx)).replace("[]", ""),
                        format!("{}/{}", key(app, Action::ScrollHalfPageUp, ctx), key(app, Action::ScrollHalfPageDown, ctx)).replace("[]", ""),
                        format!("{}/{}", key(app, Action::ScrollTop, ctx), key(app, Action::ScrollBottom, ctx)).replace("[]", ""),
                        key(app, Action::InsertMode, ctx),
                        key(app, Action::ToggleFullscreen, ctx),
                        key(app, Action::ToggleDiffView, ctx),
                        key(app, Action::ExitTerminal, ctx),
                    )
                },
                TerminalMode::Insert => {
                    let ctx = BindingContext::TerminalInsert;
                    format!(
                        "{} Normal mode | Keys sent to terminal",
                        key(app, Action::NormalMode, ctx),
                    )
                },
            },
            Focus::DiffFiles => {
                let ctx = BindingContext::Diff;
                format!(
                    "{} Nav | {} Expand | {} Add | {} Edit | {} Del | {} Jump | {} Send | {} Back",
                    format!("{}/{}", key(app, Action::MoveUp, ctx), key(app, Action::MoveDown, ctx)).replace("[]", ""),
                    key(app, Action::ToggleExpand, ctx),
                    key(app, Action::AddComment, ctx),
                    key(app, Action::EditComment, ctx),
                    key(app, Action::DeleteComment, ctx),
                    format!("{}/{}", key(app, Action::NextComment, ctx), key(app, Action::PrevComment, ctx)).replace("[]", ""),
                    key(app, Action::SubmitReviewClaude, ctx),
                    key(app, Action::BackToTerminal, ctx),
                )
            },
        };
        (help, Color::DarkGray)
    };

    let paragraph = Paragraph::new(message)
        .style(Style::default().fg(color))
        .block(Block::default().borders(Borders::ALL));

    f.render_widget(paragraph, area);
}
