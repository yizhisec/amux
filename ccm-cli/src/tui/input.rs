//! Input handling - navigation mode vs terminal Normal/Insert modes
//!
//! Supports both direct keybindings and prefix key mode (Ctrl+s as prefix).
//! Prefix mode allows access to navigation commands from any context.
//!
//! All input handlers are synchronous and return Option<AsyncAction> for deferred execution.

use super::app::{App, AsyncAction, Focus, InputMode, PrefixMode, RightPanelView, TerminalMode};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};

/// Handle keyboard input (sync version - returns async action if needed)
pub fn handle_input_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    // Check for prefix key (Ctrl+s) - works in any context except text input
    if is_prefix_key(&key) && !is_text_input_mode(app) {
        app.prefix_mode = PrefixMode::WaitingForCommand;
        return None;
    }

    // Handle prefix mode commands
    if app.prefix_mode == PrefixMode::WaitingForCommand {
        return handle_prefix_command_sync(app, key);
    }

    // Handle input mode (new branch name entry)
    if app.input_mode == InputMode::NewBranch {
        return handle_input_mode_sync(app, key);
    }

    // Handle add worktree mode
    if matches!(app.input_mode, InputMode::AddWorktree { .. }) {
        return handle_add_worktree_mode_sync(app, key);
    }

    // Handle rename session mode
    if matches!(app.input_mode, InputMode::RenameSession { .. }) {
        return handle_rename_session_mode_sync(app, key);
    }

    // Handle confirm delete mode
    if matches!(app.input_mode, InputMode::ConfirmDelete(_)) {
        return handle_confirm_delete_sync(app, key);
    }

    // Handle confirm delete branch mode
    if matches!(app.input_mode, InputMode::ConfirmDeleteBranch(_)) {
        return handle_confirm_delete_branch_sync(app, key);
    }

    // Handle confirm delete worktree sessions mode
    if matches!(
        app.input_mode,
        InputMode::ConfirmDeleteWorktreeSessions { .. }
    ) {
        return handle_confirm_delete_worktree_sessions_sync(app, key);
    }

    // Handle add line comment mode
    if matches!(app.input_mode, InputMode::AddLineComment { .. }) {
        return handle_add_line_comment_mode_sync(app, key);
    }

    // Handle terminal modes when focused on terminal
    if app.focus == Focus::Terminal {
        return match app.terminal_mode {
            TerminalMode::Insert => handle_insert_mode_sync(app, key),
            TerminalMode::Normal => {
                handle_terminal_normal_mode_sync(app, key);
                None
            }
        };
    }

    // Handle diff view mode
    if app.focus == Focus::DiffFiles {
        return handle_diff_files_mode_sync(app, key);
    }

    // Handle sidebar navigation
    handle_navigation_input_sync(app, key)
}

/// Check if key is the prefix key (Ctrl+s)
fn is_prefix_key(key: &KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s')
}

/// Check if we're in a text input mode where prefix key shouldn't work
fn is_text_input_mode(app: &App) -> bool {
    matches!(
        app.input_mode,
        InputMode::NewBranch | InputMode::AddWorktree { .. } | InputMode::RenameSession { .. }
    )
}

