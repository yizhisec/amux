//! Mouse event handling

use super::super::app::App;
use super::super::state::{Focus, RightPanelView};
use crossterm::event::{MouseEvent, MouseEventKind};

/// Handle mouse events (sync version)
/// Uses mouse position to determine which area to scroll
pub fn handle_mouse_sync(app: &mut App, mouse: MouseEvent) {
    // Determine which area the mouse is over based on x position
    // Layout: 25% sidebar (left), 75% main content (right)
    // We use a simple heuristic: x < 25% of terminal width = sidebar
    let terminal_width = app.terminal.cols.unwrap_or(80);
    let sidebar_width = terminal_width / 4; // ~25%
    let in_sidebar = mouse.column < sidebar_width;

    match mouse.kind {
        MouseEventKind::ScrollUp => {
            if in_sidebar {
                // Scroll sidebar
                for _ in 0..3 {
                    let _ = app.sidebar_move_up();
                }
                app.dirty.sidebar = true;
            } else {
                // Scroll main content area (terminal or diff)
                match app.right_panel_view {
                    RightPanelView::Terminal => {
                        app.scroll_up(3);
                    }
                    RightPanelView::Diff => {
                        for _ in 0..3 {
                            app.diff_move_up();
                        }
                    }
                }
            }
        }
        MouseEventKind::ScrollDown => {
            if in_sidebar {
                // Scroll sidebar
                for _ in 0..3 {
                    let _ = app.sidebar_move_down();
                }
                app.dirty.sidebar = true;
            } else {
                // Scroll main content area
                match app.right_panel_view {
                    RightPanelView::Terminal => {
                        app.scroll_down(3);
                    }
                    RightPanelView::Diff => {
                        for _ in 0..3 {
                            app.diff_move_down();
                        }
                    }
                }
            }
        }
        MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
            // Click to focus: left side = sidebar, right side = terminal/diff
            if in_sidebar {
                app.focus = Focus::Sidebar;
            } else {
                // Click on right panel
                match app.right_panel_view {
                    RightPanelView::Terminal => {
                        app.focus = Focus::Terminal;
                    }
                    RightPanelView::Diff => {
                        app.focus = Focus::DiffFiles;
                    }
                }
            }
            app.dirty.sidebar = true;
        }
        _ => {}
    }
}
