//! Default configuration that matches current hardcoded behavior

use crate::types::{Bindings, Config, Options, PrefixConfig, UiConfig};
use std::collections::HashMap;

/// Get default configuration matching current ccman keybindings
pub fn default_config() -> Config {
    Config {
        prefix: PrefixConfig {
            key: "C-s".to_string(),
        },
        options: Options {
            tree_view_enabled: true,
            git_panel_enabled: true,
            mouse_enabled: false,
            fullscreen_on_connect: false,
            show_completed_todos: false,
        },
        ui: UiConfig {
            show_borders: true,
            sidebar_width: 30,
            terminal_scrollback: 10000,
        },
        bindings: default_bindings(),
        source: Vec::new(),
    }
}

/// Get default key bindings matching current ccman behavior
pub fn default_bindings() -> Bindings {
    Bindings {
        global: default_global_bindings(),
        prefix: default_prefix_bindings(),
        sidebar: default_sidebar_bindings(),
        terminal_normal: default_terminal_normal_bindings(),
        terminal_insert: default_terminal_insert_bindings(),
        diff: default_diff_bindings(),
        git_status: default_git_status_bindings(),
        todo: default_todo_bindings(),
        dialog_text: default_dialog_text_bindings(),
        dialog_confirm: default_dialog_confirm_bindings(),
    }
}

fn default_global_bindings() -> HashMap<String, String> {
    HashMap::new() // No global bindings by default
}

fn default_prefix_bindings() -> HashMap<String, String> {
    let mut map = HashMap::new();

    // From ccm-cli/src/tui/input/prefix.rs
    map.insert("b".to_string(), "focus-branches".to_string());
    map.insert("s".to_string(), "focus-sessions".to_string());
    map.insert("t".to_string(), "focus-terminal".to_string());
    map.insert("n".to_string(), "create-session".to_string());
    map.insert("a".to_string(), "add-worktree".to_string());
    map.insert("d".to_string(), "delete-current".to_string());
    map.insert("r".to_string(), "refresh-all".to_string());
    map.insert("f".to_string(), "toggle-fullscreen".to_string());
    map.insert("z".to_string(), "toggle-fullscreen".to_string()); // Alias
    map.insert("[".to_string(), "terminal-normal-mode".to_string());
    map.insert("w".to_string(), "focus-sidebar".to_string());
    map.insert("g".to_string(), "focus-git-status".to_string());
    map.insert("v".to_string(), "focus-diff".to_string());
    map.insert("o".to_string(), "open-todo".to_string());
    map.insert("q".to_string(), "quit".to_string());

    // Repo switching 1-9
    for i in 1..=9 {
        // Will be handled specially in keybind resolution
        map.insert(i.to_string(), format!("switch-repo-{}", i - 1));
    }

    map
}

fn default_sidebar_bindings() -> HashMap<String, String> {
    let mut map = HashMap::new();

    // From ccm-cli/src/tui/input/navigation.rs
    map.insert("1".to_string(), "switch-repo-0".to_string());
    map.insert("2".to_string(), "switch-repo-1".to_string());
    map.insert("3".to_string(), "switch-repo-2".to_string());
    map.insert("4".to_string(), "switch-repo-3".to_string());
    map.insert("5".to_string(), "switch-repo-4".to_string());
    map.insert("6".to_string(), "switch-repo-5".to_string());
    map.insert("7".to_string(), "switch-repo-6".to_string());
    map.insert("8".to_string(), "switch-repo-7".to_string());
    map.insert("9".to_string(), "switch-repo-8".to_string());

    map.insert("Tab".to_string(), "focus-next".to_string());
    map.insert("S-Tab".to_string(), "focus-prev".to_string());
    map.insert("Esc".to_string(), "focus-prev".to_string());

    map.insert("j".to_string(), "move-down".to_string());
    map.insert("Down".to_string(), "move-down".to_string());
    map.insert("k".to_string(), "move-up".to_string());
    map.insert("Up".to_string(), "move-up".to_string());

    map.insert("Enter".to_string(), "select".to_string());
    map.insert("o".to_string(), "toggle-expand".to_string());
    map.insert("T".to_string(), "toggle-tree-view".to_string());
    map.insert("g".to_string(), "focus-git-status".to_string());
    map.insert("n".to_string(), "create-session".to_string());
    map.insert("a".to_string(), "add-worktree".to_string());
    map.insert("d".to_string(), "delete-current".to_string());
    map.insert("x".to_string(), "delete-current".to_string());
    map.insert("R".to_string(), "rename-session".to_string());
    map.insert("r".to_string(), "refresh-all".to_string());
    map.insert("q".to_string(), "quit".to_string());

    // Diff toggle (from git_status and navigation)
    map.insert("t".to_string(), "toggle-diff-view".to_string());
    map.insert("d".to_string(), "toggle-diff-view".to_string()); // Alternate

    map
}

fn default_terminal_normal_bindings() -> HashMap<String, String> {
    let mut map = HashMap::new();

    // From ccm-cli/src/tui/input/terminal.rs - Normal mode
    map.insert("f".to_string(), "toggle-fullscreen".to_string());
    map.insert("z".to_string(), "toggle-fullscreen".to_string());
    map.insert("Esc".to_string(), "exit-fullscreen".to_string());

    map.insert("i".to_string(), "insert-mode".to_string());
    map.insert("Enter".to_string(), "insert-mode".to_string());

    map.insert("k".to_string(), "scroll-up".to_string());
    map.insert("Up".to_string(), "scroll-up".to_string());
    map.insert("j".to_string(), "scroll-down".to_string());
    map.insert("Down".to_string(), "scroll-down".to_string());

    map.insert("u".to_string(), "scroll-half-page-up".to_string());
    map.insert("d".to_string(), "scroll-half-page-down".to_string());

    map.insert("g".to_string(), "scroll-top".to_string());
    map.insert("G".to_string(), "scroll-bottom".to_string());

    map.insert("S-Tab".to_string(), "exit-terminal".to_string());

    map
}