/// Handle commands after prefix key (Ctrl+s + ?)
fn handle_prefix_command_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    // Reset prefix mode first
    app.prefix_mode = PrefixMode::None;

    // Esc cancels prefix mode
    if key.code == KeyCode::Esc {
        return None;
    }

    match key.code {
        // Navigation: b = go to Branches, s = go to Sessions
        KeyCode::Char('b') => {
            // Exit terminal if needed
            if app.focus == Focus::Terminal {
                app.exit_terminal();
            }
            app.focus = Focus::Branches;
            None
        }
        KeyCode::Char('s') => {
            // Exit terminal if needed
            if app.focus == Focus::Terminal {
                app.exit_terminal();
            }
            // Go to sidebar (tree view) or sessions (legacy view)
            app.focus = if app.tree_view_enabled {
                Focus::Sidebar
            } else {
                Focus::Sessions
            };
            None
        }

        // Terminal: t = go to Terminal (enter insert mode)
        KeyCode::Char('t') => {
            if app.active_session_id.is_some() {
                Some(AsyncAction::ConnectStream)
            } else {
                None
            }
        }

        // Actions: n = new (session/worktree based on context)
        KeyCode::Char('n') => {
            // Exit terminal first if needed
            if app.focus == Focus::Terminal {
                app.exit_terminal();
            }
            Some(AsyncAction::CreateSession)
        }

        // Actions: a = add worktree
        KeyCode::Char('a') => {
            if app.focus == Focus::Terminal {
                app.exit_terminal();
            }
            app.focus = Focus::Branches;
            app.start_add_worktree();
            None
        }

        // Actions: d = delete
        KeyCode::Char('d') => {
            if app.focus == Focus::Terminal {
                app.exit_terminal();
            }
            app.request_delete();
            None
        }

        // Actions: r = refresh
        KeyCode::Char('r') => Some(AsyncAction::RefreshAll),

        // Actions: f = toggle fullscreen (terminal)
        KeyCode::Char('f') | KeyCode::Char('z') => {
            if app.focus == Focus::Terminal || app.active_session_id.is_some() {
                app.toggle_fullscreen();
            }
            None
        }

        // Actions: [ = exit to terminal Normal mode (from Insert)
        KeyCode::Char('[') => {
            if app.focus == Focus::Terminal && app.terminal_mode == TerminalMode::Insert {
                app.terminal_mode = TerminalMode::Normal;
                app.dirty.terminal = true;
            }
            None
        }

        // Repo switching: 1-9
        KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
            let idx = (c as usize) - ('1' as usize);
            if app.focus == Focus::Terminal {
                app.exit_terminal();
            }
            app.switch_repo_sync(idx)
        }

        // Quit: q
        KeyCode::Char('q') => {
            app.should_quit = true;
            None
        }

        // Unknown command - show hint
        _ => {
            app.status_message = Some(
                "Prefix: b=branches s=sessions t=terminal [=normal n=new d=delete r=refresh q=quit"
                    .to_string(),
            );
            None
        }
    }
}

/// Handle input when in confirm delete mode
fn handle_confirm_delete_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    match key.code {
        // Confirm with y or Enter
        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
            Some(AsyncAction::ConfirmDelete)
        }
        // Cancel with n, N, or Esc
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            app.cancel_input();
            None
        }
        _ => None,
    }
}

/// Handle input when in confirm delete branch mode (after worktree deletion)
fn handle_confirm_delete_branch_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    match key.code {
        // Confirm with y - delete the branch
        KeyCode::Char('y') | KeyCode::Char('Y') => Some(AsyncAction::ConfirmDeleteBranch),
        // Cancel with n, N, or Esc - keep the branch
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            app.cancel_input();
            None
        }
        _ => None,
    }
}

/// Handle input when in confirm delete worktree sessions mode
fn handle_confirm_delete_worktree_sessions_sync(
    app: &mut App,
    key: KeyEvent,
) -> Option<AsyncAction> {
    match key.code {
        // Confirm with y or Enter - delete sessions and proceed to worktree deletion
        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
            Some(AsyncAction::ConfirmDeleteWorktreeSessions)
        }
        // Cancel with n, N, or Esc
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            app.cancel_input();
            None
        }
        _ => None,
    }
}

/// Handle input when in add worktree mode
fn handle_add_worktree_mode_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    // Shift+Enter: insert newline (when typing new branch name)
    if key.code == KeyCode::Enter && key.modifiers.contains(KeyModifiers::SHIFT) {
        app.input_buffer.push('\n');
        return None;
    }

    match key.code {
        // Cancel
        KeyCode::Esc => {
            app.cancel_input();
            None
        }
        // Confirm selection
        KeyCode::Enter => Some(AsyncAction::SubmitAddWorktree),
        // Navigate up in branch list (clear input buffer if typing)
        KeyCode::Up | KeyCode::Char('k') if app.input_buffer.is_empty() => {
            if app.add_worktree_idx > 0 {
                app.add_worktree_idx -= 1;
            }
            None
        }
        // Navigate down in branch list
        KeyCode::Down | KeyCode::Char('j') if app.input_buffer.is_empty() => {
            if app.add_worktree_idx + 1 < app.available_branches.len() {
                app.add_worktree_idx += 1;
            }
            None
        }
        // Backspace - delete character or if empty, go back to list selection
        KeyCode::Backspace => {
            app.input_buffer.pop();
            None
        }
        // Type character - switch to new branch input mode
        KeyCode::Char(c) => {
            app.input_buffer.push(c);
            None
        }
        _ => None,
    }
}

