# CCM Configuration Guide

Learn how to customize CCMan (CCM) to match your workflow.

## Quick Start

CCM works out of the box with sensible vim-like defaults. To customize:

1. Create `~/.ccm/config.toml`
2. Add your customizations
3. Restart CCM

Example:

```toml
[bindings.sidebar]
"j" = "move-down"
"k" = "move-up"
"h" = "move-left"
"l" = "move-right"
```

## Default Bindings Overview

### Navigation (Sidebar)
| Key | Action | Notes |
|-----|--------|-------|
| `j` / `Down` | Move down | |
| `k` / `Up` | Move up | |
| `Tab` | Next focus | Switch to terminal or diff |
| `S-Tab` / `Esc` | Previous focus | Back to sidebar |
| `Enter` | Select | Expand/collapse or open |
| `o` | Toggle expand | Expand/collapse item |
| `n` | Create session | New session |
| `a` | Add worktree | New worktree |
| `d` / `x` | Delete | Delete session/worktree |
| `r` | Refresh | Refresh data |
| `R` | Rename | Rename session |
| `T` | Toggle tree view | Show/hide tree |
| `g` | Git status | Switch to git panel |
| `t` | Diff view | Switch to diff |
| `q` | Quit | Exit CCM |

### Terminal (Normal Mode - vim-like)
| Key | Action | Notes |
|-----|--------|-------|
| `i` / `Enter` | Insert mode | Enter interactive mode |
| `k` | Scroll up | Scroll terminal |
| `j` | Scroll down | Scroll terminal |
| `f` / `z` | Fullscreen | Toggle fullscreen |
| `Tab` / `S-Tab` | Focus | Switch focus |
| `Esc` | Exit fullscreen | Or `f`/`z` again |