fn default_terminal_insert_bindings() -> HashMap<String, String> {
    let mut map = HashMap::new();

    // From ccm-cli/src/tui/input/terminal.rs - Insert mode
    // Most keys are forwarded to PTY, but we intercept these special ones:
    map.insert("C-`".to_string(), "switch-to-shell".to_string());
    // Prefix key is handled separately (C-s) and always intercepted

    map
}

fn default_diff_bindings() -> HashMap<String, String> {
    let mut map = HashMap::new();

    // From ccm-cli/src/tui/input/diff.rs
    map.insert("j".to_string(), "move-down".to_string());
    map.insert("Down".to_string(), "move-down".to_string());
    map.insert("k".to_string(), "move-up".to_string());
    map.insert("Up".to_string(), "move-up".to_string());

    map.insert("{".to_string(), "prev-file".to_string());
    map.insert("}".to_string(), "next-file".to_string());

    map.insert("Enter".to_string(), "toggle-expand".to_string());
    map.insert("o".to_string(), "toggle-expand".to_string());

    map.insert("c".to_string(), "add-comment".to_string());
    map.insert("C".to_string(), "edit-comment".to_string());
    map.insert("x".to_string(), "delete-comment".to_string());

    map.insert("n".to_string(), "next-comment".to_string());
    map.insert("N".to_string(), "prev-comment".to_string());

    map.insert("S".to_string(), "submit-review-claude".to_string());

    map.insert("r".to_string(), "refresh-diff".to_string());

    map.insert("f".to_string(), "toggle-fullscreen".to_string());
    map.insert("z".to_string(), "toggle-fullscreen".to_string());

    map.insert("Esc".to_string(), "back-to-terminal".to_string());
    map.insert("q".to_string(), "back-to-terminal".to_string());
    map.insert("t".to_string(), "back-to-terminal".to_string());

    map
}

fn default_git_status_bindings() -> HashMap<String, String> {
    let mut map = HashMap::new();

    // From ccm-cli/src/tui/input/git_status.rs
    map.insert("j".to_string(), "move-down".to_string());
    map.insert("Down".to_string(), "move-down".to_string());
    map.insert("k".to_string(), "move-up".to_string());
    map.insert("Up".to_string(), "move-up".to_string());

    map.insert("Enter".to_string(), "toggle-or-open".to_string());
    map.insert("o".to_string(), "toggle-or-open".to_string());

    map.insert("s".to_string(), "stage-file".to_string());
    map.insert("u".to_string(), "unstage-file".to_string());
    map.insert("S".to_string(), "stage-all".to_string());
    map.insert("U".to_string(), "unstage-all".to_string());

    map.insert("r".to_string(), "refresh-status".to_string());

    map.insert("Tab".to_string(), "focus-diff".to_string());
    map.insert("Esc".to_string(), "focus-sidebar".to_string());
    map.insert("q".to_string(), "focus-sidebar".to_string());

    map
}

fn default_todo_bindings() -> HashMap<String, String> {
    let mut map = HashMap::new();

    // From ccm-cli/src/tui/input/todo.rs
    map.insert("j".to_string(), "move-down".to_string());
    map.insert("Down".to_string(), "move-down".to_string());
    map.insert("k".to_string(), "move-up".to_string());
    map.insert("Up".to_string(), "move-up".to_string());

    map.insert("g".to_string(), "goto-top".to_string());
    map.insert("G".to_string(), "goto-bottom".to_string());

    map.insert("Space".to_string(), "toggle-complete".to_string());

    map.insert("n".to_string(), "add-todo".to_string());
    map.insert("N".to_string(), "add-child-todo".to_string());

    map.insert("e".to_string(), "edit-title".to_string());
    map.insert("E".to_string(), "edit-description".to_string());

    map.insert("x".to_string(), "delete-todo".to_string());

    map.insert("J".to_string(), "move-todo-down".to_string());
    map.insert("K".to_string(), "move-todo-up".to_string());

    map.insert(">".to_string(), "indent-todo".to_string());
    map.insert("<".to_string(), "dedent-todo".to_string());

    map.insert("H".to_string(), "toggle-show-completed".to_string());

    map.insert("r".to_string(), "refresh-todos".to_string());

    map.insert("Esc".to_string(), "close-popup".to_string());
    map.insert("q".to_string(), "close-popup".to_string());

    map
}

fn default_dialog_text_bindings() -> HashMap<String, String> {
    let mut map = HashMap::new();

    // Text input dialogs
    map.insert("Enter".to_string(), "submit".to_string());
    map.insert("Esc".to_string(), "cancel".to_string());
    map.insert("S-Enter".to_string(), "insert-newline".to_string());
    // Character input is handled separately

    map
}

fn default_dialog_confirm_bindings() -> HashMap<String, String> {
    let mut map = HashMap::new();

    // Confirmation dialogs
    map.insert("y".to_string(), "confirm".to_string());
    map.insert("n".to_string(), "cancel".to_string());
    map.insert("Enter".to_string(), "confirm".to_string());
    map.insert("Esc".to_string(), "cancel".to_string());

    map
}
