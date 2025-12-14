//! Word-level diff highlighting utilities

use super::super::highlight::Highlighter;
use amux_proto::daemon::{DiffLine, LineType};
use ratatui::{
    style::{Color, Modifier, Style},
    text::Span,
};
use std::sync::OnceLock;

/// Global highlighter instance (lazy initialized)
pub fn get_highlighter() -> &'static Highlighter {
    static HIGHLIGHTER: OnceLock<Highlighter> = OnceLock::new();
    HIGHLIGHTER.get_or_init(Highlighter::new)
}

/// Token for word diff
#[derive(Debug, Clone, PartialEq)]
pub enum DiffToken {
    Same(String),
    Changed(String),
}

/// Compute word-level diff between two strings
/// Returns tokens for the "new" side (addition line)
pub fn compute_word_diff(old_line: &str, new_line: &str) -> Vec<DiffToken> {
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
pub fn compute_lcs<'a>(a: &[&'a str], b: &[&'a str]) -> Vec<&'a str> {
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
pub fn find_paired_deletion(lines: &[DiffLine], addition_idx: usize) -> Option<usize> {
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
pub fn find_paired_addition(lines: &[DiffLine], deletion_idx: usize) -> Option<usize> {
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
pub fn render_word_diff_line<'a>(
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
pub fn find_syntax_style_for_range(
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
pub fn tint_color(color: Color, is_addition: bool) -> Color {
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