/// Handle input when in rename session mode
fn handle_rename_session_mode_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    match (key.code, key.modifiers) {
        // Shift+Enter: insert newline
        (KeyCode::Enter, m) if m.contains(KeyModifiers::SHIFT) => {
            app.input_buffer.push('\n');
            None
        }
        (KeyCode::Enter, _) => Some(AsyncAction::SubmitRenameSession),
        (KeyCode::Esc, _) => {
            app.cancel_input();
            None
        }
        (KeyCode::Backspace, _) => {
            app.input_buffer.pop();
            None
        }
        (KeyCode::Char(c), _) => {
            app.input_buffer.push(c);
            None
        }
        _ => None,
    }
}

/// Handle input when adding a line comment
fn handle_add_line_comment_mode_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    match (key.code, key.modifiers) {
        // Shift+Enter: insert newline
        (KeyCode::Enter, m) if m.contains(KeyModifiers::SHIFT) => {
            app.input_buffer.push('\n');
            None
        }
        (KeyCode::Enter, _) => Some(AsyncAction::SubmitLineComment),
        (KeyCode::Esc, _) => {
            app.cancel_input();
            None
        }
        (KeyCode::Backspace, _) => {
            app.input_buffer.pop();
            None
        }
        (KeyCode::Char(c), _) => {
            app.input_buffer.push(c);
            None
        }
        _ => None,
    }
}

/// Handle input when in text entry mode (new branch name)
fn handle_input_mode_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    match (key.code, key.modifiers) {
        // Shift+Enter: insert newline
        (KeyCode::Enter, m) if m.contains(KeyModifiers::SHIFT) => {
            app.input_buffer.push('\n');
            None
        }
        (KeyCode::Enter, _) => Some(AsyncAction::SubmitInput),
        (KeyCode::Esc, _) => {
            app.cancel_input();
            None
        }
        (KeyCode::Backspace, _) => {
            app.input_buffer.pop();
            None
        }
        (KeyCode::Char(c), _) => {
            app.input_buffer.push(c);
            None
        }
        _ => None,
    }
}

/// Handle input in terminal Insert mode (send to PTY)
fn handle_insert_mode_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    // Esc exits to Normal mode
    if key.code == KeyCode::Esc {
        app.exit_to_normal_mode();
        return None;
    }

    // Convert key to bytes and send to terminal
    let data = key_to_bytes(&key);
    if !data.is_empty() {
        Some(AsyncAction::SendToTerminal { data })
    } else {
        None
    }
}

/// Handle input in terminal Normal mode (scroll/browse)
fn handle_terminal_normal_mode_sync(app: &mut App, key: KeyEvent) {
    match key.code {
        // Toggle fullscreen
        KeyCode::Char('f') | KeyCode::Char('z') => {
            app.toggle_fullscreen();
        }

        // Exit to Sessions (or exit fullscreen first)
        KeyCode::Esc | KeyCode::BackTab => {
            app.exit_terminal();
        }

        // Enter Insert mode
        KeyCode::Char('i') | KeyCode::Enter => {
            app.enter_insert_mode();
        }

        // Scroll up (show older content)
        KeyCode::Char('k') | KeyCode::Up => {
            app.scroll_up(1);
        }

        // Scroll down (show newer content)
        KeyCode::Char('j') | KeyCode::Down => {
            app.scroll_down(1);
        }

        // Half page up (Ctrl+u or u)
        KeyCode::Char('u') => {
            app.scroll_up(10);
        }

        // Half page down (Ctrl+d or d)
        KeyCode::Char('d') => {
            app.scroll_down(10);
        }

        // Go to bottom
        KeyCode::Char('G') => {
            app.scroll_to_bottom();
        }

        // Go to top (gg - for simplicity, just 'g' works too)
        KeyCode::Char('g') => {
            app.scroll_to_top();
        }

        _ => {}
    }
}

