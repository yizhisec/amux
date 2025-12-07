# CCM Config - Configuration Management System

A standalone Rust crate providing comprehensive configuration management for CCMan, inspired by tmux's configuration system.

## Overview

`ccm-config` is a dependency-free configuration system that enables users to customize key bindings, set options, and manage preferences through TOML configuration files and programmatic APIs. It is designed to be independent of the TUI implementation and can be used in other contexts.

## Features

### Configuration File Support
- **Format**: TOML (`~/.ccm/config.toml`)
- **Modular**: Support for multiple config files via `source` directive
- **Flexible**: Override any binding in any context

### Key Binding System
- **Context-Aware**: Different bindings for different modes (Sidebar, Terminal Normal/Insert, Diff, Git Status, TODO, Dialog)
- **Customizable Prefix Key**: Default `C-s`, can be changed
- **Rich Key Support**: Modifiers (Ctrl, Shift, Alt, Meta), special keys, function keys, single characters

### Action System
- **Type-Safe**: Compile-time checked actions
- **Extensible**: 70+ predefined actions covering all UI operations
- **Flexible**: Support for dynamic action variants (e.g., repo switching)

## Configuration File Format

### Basic Structure

```toml
[prefix]
key = "C-s"  # Customize the prefix key

[options]
tree_view_enabled = true
git_panel_enabled = true

[bindings.global]
"q" = "quit"
"?" = "show-help"

[bindings.sidebar]
"j" = "move-down"
"k" = "move-up"
"Tab" = "focus-next"
"S-Tab" = "focus-prev"
"Enter" = "select"

[bindings.terminal_normal]
"i" = "insert-mode"
"f" = "toggle-fullscreen"
"k" = "scroll-up"
"j" = "scroll-down"

[bindings.terminal_insert]
"C-`" = "switch-to-shell"

[bindings.diff]
"j" = "move-down"
"k" = "move-up"
"{" = "prev-file"
"}" = "next-file"
"Enter" = "toggle-expand"
"c" = "add-comment"

[bindings.git_status]
"j" = "move-down"
"k" = "move-up"
"s" = "stage-file"
"u" = "unstage-file"
"S" = "stage-all"
"U" = "unstage-all"

[bindings.todo]
"j" = "move-down"
"k" = "move-up"
"a" = "add-todo"
"e" = "edit-todo-title"
"d" = "edit-todo-description"
"x" = "delete-todo"

[bindings.prefix]
"b" = "focus-branches"
"s" = "focus-sessions"
"t" = "focus-terminal"
"n" = "create-session"
"q" = "quit"

# Source additional config files
source = ["~/.ccm/local.toml"]
```

## Key Pattern Format

Keys are specified using a simple pattern syntax:

### Modifiers
- `C-x` or `Ctrl-x` - Control key
- `S-x` or `Shift-x` - Shift key
- `A-x` or `Alt-x` - Alt key
- `M-x` or `Meta-x` - Meta/Super key

### Special Keys
- `Enter`, `Return` - Enter key
- `Esc`, `Escape` - Escape key
- `Tab` - Tab key
- `Space` - Space key
- `Backspace`, `Back` - Backspace
- `Delete`, `Insert` - Delete and Insert
- `Up`, `Down`, `Left`, `Right` - Arrow keys
- `Home`, `End` - Home and End keys
- `PageUp`, `PageDown` - Page up and down
- `F1`-`F24` - Function keys

### Single Characters
Any single character: `a`, `b`, `1`, `2`, `!`, `@`, etc.

### Examples
- `C-s` → Ctrl+s
- `S-Tab` → Shift+Tab
- `C-S-x` → Ctrl+Shift+x
- `j` → j key
- `1` → 1 key
- `Enter` → Enter key

## Binding Contexts

The configuration system supports the following contexts:

| Context | Usage | Example Bindings |
|---------|-------|-----------------|
| `global` | Always checked | `q=quit` |
| `prefix` | After prefix key | `b=focus-branches` |
| `sidebar` | Sidebar/branch list | `j/k=navigate` |
| `terminal_normal` | Terminal normal mode | `i=insert-mode` |
| `terminal_insert` | Terminal insert mode | `C-`=shell` |
| `diff` | Diff view | `j/k=navigate` |
| `git_status` | Git status panel | `s=stage` |
| `todo` | TODO popup | `a=add-todo` |
| `dialog_text` | Text input dialog | *(restricted, fixed keys)* |
| `dialog_confirm` | Confirmation dialog | *(restricted, fixed keys)* |

### Resolution Priority
1. Dialog modes (highest priority)
2. Context-specific bindings
3. Global bindings
4. Prefix mode bindings
5. Terminal Insert: forward to PTY (lowest priority)

## Available Actions

### Navigation
- `move-up`, `move-down` - Navigate in lists
- `move-left`, `move-right` - Horizontal navigation
- `goto-top`, `goto-bottom` - Jump to extremes
- `scroll-up`, `scroll-down` - Scroll content
- `focus-next`, `focus-prev` - Switch focus areas
- `focus-sidebar`, `focus-terminal`, `focus-git-status` - Focus specific panel
- `focus-branches`, `focus-sessions` - Legacy navigation

### Session Management
- `create-session` - Create new session
- `delete-current` - Delete current session/worktree
- `rename-session` - Rename current session
- `switch-repo-0` through `switch-repo-8` - Switch repositories (configurable via prefix: 1-9)

### Terminal
- `insert-mode` - Enter insert mode
- `terminal-normal-mode` - Enter normal mode
- `toggle-fullscreen` - Toggle fullscreen
- `exit-fullscreen` - Exit fullscreen
- `exit-terminal` - Exit terminal
- `switch-to-shell` - Switch to shell

