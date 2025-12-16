//! Catppuccin Mocha theme system for amux TUI
//!
//! Provides a centralized color theme using the Catppuccin Mocha palette,
//! a soothing pastel theme that's easy on the eyes.
//!
//! Palette reference: https://catppuccin.com/palette/

use ratatui::style::{Color, Modifier, Style};

/// Catppuccin Mocha color theme - soothing pastel colors
#[derive(Debug, Clone)]
pub struct CyberpunkTheme {
    // Primary accent colors
    pub neon_cyan: Color,
    pub neon_magenta: Color,
    pub neon_yellow: Color,
    pub neon_green: Color,

    // UI semantic colors
    pub focus_border: Color,
    pub unfocus_border: Color,
    pub selection_fg: Color,

    // Status colors
    pub success: Color,
    pub error: Color,
    pub warning: Color,

    // Git status colors
    pub git_added: Color,
    pub git_modified: Color,
    pub git_deleted: Color,
    pub git_renamed: Color,
    pub git_untracked: Color,
    pub git_staged: Color,
    pub git_unstaged: Color,

    // Terminal mode colors
    pub terminal_insert: Color,
    pub terminal_normal: Color,

    // Background
    pub bg_level0: Color,

    // Text colors
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_tertiary: Color,
    pub text_disabled: Color,

    // Diff colors
    pub diff_add: Color,
    pub diff_del: Color,
    pub diff_hunk_header: Color,

    // Comment colors
    pub comment_border: Color,
    pub comment_path: Color,
    pub comment_line_no: Color,
}

impl Default for CyberpunkTheme {
    fn default() -> Self {
        Self::cyberpunk()
    }
}

impl CyberpunkTheme {
    /// Catppuccin Mocha theme - soothing pastel colors
    pub fn cyberpunk() -> Self {
        Self {
            // Catppuccin Mocha accent colors
            neon_cyan: Color::Rgb(148, 226, 213), // Teal #94e2d5
            neon_magenta: Color::Rgb(203, 166, 247), // Mauve #cba6f7
            neon_yellow: Color::Rgb(249, 226, 175), // Yellow #f9e2af
            neon_green: Color::Rgb(166, 227, 161), // Green #a6e3a1

            // UI semantic
            focus_border: Color::Rgb(180, 190, 254), // Lavender #b4befe
            unfocus_border: Color::Rgb(88, 91, 112), // Surface 2 #585b70
            selection_fg: Color::Rgb(205, 214, 244), // Text #cdd6f4

            // Status
            success: Color::Rgb(166, 227, 161), // Green #a6e3a1
            error: Color::Rgb(243, 139, 168),   // Red #f38ba8
            warning: Color::Rgb(249, 226, 175), // Yellow #f9e2af

            // Git status
            git_added: Color::Rgb(166, 227, 161), // Green #a6e3a1
            git_modified: Color::Rgb(249, 226, 175), // Yellow #f9e2af
            git_deleted: Color::Rgb(243, 139, 168), // Red #f38ba8
            git_renamed: Color::Rgb(148, 226, 213), // Teal #94e2d5
            git_untracked: Color::Rgb(203, 166, 247), // Mauve #cba6f7
            git_staged: Color::Rgb(166, 227, 161), // Green #a6e3a1
            git_unstaged: Color::Rgb(250, 179, 135), // Peach #fab387

            // Terminal modes
            terminal_insert: Color::Rgb(166, 227, 161), // Green #a6e3a1
            terminal_normal: Color::Rgb(249, 226, 175), // Yellow #f9e2af

            // Background (Catppuccin Mocha base)
            bg_level0: Color::Rgb(30, 30, 46), // Base #1e1e2e

            // Text hierarchy (Catppuccin Mocha text colors)
            text_primary: Color::Rgb(205, 214, 244), // Text #cdd6f4
            text_secondary: Color::Rgb(186, 194, 222), // Subtext 1 #bac2de
            text_tertiary: Color::Rgb(127, 132, 156), // Overlay 1 #7f849c
            text_disabled: Color::Rgb(108, 112, 134), // Overlay 0 #6c7086

            // Diff colors
            diff_add: Color::Rgb(166, 227, 161), // Green #a6e3a1
            diff_del: Color::Rgb(243, 139, 168), // Red #f38ba8
            diff_hunk_header: Color::Rgb(137, 180, 250), // Blue #89b4fa

            // Comment colors
            comment_border: Color::Rgb(88, 91, 112), // Surface 2 #585b70
            comment_path: Color::Rgb(148, 226, 213), // Teal #94e2d5
            comment_line_no: Color::Rgb(249, 226, 175), // Yellow #f9e2af
        }
    }