/// Handle input in navigation mode (sidebar)
fn handle_navigation_input_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    use super::app::SidebarItem;

    // Clear status messages on any key press
    app.status_message = None;

    match key.code {
        // Repo switching (1-9)
        KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
            let idx = (c as usize) - ('1' as usize);
            app.switch_repo_sync(idx)
        }

        // Tab: forward navigation (Sidebar/Branches -> Sessions -> Terminal Normal)
        KeyCode::Tab => {
            match app.focus {
                Focus::Sidebar => {
                    // In tree view: Enter terminal if on a session, else do nothing
                    if let SidebarItem::Session(_, _) = app.current_sidebar_item() {
                        if app.active_session_id.is_some() {
                            return Some(AsyncAction::ConnectStream);
                        }
                    }
                }
                Focus::Branches => {
                    app.focus = Focus::Sessions;
                }
                Focus::Sessions => {
                    // Enter terminal Normal mode if session is active
                    if app.active_session_id.is_some() {
                        return Some(AsyncAction::ConnectStream);
                    }
                }
                Focus::Terminal | Focus::DiffFiles => {
                    // Shouldn't happen here
                }
            }
            None
        }

        // Esc/Shift+Tab: backward navigation
        KeyCode::Esc | KeyCode::BackTab => {
            match app.focus {
                Focus::Sidebar => {
                    // Already at the beginning in tree view
                }
                Focus::Branches => {
                    // Already at the beginning
                }
                Focus::Sessions => {
                    app.focus = Focus::Branches;
                }
                Focus::Terminal | Focus::DiffFiles => {
                    // Handled in their respective modes
                }
            }
            None
        }

        // Navigation
        KeyCode::Up | KeyCode::Char('k') => app.select_prev_sync(),
        KeyCode::Down | KeyCode::Char('j') => app.select_next_sync(),

        // Enter: forward navigation or toggle expand
        KeyCode::Enter => match app.focus {
            Focus::Sidebar => {
                match app.current_sidebar_item() {
                    SidebarItem::Worktree(_) => {
                        // Toggle expand when on worktree
                        app.toggle_sidebar_expand()
                    }
                    SidebarItem::Session(_, _) => {
                        // Enter terminal when on session
                        if app.active_session_id.is_some() {
                            Some(AsyncAction::ConnectStream)
                        } else {
                            None
                        }
                    }
                    SidebarItem::None => None,
                }
            }
            Focus::Branches => {
                app.focus = Focus::Sessions;
                None
            }
            Focus::Sessions => {
                if app.active_session_id.is_some() {
                    Some(AsyncAction::ConnectStream)
                } else {
                    None
                }
            }
            Focus::Terminal | Focus::DiffFiles => None,
        },

        // Toggle expand in tree view (o key)
        KeyCode::Char('o') if app.focus == Focus::Sidebar => app.toggle_sidebar_expand(),

        // Toggle tree view mode (T key)
        KeyCode::Char('T') => {
            app.toggle_tree_view();
            None
        }

        // Create new (n for sessions, a for worktrees)
        KeyCode::Char('n') => Some(AsyncAction::CreateSession),

        // Add worktree (when in Branches or Sidebar focus)
        KeyCode::Char('a') if app.focus == Focus::Branches || app.focus == Focus::Sidebar => {
            app.start_add_worktree();
            None
        }

        // Switch to diff view
        KeyCode::Char('d') => {
            if app.right_panel_view == RightPanelView::Diff {
                // Already in diff view, switch back to terminal
                app.switch_to_terminal_view();
                None
            } else {
                // Switch to diff view
                Some(AsyncAction::SwitchToDiffView)
            }
        }

        // Delete (with confirmation) - use 'x' key
        KeyCode::Char('x') => {
            app.request_delete();
            None
        }

        // Rename session (R when in Sessions or Sidebar focus with session selected)
        KeyCode::Char('R') if app.focus == Focus::Sessions => {
            app.start_rename_session();
            None
        }
        KeyCode::Char('R') if app.focus == Focus::Sidebar => {
            if let SidebarItem::Session(_, _) = app.current_sidebar_item() {
                app.start_rename_session();
            }
            None
        }

        // Refresh (lowercase r)
        KeyCode::Char('r') => Some(AsyncAction::RefreshAll),

        // Quit
        KeyCode::Char('q') => {
            app.should_quit = true;
            None
        }

        _ => None,
    }
}

/// Handle input in DiffFiles mode (unified file + line navigation)
fn handle_diff_files_mode_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    match key.code {
        // Navigate up (files and lines)
        KeyCode::Up | KeyCode::Char('k') => {
            app.diff_move_up();
            None
        }

        // Navigate down (files and lines)
        KeyCode::Down | KeyCode::Char('j') => {
            app.diff_move_down();
            None
        }

        // Jump to previous file
        KeyCode::Char('{') => {
            app.diff_prev_file();
            None
        }

        // Jump to next file
        KeyCode::Char('}') => {
            app.diff_next_file();
            None
        }

        // Toggle expand/collapse file diff ('o' or Enter)
        KeyCode::Enter | KeyCode::Char('o') => app.toggle_diff_expand(),

        // Add comment on current line
        KeyCode::Char('c') => {
            app.start_add_line_comment();
            None
        }

        // Submit review to Claude
        KeyCode::Char('S') => Some(AsyncAction::SubmitReviewToClaude),

        // Refresh diff
        KeyCode::Char('r') => Some(AsyncAction::LoadDiffFiles),

        // Toggle fullscreen
        KeyCode::Char('f') | KeyCode::Char('z') => {
            app.toggle_diff_fullscreen();
            None
        }

        // Back to terminal view
        KeyCode::Esc | KeyCode::Char('t') => {
            app.switch_to_terminal_view();
            None
        }

        _ => None,
    }
}

