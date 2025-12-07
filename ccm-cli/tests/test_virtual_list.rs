//! Integration tests for VirtualList trait

// Note: This is an integration test, so we need to test through the public API
// Since VirtualList is not exposed in the public API of ccm-cli (it's only used internally),
// we'll create mock implementations here to verify the trait behavior.

/// Mock list implementation for testing
struct MockList {
    len: usize,
    cursor: usize,
}

/// VirtualList trait definition (copied for testing)
trait VirtualList {
    fn virtual_len(&self) -> usize;
    fn cursor(&self) -> usize;
    fn set_cursor(&mut self, pos: usize);

    fn move_up(&mut self) -> bool {
        if self.cursor() > 0 {
            self.set_cursor(self.cursor() - 1);
            true
        } else {
            false
        }
    }

    fn move_down(&mut self) -> bool {
        let max = self.virtual_len().saturating_sub(1);
        if self.cursor() < max {
            self.set_cursor(self.cursor() + 1);
            true
        } else {
            false
        }
    }

    fn goto_top(&mut self) {
        self.set_cursor(0);
    }

    fn goto_bottom(&mut self) {
        let max = self.virtual_len().saturating_sub(1);
        self.set_cursor(max);
    }

    fn page_up(&mut self, lines: usize) -> bool {
        let current = self.cursor();
        if current > 0 {
            let new_pos = current.saturating_sub(lines);
            self.set_cursor(new_pos);
            true
        } else {
            false
        }
    }

    fn page_down(&mut self, lines: usize) -> bool {
        let current = self.cursor();
        let max = self.virtual_len().saturating_sub(1);
        if current < max {
            let new_pos = (current + lines).min(max);
            self.set_cursor(new_pos);
            true
        } else {
            false
        }
    }

    fn is_at_top(&self) -> bool {
        self.cursor() == 0
    }

    fn is_at_bottom(&self) -> bool {
        self.cursor() >= self.virtual_len().saturating_sub(1)
    }

    fn clamp_cursor(&mut self) {
        let max = self.virtual_len().saturating_sub(1);
        if self.cursor() > max {
            self.set_cursor(max);
        }
    }
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

    let mut list = MockList { len: 10, cursor: 0 };
    assert!(!list.move_up());
    assert_eq!(list.cursor(), 0);
}

#[test]
fn test_move_down() {
    let mut list = MockList { len: 10, cursor: 5 };
    assert!(list.move_down());
    assert_eq!(list.cursor(), 6);

    let mut list = MockList { len: 10, cursor: 9 };
    assert!(!list.move_down());
    assert_eq!(list.cursor(), 9);
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
    // At position 5, can still page up to 0 (5 - 10 = saturating_sub -> 0)
    assert!(list.page_up(10)); // This should return true because cursor moved
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
}

#[test]
fn test_single_item_list() {
    let mut list = MockList { len: 1, cursor: 0 };
    assert!(!list.move_down());
    assert!(!list.move_up());
    assert!(list.is_at_top());
    assert!(list.is_at_bottom());
}

#[test]
fn test_navigation_boundaries() {
    let mut list = MockList { len: 5, cursor: 2 };

    // Move to extremes
    list.goto_top();
    assert_eq!(list.cursor(), 0);
    assert!(list.is_at_top());

    list.goto_bottom();
    assert_eq!(list.cursor(), 4);
    assert!(list.is_at_bottom());

    // Stay at boundaries
    assert!(!list.move_down());
    assert_eq!(list.cursor(), 4);

    list.goto_top();
    assert!(!list.move_up());
    assert_eq!(list.cursor(), 0);
}

#[test]
fn test_large_list_pagination() {
    let mut list = MockList {
        len: 1000,
        cursor: 500,
    };

    // Page down
    assert!(list.page_down(100));
    assert_eq!(list.cursor(), 600);

    // Page up
    assert!(list.page_up(100));
    assert_eq!(list.cursor(), 500);

    // Page past boundaries
    list.goto_bottom();
    assert!(!list.page_down(100));
    assert_eq!(list.cursor(), 999);

    list.goto_top();
    assert!(!list.page_up(100));
    assert_eq!(list.cursor(), 0);
}
