//! Virtual list navigation trait and utilities
//!
//! Provides a common trait for virtual list navigation, eliminating code duplication
//! across different UI components (sidebar, diff, git status, todo).

/// Trait for components that implement virtual list navigation
///
/// Virtual lists support navigating through a variable-length list with a cursor.
/// This trait provides common navigation operations to avoid duplication.
///
/// This trait is used throughout the codebase to implement unified navigation logic,
/// reducing code duplication across different UI components (sidebar, diff, git status, todo).
///
/// Note: Some trait methods (page_up, page_down, is_at_top, is_at_bottom, scroll_offset, clamp_cursor)
/// are not currently used but are included for completeness and future use cases. They are marked
/// with #[allow(dead_code)] to suppress warnings while maintaining a cohesive trait interface.
#[allow(dead_code)]
pub trait VirtualList {
    /// Get the total length of the virtual list
    fn virtual_len(&self) -> usize;

    /// Get the current cursor position
    fn cursor(&self) -> usize;

    /// Set the cursor to a specific position
    fn set_cursor(&mut self, pos: usize);

    /// Move cursor up by one position, wrapping to bottom if at top
    ///
    /// Returns true if the cursor moved
    fn move_up(&mut self) -> bool {
        let len = self.virtual_len();
        if len == 0 {
            return false;
        }
        let current = self.cursor();
        if current > 0 {
            self.set_cursor(current - 1);
        } else {
            // Wrap to bottom
            self.set_cursor(len.saturating_sub(1));
        }
        true
    }

    /// Move cursor down by one position, wrapping to top if at bottom
    ///
    /// Returns true if the cursor moved
    fn move_down(&mut self) -> bool {
        let len = self.virtual_len();
        if len == 0 {
            return false;
        }
        let max = len.saturating_sub(1);
        let current = self.cursor();
        if current < max {
            self.set_cursor(current + 1);
        } else {
            // Wrap to top
            self.set_cursor(0);
        }
        true
    }

    /// Move to the top of the list
    fn goto_top(&mut self) {
        self.set_cursor(0);
    }

    /// Move to the bottom of the list
    fn goto_bottom(&mut self) {
        let max = self.virtual_len().saturating_sub(1);
        self.set_cursor(max);
    }

    /// Page up by a given number of lines
    /// Returns true if moved the full distance, false if hit the top
    fn page_up(&mut self, lines: usize) -> bool {
        let current = self.cursor();
        let new_pos = current.saturating_sub(lines);
        self.set_cursor(new_pos);
        // Return true only if we moved the full distance
        current >= lines
    }

    /// Page down by a given number of lines
    /// Returns true if moved the full distance, false if hit the bottom
    fn page_down(&mut self, lines: usize) -> bool {
        let current = self.cursor();
        let max = self.virtual_len().saturating_sub(1);
        let new_pos = (current + lines).min(max);
        self.set_cursor(new_pos);
        // Return true only if we moved the full distance
        new_pos == current + lines
    }

    /// Check if cursor is at the top
    fn is_at_top(&self) -> bool {
        self.cursor() == 0
    }

    /// Check if cursor is at the bottom
    fn is_at_bottom(&self) -> bool {
        self.cursor() >= self.virtual_len().saturating_sub(1)
    }

    /// Get the scroll offset for rendering (where to start showing items)
    ///
    /// This is useful for calculating which items should be visible in a scrollable area.
    /// Given a viewport height, it returns the index of the first visible item.
    fn scroll_offset(&self, viewport_height: usize) -> usize {
        let cursor = self.cursor();
        let len = self.virtual_len();

        if len <= viewport_height {
            // All items fit, no scrolling needed
            0
        } else if cursor < viewport_height / 2 {
            // Near the top, show from start
            0
        } else if cursor >= len - viewport_height / 2 {
            // Near the bottom, show to end
            len.saturating_sub(viewport_height)
        } else {
            // In the middle, center the cursor
            cursor.saturating_sub(viewport_height / 2)
        }
    }