/// Convert a key event to bytes to send to PTY
fn key_to_bytes(key: &KeyEvent) -> Vec<u8> {
    use KeyCode::*;

    if key.modifiers.contains(KeyModifiers::CONTROL) {
        if let Char(c) = key.code {
            // Ctrl+A = 0x01, Ctrl+B = 0x02, etc.
            let ctrl_char = (c.to_ascii_lowercase() as u8).wrapping_sub(b'a' - 1);
            return vec![ctrl_char];
        }
    }

    // Shift+Enter sends newline (for multi-line input in Claude Code)
    if key.code == Enter && key.modifiers.contains(KeyModifiers::SHIFT) {
        return vec![b'\n'];
    }

    match key.code {
        Char(c) => c.to_string().into_bytes(),
        Enter => vec![b'\r'],
        Tab => vec![b'\t'],
        BackTab => b"\x1b[Z".to_vec(), // Shift+Tab escape sequence
        Backspace => vec![0x7f],
        Esc => vec![0x1b],
        Up => b"\x1b[A".to_vec(),
        Down => b"\x1b[B".to_vec(),
        Right => b"\x1b[C".to_vec(),
        Left => b"\x1b[D".to_vec(),
        Home => b"\x1b[H".to_vec(),
        End => b"\x1b[F".to_vec(),
        PageUp => b"\x1b[5~".to_vec(),
        PageDown => b"\x1b[6~".to_vec(),
        Delete => b"\x1b[3~".to_vec(),
        Insert => b"\x1b[2~".to_vec(),
        F(n) => match n {
            1 => b"\x1bOP".to_vec(),
            2 => b"\x1bOQ".to_vec(),
            3 => b"\x1bOR".to_vec(),
            4 => b"\x1bOS".to_vec(),
            5 => b"\x1b[15~".to_vec(),
            6 => b"\x1b[17~".to_vec(),
            7 => b"\x1b[18~".to_vec(),
            8 => b"\x1b[19~".to_vec(),
            9 => b"\x1b[20~".to_vec(),
            10 => b"\x1b[21~".to_vec(),
            11 => b"\x1b[23~".to_vec(),
            12 => b"\x1b[24~".to_vec(),
            _ => vec![],
        },
        _ => vec![],
    }
}

/// Handle mouse events (sync version)
pub fn handle_mouse_sync(app: &mut App, mouse: MouseEvent) {
    match mouse.kind {
        MouseEventKind::ScrollUp => {
            match app.focus {
                Focus::Terminal if app.terminal_mode == TerminalMode::Normal => {
                    app.scroll_up(3);
                }
                Focus::DiffFiles => {
                    // Scroll up 3 lines in diff view
                    for _ in 0..3 {
                        app.diff_move_up();
                    }
                }
                Focus::Sidebar => {
                    // Scroll up in tree view
                    for _ in 0..3 {
                        let _ = app.sidebar_move_up();
                    }
                }
                Focus::Branches | Focus::Sessions => {
                    // Scroll up in legacy sidebar lists
                    for _ in 0..3 {
                        let _ = app.select_prev_sync();
                    }
                }
                _ => {}
            }
            app.dirty.sidebar = true;
            app.dirty.terminal = true;
        }
        MouseEventKind::ScrollDown => {
            match app.focus {
                Focus::Terminal if app.terminal_mode == TerminalMode::Normal => {
                    app.scroll_down(3);
                }
                Focus::DiffFiles => {
                    // Scroll down 3 lines in diff view
                    for _ in 0..3 {
                        app.diff_move_down();
                    }
                }
                Focus::Sidebar => {
                    // Scroll down in tree view
                    for _ in 0..3 {
                        let _ = app.sidebar_move_down();
                    }
                }
                Focus::Branches | Focus::Sessions => {
                    // Scroll down in legacy sidebar lists
                    for _ in 0..3 {
                        let _ = app.select_next_sync();
                    }
                }
                _ => {}
            }
            app.dirty.sidebar = true;
            app.dirty.terminal = true;
        }
        _ => {}
    }
}