### View Switching
- `toggle-diff-view` - Toggle between diff and terminal
- `toggle-tree-view` - Toggle tree view
- `back-to-terminal` - Back to terminal from diff

### Diff Operations
- `prev-file`, `next-file` - Navigate files in diff
- `toggle-expand` - Expand/collapse file
- `add-comment` - Add line comment
- `edit-comment` - Edit line comment
- `delete-comment` - Delete comment
- `next-comment`, `prev-comment` - Jump between comments
- `submit-review-claude` - Submit review to Claude
- `refresh-diff` - Refresh diff view

### Git Operations
- `stage-file`, `unstage-file` - Stage/unstage file
- `stage-all`, `unstage-all` - Stage/unstage all
- `refresh-status` - Refresh git status
- `toggle-or-open` - Toggle expand or open diff

### TODO Operations
- `add-todo` - Add new TODO
- `add-child-todo` - Add TODO as child of current
- `edit-todo-title` - Edit TODO title
- `edit-todo-description` - Edit TODO description
- `delete-todo` - Delete TODO
- `toggle-todo-complete` - Toggle completion status
- `toggle-show-completed` - Show/hide completed items

### Utilities
- `select` - Select current item
- `toggle-expand` - Toggle expansion
- `toggle-or-open` - Toggle or open (context-dependent)
- `refresh-all` - Refresh all data
- `add-worktree` - Add new worktree
- `open-todo` - Open TODO popup
- `show-help` - Show help
- `quit` - Quit application

## Programmatic API

### Loading Configuration

```rust
use ccm_config::Config;

// Load from default location (~/.ccm/config.toml) or use defaults
let config = Config::load_or_default()?;

// Load from specific path
let config = Config::load_from_file(Path::new("/path/to/config.toml"))?;

// Build keybind map
let keybinds = config.to_keybind_map()?;
```

### Resolving Bindings

```rust
use ccm_config::{KeyPattern, BindingContext};

// Parse key pattern
let pattern = KeyPattern::parse("C-s")?;

// Resolve action
if let Some(action) = keybinds.resolve(&pattern_string, BindingContext::Sidebar) {
    // Execute action
}

// Get prefix key
let prefix_key = keybinds.prefix_key();
```

### Accessing Configuration

```rust
// Get option values
let tree_view_enabled = config.options.tree_view_enabled;
let git_panel_enabled = config.options.git_panel_enabled;

// Get all bindings for a context
if let Some(bindings) = keybinds.bindings_for_context(BindingContext::Sidebar) {
    for (key, action) in bindings {
        println!("{} => {}", key, action);
    }
}
```

## Integration with CCM CLI

The `ccm-config` crate is integrated into `ccm-cli` as follows:

1. **Configuration Loading**: `App::new()` loads config automatically
2. **Keybind Resolution**: Input handlers use `app.keybinds` for resolution
3. **Resolver Adapter**: `ccm-cli/src/tui/input/resolver.rs` bridges crossterm and ccm-config

## Default Configuration

If no configuration file exists, CCM uses sensible defaults that match the original hardcoded bindings:

- **Prefix Key**: `C-s` (Ctrl+s)
- **Sidebar**: vim-style navigation (j/k/J/K), Tab for focus switching
- **Terminal**: Insert mode for input, Normal mode for scrolling
- **Diff**: vim navigation, expandable files, comment support
- **Git**: Navigation, staging/unstaging, refresh
- **TODO**: Full CRUD operations

## Modular Configuration

Organize your configuration into multiple files:

```
~/.ccm/
├── config.toml           # Main configuration
├── keybinds/
│   ├── vim.toml         # Vim-style bindings
│   └── emacs.toml       # Emacs-style bindings
├── themes/
│   ├── dark.toml        # Dark theme options
│   └── light.toml       # Light theme options
└── local.toml           # Local overrides
```

Then in `~/.ccm/config.toml`:

```toml
# ... main configuration ...

source = [
    "~/.ccm/keybinds/vim.toml",
    "~/.ccm/themes/dark.toml",
    "~/.ccm/local.toml"
]
```

## Error Handling

The system provides detailed error messages for configuration issues:

```
Failed to load config: Invalid key pattern: C-s-x (too many modifiers)
Failed to load config: Invalid action: "unknown-action"
Failed to load config: File not found: /path/to/config.toml
```

## Design Decisions

### Type-Safe Actions
All bindable operations are represented as an `Action` enum, enabling compile-time checking and preventing invalid action names.

### Context Priority
The resolution order (dialogs → context-specific → global → prefix) ensures dialog modes work reliably and provides fallback chains.

### Independent Crate
`ccm-config` is fully independent of the TUI layer, making it suitable for use in other applications (scripting, CLI tools, etc.).

### Default Compatibility
Default configuration exactly matches the original hardcoded bindings, ensuring zero breaking changes for existing users.

## Migration from Hardcoded Bindings

Users don't need to do anything. CCM automatically:
1. Detects missing configuration file
2. Uses built-in defaults
3. Maintains existing behavior

To customize, users simply create `~/.ccm/config.toml` and override what they want.

## Future Enhancements

Potential additions (not implemented yet):

- Runtime `:set` and `:bind` commands
- Interactive keybind menu
- Keybind conflict detection
- Theme system
- Macro system
- Conditional bindings based on context variables

## Contributing

To add new actions:

1. Add variant to `Action` enum in `ccm-config/src/actions.rs`
2. Implement `from_str()` parsing
3. Map in appropriate input handler
4. Add tests

## License

Same as CCMan project.