    /// Ensure cursor is within valid bounds
    ///
    /// Useful after the list changes size
    fn clamp_cursor(&mut self) {
        let max = self.virtual_len().saturating_sub(1);
        if self.cursor() > max {
            self.set_cursor(max);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Simple test implementation
    struct MockList {
        len: usize,
        cursor: usize,
    }

    impl VirtualList for MockList {
        fn virtual_len(&self) -> usize {
            self.len
        }

        fn cursor(&self) -> usize {
            self.cursor
        }

        fn set_cursor(&mut self, pos: usize) {
            self.cursor = pos.min(self.len.saturating_sub(1));
        }
    }

    #[test]
    fn test_move_up() {
        let mut list = MockList { len: 10, cursor: 5 };
        assert!(list.move_up());
        assert_eq!(list.cursor(), 4);

        // Wraps to bottom when at top
        let mut list = MockList { len: 10, cursor: 0 };
        assert!(list.move_up());
        assert_eq!(list.cursor(), 9);
    }

    #[test]
    fn test_move_down() {
        let mut list = MockList { len: 10, cursor: 5 };
        assert!(list.move_down());
        assert_eq!(list.cursor(), 6);

        // Wraps to top when at bottom
        let mut list = MockList { len: 10, cursor: 9 };
        assert!(list.move_down());
        assert_eq!(list.cursor(), 0);
    }

    #[test]
    fn test_goto_top_bottom() {
        let mut list = MockList { len: 10, cursor: 5 };
        list.goto_top();
        assert_eq!(list.cursor(), 0);

        list.goto_bottom();
        assert_eq!(list.cursor(), 9);
    }

    #[test]
    fn test_page_up_down() {
        let mut list = MockList {
            len: 100,
            cursor: 50,
        };

        assert!(list.page_up(10));
        assert_eq!(list.cursor(), 40);

        assert!(list.page_down(10));
        assert_eq!(list.cursor(), 50);

        list.set_cursor(5);
        assert!(!list.page_up(10));
        assert_eq!(list.cursor(), 0);
    }

    #[test]
    fn test_is_at_top_bottom() {
        let mut list = MockList { len: 10, cursor: 0 };
        assert!(list.is_at_top());
        assert!(!list.is_at_bottom());

        list.set_cursor(9);
        assert!(!list.is_at_top());
        assert!(list.is_at_bottom());
    }

    #[test]
    fn test_scroll_offset() {
        let list = MockList {
            len: 100,
            cursor: 50,
        };

        // Cursor at 50, viewport 20, should center around cursor
        let offset = list.scroll_offset(20);
        assert_eq!(offset, 40);

        // Near top
        let list = MockList {
            len: 100,
            cursor: 5,
        };
        let offset = list.scroll_offset(20);
        assert_eq!(offset, 0);

        // Near bottom
        let list = MockList {
            len: 100,
            cursor: 95,
        };
        let offset = list.scroll_offset(20);
        assert_eq!(offset, 80);

        // List smaller than viewport
        let list = MockList { len: 10, cursor: 5 };
        let offset = list.scroll_offset(20);
        assert_eq!(offset, 0);
    }

    #[test]
    fn test_clamp_cursor() {
        let mut list = MockList { len: 10, cursor: 5 };
        list.clamp_cursor();
        assert_eq!(list.cursor(), 5);

        // Simulate list shrinking
        list.len = 3;
        list.clamp_cursor();
        assert_eq!(list.cursor(), 2); // Clamped to max valid index
    }

    #[test]
    fn test_empty_list() {
        let mut list = MockList { len: 0, cursor: 0 };
        assert!(!list.move_down());
        assert!(!list.move_up());
        assert!(list.is_at_top());
        assert!(list.is_at_bottom());
        assert_eq!(list.scroll_offset(10), 0);
    }
}