### Terminal (Insert Mode - interactive)
| Key | Action | Notes |
|-----|--------|-------|
| All text | Forward to shell | Type normally |
| `Ctrl+s` | Prefix mode | Access commands |
| `C-`` | Switch to shell | Alternative shell |
| `Shift+Tab` | Exit terminal | Back to sidebar |

### Prefix Mode (`Ctrl+s` + key)
| Key | Action | Notes |
|-----|--------|-------|
| `b` | Focus branches | Show branch list |
| `s` | Focus sessions | Show session list |
| `t` | Focus terminal | Show terminal |
| `n` | New session | Create new session |
| `d` | Delete | Delete current |
| `r` | Refresh | Refresh all data |
| `f` / `z` | Fullscreen | Toggle fullscreen |
| `[` | Normal mode | Enter terminal normal |
| `w` | Sidebar | Back to sidebar |
| `o` | TODO | Open TODO popup |
| `a` | Add worktree | New worktree |
| `q` | Quit | Exit CCM |
| `1`-`9` | Repo | Switch repository |

### Diff View
| Key | Action | Notes |
|-----|--------|-------|
| `j` / `Down` | Down | Move down |
| `k` / `Up` | Up | Move up |
| `{` | Prev file | Previous file |
| `}` | Next file | Next file |
| `Enter` / `o` | Expand | Expand/collapse |
| `c` | Add comment | Comment on line |
| `C` | Edit comment | Edit existing comment |
| `x` | Delete comment | Remove comment |
| `n` | Next comment | Jump to next |
| `N` | Prev comment | Jump to previous |
| `S` | Claude review | Submit diff to Claude |
| `f` / `z` | Fullscreen | Toggle fullscreen |
| `r` | Refresh | Refresh diff |
| `Esc` / `t` | Terminal | Back to terminal |

### Git Status Panel
| Key | Action | Notes |
|-----|--------|-------|
| `j` / `Down` | Down | Move down |
| `k` / `Up` | Up | Move up |
| `Enter` / `o` | Open diff | View file diff |
| `s` | Stage | Stage file |
| `u` | Unstage | Unstage file |
| `S` | Stage all | Stage all files |
| `U` | Unstage all | Unstage all files |
| `r` | Refresh | Refresh status |
| `Tab` | Diff view | Switch to diff |
| `Esc` | Back | Back to sidebar |

### TODO List
| Key | Action | Notes |
|-----|--------|-------|
| `j` / `Down` | Down | Move down |
| `k` / `Up` | Up | Move up |
| `g` | Top | Go to top |
| `G` | Bottom | Go to bottom |
| `a` | Add | New TODO |
| `A` | Add child | Sub-TODO |
| `e` | Edit | Edit title |
| `D` | Description | Edit description |
| `x` | Delete | Delete TODO |
| `Space` | Toggle | Mark done/undone |
| `s` | Completed | Show/hide completed |
| `Esc` | Close | Close popup |

## Customization Examples

### Use Arrow Keys Instead of vim Keys

```toml
[bindings.sidebar]
"Down" = "move-down"
"Up" = "move-up"
"Left" = "move-left"
"Right" = "move-right"
```

### Change Prefix Key to Ctrl+a (like tmux)

```toml
[prefix]
key = "C-a"

[bindings.prefix]
# ... your prefix bindings
```

### Emacs-Style Navigation

```toml
[bindings.sidebar]
"C-n" = "move-down"
"C-p" = "move-up"
"C-f" = "move-right"
"C-b" = "move-left"

[bindings.terminal_normal]
"C-n" = "scroll-down"
"C-p" = "scroll-up"
```

### No Prefix Mode (Direct Access)

Remove the prefix bindings and add direct bindings instead:

```toml
[bindings.global]
"b" = "focus-branches"
"s" = "focus-sessions"
"t" = "focus-terminal"
"q" = "quit"
```

### Vim-Operator Style

```toml
[bindings.diff]
"d" = "delete-comment"
"y" = "add-comment"
"c" = "edit-comment"
```

### Minimal Configuration

Only override what you need:

```toml
[prefix]
key = "C-space"

[bindings.sidebar]
"h" = "move-left"
"l" = "move-right"
```

The rest will use defaults automatically.

## Configuration Organization

For larger customizations, split into multiple files:

**~/.ccm/config.toml:**
```toml
[prefix]
key = "C-s"

[options]
tree_view_enabled = true

source = [
    "~/.ccm/bindings.toml",
    "~/.ccm/local.toml"
]
```

**~/.ccm/bindings.toml:**
```toml
[bindings.sidebar]
"j" = "move-down"
"k" = "move-up"
# ... more bindings
```

**~/.ccm/local.toml:**
```toml
# Local overrides (not in version control)
[bindings.sidebar]
"h" = "move-left"
```

## Key Pattern Reference

### Modifiers
- `C-x` or `Ctrl-x` - Control
- `S-x` or `Shift-x` - Shift
- `A-x` or `Alt-x` - Alt
- `M-x` or `Meta-x` - Meta/Super

Combine: `C-S-x` = Ctrl+Shift+x

### Special Keys
- `Enter`, `Return`
- `Esc`, `Escape`
- `Tab`
- `Space`
- `Backspace`, `Back`
- `Delete`, `Insert`
- `Up`, `Down`, `Left`, `Right`
- `Home`, `End`
- `PageUp`, `PageDown`
- `F1` to `F24`

### Single Characters
- Letters: `a`-`z`, `A`-`Z`
- Numbers: `0`-`9`
- Symbols: `!`, `@`, `#`, `$`, etc.

## Available Actions

See `ccm-config/README.md` for complete list. Common ones:

- Navigation: `move-up`, `move-down`, `scroll-up`, `scroll-down`
- Focus: `focus-next`, `focus-prev`, `focus-sidebar`, `focus-terminal`
- Session: `create-session`, `delete-current`, `rename-session`
- Terminal: `insert-mode`, `toggle-fullscreen`, `exit-terminal`
- Git: `stage-file`, `unstage-file`, `refresh-status`
- Diff: `add-comment`, `toggle-expand`, `prev-file`, `next-file`
- TODO: `add-todo`, `edit-todo-title`, `delete-todo`
- General: `select`, `refresh-all`, `quit`

## Validation

CCM validates your configuration on startup:

```
Warning: Invalid key pattern in config: XYZ
Warning: Invalid action in config: unknown-action
```

Invalid entries are skipped; valid ones apply.

## Troubleshooting

### Bindings Don't Work

1. Check syntax: Key names are case-sensitive
2. Verify file location: `~/.ccm/config.toml`
3. Check for typos: TOML is strict about formatting
4. Verify action names in `ccm-config/README.md`
5. Restart CCM after changes

### Default Bindings Still Work?

That's expected! Missing bindings fall back to defaults. If you only override some keys, others use defaults.

### Conflicts

If you bind the same key to different actions, the last one wins. Within the same file, this is usually a mistake. Check for:
- Duplicate keys in same context
- Overlapping modifiers (e.g., both `C-a` and `Ctrl-a`)

## Migration from Other Tools

### From tmux

Replace `bind-key` with config syntax:

```sh
# tmux
bind-key j send-keys "cd .."

# CCM (not supported - focus on UI navigation)
# Use direct navigation instead
```

### From vim

Share your vim keybindings:

```toml
[bindings.sidebar]
"j" = "move-down"
"k" = "move-up"
"h" = "move-left"
"l" = "move-right"
"w" = "focus-next"
"b" = "focus-prev"
```

## Performance

Configuration is loaded once at startup. Changes require restart.

File size: A typical config file is <1KB. Even 100+ custom bindings is <5KB.

## Next Steps

1. Create `~/.ccm/config.toml`
2. Add your customizations
3. Restart CCM
4. Verify bindings work
5. Adjust as needed

Happy customizing!
