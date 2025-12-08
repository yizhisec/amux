//! Terminal rendering with inline PseudoTerminal widget
//!
//! This module provides terminal rendering using a simple inline widget
//! that converts vt100::Screen to ratatui widgets, similar to tui-term crate.

use super::super::app::App;
use super::super::state::{Focus, TerminalMode};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Paragraph, Widget},
    Frame,
};

/// Simple PseudoTerminal widget that renders vt100::Screen
struct PseudoTerminal<'a> {
    screen: &'a vt100::Screen,
}

impl<'a> PseudoTerminal<'a> {
    fn new(screen: &'a vt100::Screen) -> Self {
        Self { screen }
    }
}

impl Widget for PseudoTerminal<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        for row in 0..area.height {
            for col in 0..area.width {
                if let Some(cell) = self.screen.cell(row, col) {
                    let ch = cell.contents();
                    let display_char = if ch.is_empty() { " " } else { ch };

                    let mut style = Style::default();

                    // Apply foreground color
                    let fg = cell.fgcolor();
                    if fg != vt100::Color::Default {
                        style = style.fg(vt100_color_to_ratatui(fg));
                    }

                    // Apply background color
                    let bg = cell.bgcolor();
                    if bg != vt100::Color::Default {
                        style = style.bg(vt100_color_to_ratatui(bg));
                    }

                    // Apply modifiers
                    if cell.bold() {
                        style = style.add_modifier(Modifier::BOLD);
                    }
                    if cell.italic() {
                        style = style.add_modifier(Modifier::ITALIC);
                    }
                    if cell.underline() {
                        style = style.add_modifier(Modifier::UNDERLINED);
                    }
                    if cell.inverse() {
                        style = style.add_modifier(Modifier::REVERSED);
                    }

                    let x = area.x + col;
                    let y = area.y + row;
                    if x < area.x + area.width && y < area.y + area.height {
                        buf[(x, y)].set_symbol(display_char).set_style(style);
                    }
                }
            }
        }
    }
}

/// Convert vt100 color to ratatui color
fn vt100_color_to_ratatui(color: vt100::Color) -> Color {
    match color {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(idx) => Color::Indexed(idx),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

/// Draw terminal preview/interaction area
pub fn draw_terminal(f: &mut Frame, area: Rect, app: &App) {
    let is_terminal_focused = app.focus == Focus::Terminal;
    let border_color = if is_terminal_focused {
        match app.terminal.mode {
            TerminalMode::Insert => Color::Green,
            TerminalMode::Normal => Color::Yellow,
        }
    } else {
        Color::DarkGray
    };

    let title = if is_terminal_focused {
        match app.terminal.mode {
            TerminalMode::Insert => " Terminal [INSERT] ",
            TerminalMode::Normal => " Terminal [NORMAL] ",
        }
    } else if app.terminal.active_session_id.is_some() {
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

    // Render terminal content using PseudoTerminal widget
    if app.terminal.active_session_id.is_some() {
        if let Ok(parser) = app.terminal.parser.lock() {
            let pseudo_term = PseudoTerminal::new(parser.screen());
            f.render_widget(pseudo_term, inner);
        }
    } else {
        // Show placeholder
        let placeholder = Paragraph::new("Select a session to see terminal output")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(ratatui::layout::Alignment::Center);
        f.render_widget(placeholder, inner);
    }
}

/// Draw fullscreen terminal
pub fn draw_terminal_fullscreen(f: &mut Frame, area: Rect, app: &App) {
    let border_color = match app.terminal.mode {
        TerminalMode::Insert => Color::Green,
        TerminalMode::Normal => Color::Yellow,
    };

    let title = match app.terminal.mode {
        TerminalMode::Insert => " Terminal [INSERT - FULLSCREEN] ",
        TerminalMode::Normal => " Terminal [NORMAL - FULLSCREEN] ",
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(title);

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Render terminal content using PseudoTerminal widget
    if let Ok(parser) = app.terminal.parser.lock() {
        let pseudo_term = PseudoTerminal::new(parser.screen());
        f.render_widget(pseudo_term, inner);
    }
}
