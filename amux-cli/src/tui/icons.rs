//! Icons and symbols for TUI
//!
//! Provides Nerd Fonts icons and Unicode symbols with fallback support.
//! Also includes Box Drawing characters for enhanced borders.

/// Nerd Fonts icons (requires a Nerd Font installed in terminal)
pub mod nerd {
    // Git icons
    pub const GIT_BRANCH: &str = "\u{e0a0}"; //

    // Status icons
    pub const CHECK: &str = "\u{f00c}"; //
    pub const WARNING: &str = "\u{f071}"; //

    // Control icons
    pub const PLAY: &str = "\u{f04b}"; //
    pub const STOP: &str = "\u{f04d}"; //

    // Navigation icons
    pub const ARROW_RIGHT: &str = "\u{f061}"; //
    pub const CHEVRON_RIGHT: &str = "\u{f054}"; //
    pub const CHEVRON_DOWN: &str = "\u{f078}"; //

    // Shape icons
    pub const DIAMOND: &str = "\u{f219}"; //

    // Application icons
    pub const COMMENT: &str = "\u{f075}"; //
}

/// Unicode symbols (universal fallback, works in most terminals)
pub mod unicode {
    // Arrows and navigation
    pub const EXPAND: &str = "▶";
    pub const COLLAPSE: &str = "▼";

    // Shapes
    pub const DIAMOND_FILLED: &str = "◆";
    pub const DIAMOND_EMPTY: &str = "◇";
    pub const CIRCLE_FILLED: &str = "●";
    pub const CIRCLE_EMPTY: &str = "○";
    pub const TRIANGLE_RIGHT: &str = "▸";
}

/// Box Drawing characters for borders and frames
pub mod box_drawing {
    // Single line borders
    pub const HORIZONTAL: &str = "─";
    pub const VERTICAL: &str = "│";

    // Rounded corners
    pub const ROUND_TOP_LEFT: &str = "╭";
    pub const ROUND_BOTTOM_LEFT: &str = "╰";

    // Heavy (bold) borders
    pub const HEAVY_VERTICAL: &str = "┃";

    // Misc
    pub const MIDDOT: &str = "·";
}

/// Status icons with Nerd Fonts / Unicode fallback support
#[derive(Debug, Clone, Copy)]
pub struct StatusIcons {
    use_nerd_fonts: bool,
}

impl StatusIcons {
    pub fn new(use_nerd_fonts: bool) -> Self {
        Self { use_nerd_fonts }
    }

    /// Nerd Fonts enabled (richer icons)
    pub fn nerd() -> Self {
        Self::new(true)
    }

    // ===== Navigation =====

    pub fn expand(&self) -> &'static str {
        if self.use_nerd_fonts {
            nerd::CHEVRON_RIGHT
        } else {
            unicode::EXPAND
        }
    }

    pub fn collapse(&self) -> &'static str {
        if self.use_nerd_fonts {
            nerd::CHEVRON_DOWN
        } else {
            unicode::COLLAPSE
        }
    }

    // ===== Session status =====

    pub fn running(&self) -> &'static str {
        if self.use_nerd_fonts {
            nerd::PLAY
        } else {
            unicode::CIRCLE_FILLED
        }
    }

    pub fn stopped(&self) -> &'static str {
        if self.use_nerd_fonts {
            nerd::STOP
        } else {
            unicode::CIRCLE_EMPTY
        }
    }

    pub fn active_indicator(&self) -> &'static str {
        if self.use_nerd_fonts {
            nerd::ARROW_RIGHT
        } else {
            unicode::TRIANGLE_RIGHT
        }
    }

    // ===== Worktree indicators =====

    pub fn main_worktree(&self) -> &'static str {
        if self.use_nerd_fonts {
            nerd::DIAMOND
        } else {
            unicode::DIAMOND_FILLED
        }
    }

    pub fn worktree(&self) -> &'static str {
        if self.use_nerd_fonts {
            nerd::GIT_BRANCH
        } else {
            unicode::CIRCLE_FILLED
        }
    }

    // ===== Git status =====

    pub fn git_added(&self) -> &'static str {
        "A"
    }

    pub fn git_modified(&self) -> &'static str {
        "M"
    }

    pub fn git_deleted(&self) -> &'static str {
        "D"
    }

    pub fn git_renamed(&self) -> &'static str {
        "R"
    }

    pub fn git_untracked(&self) -> &'static str {
        "?"
    }

    pub fn staged_indicator(&self) -> &'static str {
        if self.use_nerd_fonts {
            nerd::CHECK
        } else {
            unicode::DIAMOND_FILLED
        }
    }

    pub fn unstaged_indicator(&self) -> &'static str {
        if self.use_nerd_fonts {
            nerd::WARNING
        } else {
            unicode::DIAMOND_EMPTY
        }
    }

    pub fn untracked_indicator(&self) -> &'static str {
        "?"
    }

    // ===== Comments =====

    pub fn comment(&self) -> &'static str {
        if self.use_nerd_fonts {
            nerd::COMMENT
        } else {
            "💬"
        }
    }

    // ===== Cursor =====

    pub fn cursor(&self) -> &'static str {
        " "
    }
}

impl Default for StatusIcons {
    fn default() -> Self {
        // Default to Nerd Fonts - users with proper fonts get better experience
        Self::nerd()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_icons_nerd() {
        let icons = StatusIcons::nerd();
        assert_eq!(icons.expand(), nerd::CHEVRON_RIGHT);
    }

    #[test]
    fn test_status_icons_unicode_fallback() {
        let icons = StatusIcons::new(false);
        assert_eq!(icons.expand(), unicode::EXPAND);
    }
}
