# TUI Performance Optimization

This document analyzes the input latency issues in our TUI and proposes optimizations based on tmux's architecture.

## Problem Statement

The TUI sometimes feels sluggish during input, especially when:
- Navigating between sessions
- Typing in terminal insert mode
- Switching repos or branches

## Root Cause Analysis

### Current Architecture (app.rs:1136-1165)

```rust
loop {
    app.poll_terminal_output();     // 1. Poll terminal output
    app.poll_events();              // 2. Poll daemon events

    // 3. Render - full redraw every loop
    terminal.draw(|f| draw(f, &app))?;

    // 4. Wait for input with 50ms timeout
    if event::poll(Duration::from_millis(50))? {
        if let Event::Key(key) = event::read()? {
            handle_input(&mut app, key).await?;  // async!
        }
    }
}
```

### Identified Issues

| Issue | Description | Impact |
|-------|-------------|--------|
| **Synchronous Rendering** | Full UI redraw every loop iteration | Input must wait for render completion |
| **Async Input Handling** | `handle_input` contains RPC calls | Keypress response delayed by network |
| **Fixed Poll Interval** | 50ms regardless of activity | Wasted CPU when idle, slow when busy |
| **Single-threaded Model** | All operations in one tokio task | Cannot parallelize input and rendering |

### Example: Navigation Causes Network Call

```rust
// input.rs - selecting previous item triggers RPC
pub async fn select_prev(&mut self) {
    match self.focus {
        Focus::Branches => {
            if self.branch_idx > 0 {
                self.branch_idx -= 1;
                let _ = self.refresh_sessions().await;  // RPC call blocks input!
            }
        }
        // ...
    }
}
```

## tmux Architecture Reference

tmux achieves excellent responsiveness through:

1. **libevent-based Event Loop** - True non-blocking I/O with epoll/kqueue
2. **Client-Server Separation** - Input processing decoupled from rendering
3. **Buffered Event Queues** - Input processed immediately, render notifications async
4. **Render Rate Limiting** - Automatic backoff when output is heavy

### Key Insight

> The core to tmux's responsiveness is the separation of concerns:
> - Server side: Rapidly processes all PTY input and buffers output to clients
> - Client side: Handles rendering and terminal updates independently

This explains why even when tmux seems frozen (rendering backlog), Ctrl+C works immediately - the input has already been processed; only the display is delayed.

## Proposed Optimizations

### Phase 1: Separate Sync/Async Operations (High Priority)

Split input handling into immediate local updates and deferred async actions:

```rust
// Define async actions that can be queued
pub enum AsyncAction {
    RefreshAll,
    RefreshSessions,
    CreateSession { repo_id: String, branch: String },
    DestroySession { session_id: String },
    // ...
}

// Synchronous input handler - returns optional async action
fn handle_input_sync(app: &mut App, key: KeyEvent) -> Option<AsyncAction> {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            app.select_prev_sync();  // Pure local state update
            None
        }
        KeyCode::Char('r') => {
            Some(AsyncAction::RefreshAll)  // Queue for later
        }
        // ...
    }
}

// Local state updates - no await
impl App {
    fn select_prev_sync(&mut self) {
        match self.focus {
            Focus::Branches => {
                if self.branch_idx > 0 {
                    self.branch_idx -= 1;
                    self.needs_session_refresh = true;  // Flag instead of RPC
                }
            }
            // ...
        }
    }
}
```

### Phase 2: Event-Driven Loop with tokio::select! (Medium Priority)

Replace polling with true event-driven architecture:

```rust
pub async fn run_with_client(mut app: App) -> Result<RunResult> {
    let mut render_interval = tokio::time::interval(Duration::from_millis(16));
    let (input_tx, mut input_rx) = mpsc::channel(32);

    // Spawn input reader thread (crossterm events are blocking)
    std::thread::spawn(move || {
        loop {
            if let Ok(event) = event::read() {
                if input_tx.blocking_send(event).is_err() {
                    break;
                }
            }
        }
    });

    let mut dirty = true;
    let mut pending_action: Option<AsyncAction> = None;

    loop {
        tokio::select! {
            biased;  // Priority order matters

            // 1. Highest priority: keyboard input
            Some(event) = input_rx.recv() => {
                if let Event::Key(key) = event {
                    pending_action = handle_input_sync(&mut app, key);
                    dirty = true;
                }
            }

            // 2. Terminal output from PTY
            Some(data) = async {
                app.terminal_stream.as_mut()?.output_rx.recv().await
            } => {
                if let Ok(mut parser) = app.terminal_parser.lock() {
                    parser.process(&data);
                }
                dirty = true;
            }

            // 3. Daemon events
            Some(event) = async {
                app.event_rx.as_mut()?.recv().await
            } => {
                app.handle_daemon_event(event);
                dirty = true;
            }

            // 4. Render tick (~60fps)
            _ = render_interval.tick() => {
                // Process any pending async action
                if let Some(action) = pending_action.take() {
                    execute_async_action(&mut app, action).await;
                    dirty = true;
                }

                // Only render if dirty
                if dirty {
                    terminal.draw(|f| draw(f, &app))?;
                    dirty = false;
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(RunResult::Quit)
}
```

### Phase 3: Dirty Flag Optimization (Low Priority)

Track what changed to minimize redraw work:

```rust
pub struct App {
    // Dirty flags for partial updates
    dirty_flags: DirtyFlags,
}

#[derive(Default)]
pub struct DirtyFlags {
    pub sidebar: bool,
    pub terminal: bool,
    pub status_bar: bool,
    pub full_redraw: bool,
}

impl DirtyFlags {
    pub fn any(&self) -> bool {
        self.sidebar || self.terminal || self.status_bar || self.full_redraw
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}
```

## Implementation Priority

| Phase | Effort | Impact | Description |
|-------|--------|--------|-------------|
| 1 | Medium | High | Separate sync/async - immediate input response |
| 2 | High | High | Event-driven loop - proper architecture |
| 3 | Low | Low | Dirty flags - reduce CPU usage |

## Quick Wins (Can Implement Now)

1. **Reduce poll timeout** from 50ms to 16ms for snappier feel
2. **Batch terminal output processing** - process all available data before render
3. **Defer session refresh** - don't block navigation on RPC

```rust
// Quick fix: process all pending input before render
loop {
    // Drain all pending input first
    while event::poll(Duration::ZERO)? {
        if let Event::Key(key) = event::read()? {
            handle_input_sync(&mut app, key);
        }
    }

    // Then render once
    terminal.draw(|f| draw(f, &app))?;

    // Short sleep for next frame
    tokio::time::sleep(Duration::from_millis(16)).await;
}
```

## Metrics to Track

- Input-to-render latency (target: <16ms for local operations)
- Frame time (target: <16ms = 60fps)
- RPC call frequency during navigation

## References

- [tmux source code](https://github.com/tmux/tmux)
- [libevent documentation](https://libevent.org/)
- [ratatui performance tips](https://ratatui.rs/concepts/rendering/)
- [tokio::select! macro](https://docs.rs/tokio/latest/tokio/macro.select.html)
