//! Syntax highlighting module using syntect

use ratatui::style::{Color, Modifier, Style};
use std::path::Path;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style as SyntectStyle, ThemeSet};
use syntect::parsing::SyntaxSet;

/// Syntax highlighter using syntect
pub struct Highlighter {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
}

impl Default for Highlighter {
    fn default() -> Self {
        Self::new()
    }
}

impl Highlighter {
    /// Create a new highlighter with default syntaxes and themes
    pub fn new() -> Self {
        Self {
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set: ThemeSet::load_defaults(),
        }
    }

    /// Highlight a line of code based on file extension
    /// Returns a vector of (style, text) pairs
    pub fn highlight_line<'a>(&self, line: &'a str, file_path: &str) -> Vec<(Style, &'a str)> {
        // Get syntax from file extension
        let extension = Path::new(file_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let syntax = self
            .syntax_set
            .find_syntax_by_extension(extension)
            .or_else(|| self.syntax_set.find_syntax_by_first_line(line))
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        // Use base16-eighties.dark theme (good for terminals)
        let theme = &self.theme_set.themes["base16-eighties.dark"];

        let mut highlighter = HighlightLines::new(syntax, theme);

        match highlighter.highlight_line(line, &self.syntax_set) {
            Ok(ranges) => ranges
                .into_iter()
                .map(|(style, text)| (self.syntect_to_ratatui(style), text))
                .collect(),
            Err(_) => vec![(Style::default(), line)],
        }
    }

    /// Convert syntect style to ratatui style
    fn syntect_to_ratatui(&self, style: SyntectStyle) -> Style {
        let fg = style.foreground;
        let mut ratatui_style = Style::default().fg(Color::Rgb(fg.r, fg.g, fg.b));

        // Add modifiers based on font_style
        if style
            .font_style
            .contains(syntect::highlighting::FontStyle::BOLD)
        {
            ratatui_style = ratatui_style.add_modifier(Modifier::BOLD);
        }
        if style
            .font_style
            .contains(syntect::highlighting::FontStyle::ITALIC)
        {
            ratatui_style = ratatui_style.add_modifier(Modifier::ITALIC);
        }
        if style
            .font_style
            .contains(syntect::highlighting::FontStyle::UNDERLINE)
        {
            ratatui_style = ratatui_style.add_modifier(Modifier::UNDERLINED);
        }

        ratatui_style
    }

    /// Highlight a line with diff-specific styling overlay
    /// This applies syntax highlighting first, then overlays diff colors
    #[allow(dead_code)]
    pub fn highlight_diff_line<'a>(
        &self,
        line: &'a str,
        file_path: &str,
        is_addition: bool,
        is_selected: bool,
        is_focused: bool,
    ) -> Vec<(Style, &'a str)> {
        let highlighted = self.highlight_line(line, file_path);

        // Apply diff-specific overlay
        highlighted
            .into_iter()
            .map(|(style, text)| {
                let mut new_style = style;

                // Tint based on addition/deletion
                if is_addition {
                    // Green tint for additions - blend with existing color
                    if let Some(Color::Rgb(r, g, b)) = style.fg {
                        // Add green tint
                        let new_g = (g as u16 + 40).min(255) as u8;
                        new_style = new_style.fg(Color::Rgb(r, new_g, b));
                    } else {
                        new_style = new_style.fg(Color::LightGreen);
                    }
                } else {
                    // Red tint for deletions
                    if let Some(Color::Rgb(r, g, b)) = style.fg {
                        // Add red tint
                        let new_r = (r as u16 + 40).min(255) as u8;
                        new_style = new_style.fg(Color::Rgb(new_r, g, b));
                    } else {
                        new_style = new_style.fg(Color::LightRed);
                    }
                }

                // Apply selection styling
                if is_selected && is_focused {
                    new_style = new_style.add_modifier(Modifier::REVERSED);
                }

                (new_style, text)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highlighter_creation() {
        let _highlighter = Highlighter::new();
    }

    #[test]
    fn test_highlight_rust_line() {
        let highlighter = Highlighter::new();
        let result = highlighter.highlight_line("fn main() {}", "test.rs");
        assert!(!result.is_empty());
    }

    #[test]
    fn test_highlight_unknown_extension() {
        let highlighter = Highlighter::new();
        let result = highlighter.highlight_line("some text", "file.xyz");
        assert!(!result.is_empty());
    }
}
