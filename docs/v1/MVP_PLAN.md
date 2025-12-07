# CCM MVP Plan

A minimal viable product for CCM (Claude Code Manager) - a tool to manage multiple Claude Code sessions organized by Git repos and branches.

## Overview

CCM provides a TUI interface to:
- Manage multiple Git repositories
- Create and switch between worktrees (branches)
- Run multiple Claude Code sessions per worktree
- Preview and interact with terminal output

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        ccm (TUI)                            │
│  ┌─────────────────┐  ┌─────────────────────────────────┐  │
│  │   Sidebar       │  │      Terminal Preview           │  │
│  │  ┌───────────┐  │  │                                 │  │
│  │  │ Branches  │  │  │  ┌─────────────────────────┐   │  │
│  │  │  > main   │  │  │  │ Claude Code output...   │   │  │
│  │  │    feat-1 │  │  │  │                         │   │  │
│  │  │    feat-2 │  │  │  │                         │   │  │
│  │  └───────────┘  │  │  └─────────────────────────┘   │  │
│  │  ┌───────────┐  │  │                                 │  │
│  │  │ Sessions  │  │  │  [Normal/Insert mode]           │  │
│  │  │  > main   │  │  │  [j/k scroll, i insert]         │  │
│  │  │    main-2 │  │  │                                 │  │
│  │  └───────────┘  │  │                                 │  │
│  └─────────────────┘  └─────────────────────────────────┘  │
│  [Tab bar: 1:repo1 | 2:repo2 | ...]                        │
│  [Status bar: keybindings help]                            │
└─────────────────────────────────────────────────────────────┘
```

## Components

### ccm-daemon
Background daemon that:
- Manages repositories (add/remove/list)
- Manages git worktrees (create/remove/list)
- Manages sessions (create/destroy/list)
- Spawns PTY processes for Claude Code
- Streams terminal I/O via gRPC
- Persists output buffer for history replay

### ccm-cli (ccm)
TUI client that:
- Connects to daemon via Unix socket
- Displays repo tabs, branch/session sidebar
- Renders terminal output with colors (vt100)
- Vim-style Normal/Insert modes for terminal
- Supports scrollback and fullscreen toggle

### ccm-proto
Protocol buffer definitions for gRPC:
- CcmDaemon service
- Session/Repo/Worktree management RPCs
- AttachSession bidirectional streaming

## Key Features (MVP)

1. **Repository Management**
   - Add/remove Git repositories
   - Persist repo list to disk

2. **Worktree Management**
   - List all branches
   - Auto-create worktrees when needed
   - Remove worktrees

3. **Session Management**
   - Create sessions (spawns `claude` in PTY)
   - List sessions per branch
   - Destroy sessions
   - Session name from Claude slug

4. **Terminal Interaction**
   - Real-time terminal preview
   - Vim-style Normal/Insert modes
   - Scrollback with j/k, Ctrl+d/u, g/G
   - Fullscreen toggle (f/z)
   - Color support via vt100

5. **Buffer Persistence**
   - Daemon stores raw PTY output
   - History replay on client attach

6. **Input Method Control**
   - Disable fcitx5 during TUI
   - Re-enable on exit

## Keybindings

### Prefix Key (Ctrl+s)
Works from any context (except text input). Press `Ctrl+s` then:
- `b`: Go to Branches
- `s`: Go to Sessions
- `t`: Go to Terminal (Insert mode)
- `n`: New session/worktree
- `a`: Add worktree
- `d`: Delete
- `r`: Refresh all
- `f/z`: Toggle fullscreen
- `1-9`: Switch repos
- `q`: Quit
- `Esc`: Cancel prefix

### Navigation Mode (Sidebar)
- `1-9`: Switch repos
- `Tab`: Move focus (Branches → Sessions → Terminal)
- `Shift+Tab/Esc`: Move focus backward
- `j/k`: Move selection
- `Enter`: Enter terminal / create session
- `n`: New branch/session
- `a`: Add worktree (in Branches)
- `d`: Delete branch/session
- `r`: Refresh all
- `q`: Quit

### Terminal Normal Mode
- `j/k`: Scroll up/down
- `Ctrl+u/d`: Half page up/down
- `g/G`: Top/bottom
- `i/Enter`: Enter Insert mode
- `f/z`: Toggle fullscreen
- `Esc/Shift+Tab`: Exit to Sessions

### Terminal Insert Mode
- All keys sent to terminal
- `Esc`: Exit to Normal mode

## File Structure

```
ccman/
├── Cargo.toml           # Workspace
├── docs/
│   └── MVP_PLAN.md      # This file
├── ccm-proto/
│   ├── proto/daemon.proto
│   └── src/lib.rs
├── ccm-daemon/
│   └── src/
│       ├── main.rs
│       ├── server.rs    # gRPC service
│       ├── session.rs   # Session + buffer
│       ├── pty.rs       # PTY management
│       ├── git.rs       # Git operations
│       ├── repo.rs      # Repo persistence
│       └── state.rs     # Shared state
└── ccm-cli/
    └── src/
        ├── main.rs
        ├── client.rs    # gRPC client
        └── tui/
            ├── mod.rs
            ├── app.rs   # App state + vt100
            ├── input.rs # Key handling
            └── ui.rs    # Rendering
```

## Future Enhancements (Post-MVP)

- Session persistence across daemon restarts
- Remote daemon support (TCP)
- Multiple terminal panes
- Search in scrollback
- Copy/paste support
- Custom themes
