//! TUI rendering - Tab + Sidebar + Terminal layout
//!
//! This module is organized into sub-modules by functional domain:
//! - `helpers`: Word-level diff utilities and syntax highlighting
//! - `tab_bar`: Tab bar and status bar rendering
//! - `sidebar`: Sidebar with worktrees and sessions
//! - `git_panel`: Git status panel
//! - `terminal`: Terminal preview/interaction area
//! - `diff`: Diff view with inline expansion
//! - `overlays`: All popup dialogs and overlays

mod diff;
mod git_panel;
mod helpers;
mod overlays;
mod sidebar;
mod tab_bar;
mod terminal;

use super::app::App;
use super::state::{Focus, InputMode, RightPanelView};
use diff::{draw_diff_fullscreen, draw_diff_view};
use overlays::{
    draw_add_line_comment_overlay, draw_add_todo_overlay, draw_add_worktree_overlay,
    draw_confirm_delete_branch_overlay, draw_confirm_delete_overlay,
    draw_confirm_delete_todo_overlay, draw_confirm_delete_worktree_sessions_overlay,
    draw_edit_line_comment_overlay, draw_edit_todo_description_overlay, draw_edit_todo_overlay,
    draw_input_overlay, draw_rename_session_overlay, draw_todo_popup,
};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    Frame,
};
use sidebar::draw_sidebar;
use tab_bar::{draw_status_bar, draw_tab_bar};
use terminal::{draw_terminal, draw_terminal_fullscreen};

/// Main draw function - entry point for TUI rendering
pub fn draw(f: &mut Frame, app: &App) {
    // Main layout: Tab bar + Main content + Status bar
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Tab bar
            Constraint::Min(0),    // Main content
            Constraint::Length(3), // Status bar
        ])
        .split(f.area());

    draw_tab_bar(f, chunks[0], app);
    draw_main_content(f, chunks[1], app);
    draw_status_bar(f, chunks[2], app);
}

/// Draw main content: Sidebar + Terminal/Diff with overlay handling
fn draw_main_content(f: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    // Check for input mode overlay
    if app.input_mode == InputMode::NewBranch {
        draw_input_overlay(f, area, app);
        return;
    }

    // Check for add worktree overlay
    if let InputMode::AddWorktree { ref base_branch } = app.input_mode {
        draw_add_worktree_overlay(f, area, app, base_branch.as_deref());
        return;
    }

    // Check for rename session overlay
    if matches!(app.input_mode, InputMode::RenameSession { .. }) {
        draw_rename_session_overlay(f, area, app);
        return;
    }

    // Check for confirm delete overlay
    if let InputMode::ConfirmDelete(ref target) = app.input_mode {
        draw_confirm_delete_overlay(f, area, target);
        return;
    }

    // Check for confirm delete branch overlay
    if let InputMode::ConfirmDeleteBranch(ref branch) = app.input_mode {
        draw_confirm_delete_branch_overlay(f, area, branch);
        return;
    }

    // Check for confirm delete worktree sessions overlay
    if let InputMode::ConfirmDeleteWorktreeSessions {
        ref branch,
        session_count,
        ..
    } = app.input_mode
    {
        draw_confirm_delete_worktree_sessions_overlay(f, area, branch, session_count);
        return;
    }

    // Check for add line comment overlay
    if let InputMode::AddLineComment {
        ref file_path,
        line_number,
        ..
    } = app.input_mode
    {
        draw_add_line_comment_overlay(f, area, app, file_path, line_number);
        return;
    }

    // Check for edit line comment overlay
    if let InputMode::EditLineComment {
        ref file_path,
        line_number,
        ..
    } = app.input_mode
    {
        draw_edit_line_comment_overlay(f, area, app, file_path, line_number);
        return;
    }

    // Check for TODO popup
    if app.input_mode == InputMode::TodoPopup {
        draw_todo_popup(f, area, app);
        return;
    }

    // Check for add TODO overlay
    if matches!(app.input_mode, InputMode::AddTodo { .. }) {
        draw_add_todo_overlay(f, area, app);
        return;
    }

    // Check for edit TODO overlay
    if matches!(app.input_mode, InputMode::EditTodo { .. }) {
        draw_edit_todo_overlay(f, area, app);
        return;
    }

    // Check for edit TODO description overlay
    if matches!(app.input_mode, InputMode::EditTodoDescription { .. }) {
        draw_edit_todo_description_overlay(f, area, app);
        return;
    }

    // Check for confirm delete TODO overlay
    if let InputMode::ConfirmDeleteTodo { ref title, .. } = app.input_mode {
        draw_confirm_delete_todo_overlay(f, area, title);
        return;
    }

    // Fullscreen terminal mode
    if app.terminal.fullscreen && app.focus == Focus::Terminal {
        draw_terminal_fullscreen(f, area, app);
        return;
    }

    // Fullscreen diff mode
    if app.diff.fullscreen && app.focus == Focus::DiffFiles {
        draw_diff_fullscreen(f, area, app);
        return;
    }

    // Split into sidebar and main content
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25), // Sidebar
            Constraint::Percentage(75), // Main content (Terminal or Diff)
        ])
        .split(area);

    draw_sidebar(f, chunks[0], app);

    // Draw right panel based on view mode
    match app.right_panel_view {
        RightPanelView::Terminal => draw_terminal(f, chunks[1], app),
        RightPanelView::Diff => draw_diff_view(f, chunks[1], app),
    }
}