    // ========== Style Helpers ==========

    /// Style for focused panel border
    pub fn focused_border_style(&self) -> Style {
        Style::default()
            .fg(self.focus_border)
            .add_modifier(Modifier::BOLD)
    }

    /// Style for unfocused panel border
    pub fn unfocused_border_style(&self) -> Style {
        Style::default().fg(self.unfocus_border)
    }

    /// Style for selected item (cursor on it, panel focused)
    pub fn selection_style(&self) -> Style {
        Style::default()
            .fg(self.selection_fg)
            .add_modifier(Modifier::BOLD)
    }

    /// Style for selected item when panel is not focused
    pub fn selection_unfocused_style(&self) -> Style {
        Style::default().fg(self.text_primary)
    }

    /// Style for normal (non-selected) items
    pub fn normal_style(&self) -> Style {
        Style::default().fg(self.text_disabled)
    }

    // ========== Git Status Styles ==========

    /// Get color for git file status
    pub fn git_status_color(&self, status: GitFileStatus) -> Color {
        match status {
            GitFileStatus::Added => self.git_added,
            GitFileStatus::Modified => self.git_modified,
            GitFileStatus::Deleted => self.git_deleted,
            GitFileStatus::Renamed => self.git_renamed,
            GitFileStatus::Untracked => self.git_untracked,
            GitFileStatus::Unknown => self.text_disabled,
        }
    }

    /// Get color for git section
    pub fn git_section_color(&self, section: GitSection) -> Color {
        match section {
            GitSection::Staged => self.git_staged,
            GitSection::Unstaged => self.git_unstaged,
            GitSection::Untracked => self.git_untracked,
        }
    }

    // ========== Terminal Mode Styles ==========

    /// Border style for terminal based on mode
    pub fn terminal_border_style(&self, mode: TerminalMode, is_focused: bool) -> Style {
        if !is_focused {
            return self.unfocused_border_style();
        }
        match mode {
            TerminalMode::Insert => Style::default().fg(self.terminal_insert),
            TerminalMode::Normal => Style::default().fg(self.terminal_normal),
        }
    }

    // ========== Diff Styles ==========

    /// Style for diff addition line
    pub fn diff_add_style(&self) -> Style {
        Style::default().fg(self.diff_add)
    }

    /// Style for diff deletion line
    pub fn diff_del_style(&self) -> Style {
        Style::default().fg(self.diff_del)
    }

    /// Style for diff hunk header
    pub fn diff_hunk_style(&self) -> Style {
        Style::default()
            .fg(self.diff_hunk_header)
            .add_modifier(Modifier::BOLD)
    }
}

/// Git file status for color mapping
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitFileStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Untracked,
    Unknown,
}

/// Git section type for color mapping
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitSection {
    Staged,
    Unstaged,
    Untracked,
}

/// Terminal mode for border color
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalMode {
    Insert,
    Normal,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_default() {
        let theme = CyberpunkTheme::default();
        // Catppuccin Mocha Teal #94e2d5
        assert_eq!(theme.neon_cyan, Color::Rgb(148, 226, 213));
    }
}
