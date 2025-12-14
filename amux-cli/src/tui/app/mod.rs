//! TUI application state machine
//!
//! Split into functional submodules:
//! - repo.rs: Repository access and refresh operations
//! - terminal.rs: Terminal operations and stream management
//! - git_ops.rs: Git status operations
//! - diff.rs: Diff view operations
//! - comments.rs: Line comment operations
//! - todo.rs: TODO operations
//! - input_forms.rs: Input form handling
//! - events.rs: Event handling and async action execution

mod comments;
mod diff;
mod events;
mod git_ops;
mod input_forms;
mod repo;
mod terminal;
mod todo;

pub use terminal::TerminalStream;

use crate::client::Client;
use crate::error::TuiError;
use amux_config::{Config, KeybindMap};
use amux_proto::daemon::Event as DaemonEvent;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, BeginSynchronizedUpdate, EndSynchronizedUpdate,
        EnterAlternateScreen, LeaveAlternateScreen,
    },
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::collections::HashMap;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

type Result<T> = std::result::Result<T, TuiError>;

use super::input::{handle_input_sync, handle_mouse_sync, TextInput};
use super::layout::draw;
use super::state::{
    AsyncAction, DirtyFlags, ExitCleanupAction, Focus, InputMode, PrefixMode, RepoState,
    RightPanelView, SavedFocusState, SidebarState, TerminalState, TodoState,
};

/// Deactivate fcitx5 input method
fn deactivate_ime() {
    let _ = std::process::Command::new("fcitx5-remote")
        .arg("-c")
        .output();
}

/// Activate fcitx5 input method
fn activate_ime() {
    let _ = std::process::Command::new("fcitx5-remote")
        .arg("-o")
        .output();
}

/// Application state
pub struct App {
    pub client: Client,

    // ============ Repo Management ============
    /// Per-repo state keyed by repo_id (contains all repo-specific data)
    pub repo_states: HashMap<String, RepoState>,
    /// Order of repos for display (list of repo_ids)
    pub repo_order: Vec<String>,
    /// Currently selected repo ID
    pub current_repo_id: Option<String>,

    // ============ Global UI State ============
    /// Focus position
    pub focus: Focus,
    /// Focus restoration stack for popups/dialogs (saves focus and terminal mode)
    pub saved_focus_stack: Vec<SavedFocusState>,

    // ============ Terminal State (global, shared across repos) ============
    pub terminal: TerminalState,
    pub terminal_stream: Option<TerminalStream>,

    // ============ Sidebar State (global parts only) ============
    pub sidebar: SidebarState,

    // ============ TODO State (global) ============
    pub todo: TodoState,

    // ============ View State ============
    /// Right panel view mode (shared between terminal and diff)
    pub right_panel_view: RightPanelView,

    // ============ UI State ============
    pub should_quit: bool,
    pub error_message: Option<String>,
    pub status_message: Option<String>,
    pub input_mode: InputMode,
    pub text_input: TextInput,
    pub session_delete_action: ExitCleanupAction,

    // ============ Event Subscription ============
    pub event_rx: Option<mpsc::Receiver<DaemonEvent>>,

    // ============ Debounce ============
    pub last_git_refresh: Option<std::time::Instant>,

    // ============ Prefix Key Mode ============
    pub prefix_mode: PrefixMode,

    // ============ Configuration ============
    #[allow(dead_code)]
    pub config: Config,
    pub keybinds: KeybindMap,

    // ============ Dirty Flags ============
    pub dirty: DirtyFlags,
}

impl App {
    pub async fn new(client: Client) -> Result<Self> {
        // Load configuration and build keybind map
        let config = Config::load_or_default()
            .map_err(|e| TuiError::Config(format!("Failed to load config: {}", e)))?;
        let keybinds = config
            .to_keybind_map()
            .map_err(|e| TuiError::Config(format!("Failed to build keybind map: {}", e)))?;

        let mut app = Self {
            client,
            // Repo management
            repo_states: HashMap::new(),
            repo_order: Vec::new(),
            current_repo_id: None,
            // Global UI
            focus: Focus::Sidebar,
            saved_focus_stack: Vec::new(),
            // Terminal
            terminal: TerminalState::default(),
            terminal_stream: None,
            // Sidebar (global parts)
            sidebar: SidebarState::default(),
            // TODO
            todo: TodoState::new(),
            // View
            right_panel_view: RightPanelView::Terminal,
            // UI state
            should_quit: false,
            error_message: None,
            status_message: None,
            input_mode: InputMode::Normal,
            text_input: TextInput::new(),
            session_delete_action: ExitCleanupAction::Destroy,
            // Event subscription
            event_rx: None,
            // Debounce
            last_git_refresh: None,
            // Prefix mode
            prefix_mode: PrefixMode::None,
            // Config
            config,
            keybinds,
            // Dirty flags
            dirty: DirtyFlags::default(),
        };

        // Load initial data
        app.refresh_all().await?;

        // Load git status for current worktree
        let _ = app.load_git_status().await;

        // Subscribe to events (don't fail if subscription fails)
        app.subscribe_events().await;

        Ok(app)
    }
}

// Drop trait for automatic cleanup on abnormal exit
impl Drop for App {
    fn drop(&mut self) {
        // Only cleanup on abnormal exit (should_quit = false means unexpected termination)
        if !self.should_quit && !self.sessions().is_empty() {
            let running_ids: Vec<String> = self
                .sessions()
                .iter()
                .filter(|s| s.status == 1) // SESSION_STATUS_RUNNING
                .map(|s| s.id.clone())
                .collect();

            if !running_ids.is_empty() {
                // Drop cannot be async, so create a sync runtime
                if let Ok(runtime) = tokio::runtime::Runtime::new() {
                    for session_id in &running_ids {
                        let _ = runtime.block_on(self.client.stop_session(session_id));
                    }
                }
            }
        }
    }
}

/// Result of TUI run
pub enum RunResult {
    /// User quit (q)
    Quit,
}

/// Spawn a thread to read crossterm events (blocking I/O)
fn spawn_input_reader() -> mpsc::Receiver<Event> {
    let (tx, rx) = mpsc::channel(32);

    std::thread::spawn(move || {
        while let Ok(event) = event::read() {
            if tx.blocking_send(event).is_err() {
                break; // Receiver dropped
            }
        }
    });

    rx
}

/// Run the TUI application
pub async fn run_with_client(mut app: App, should_exit: Arc<AtomicBool>) -> Result<RunResult> {
    // Deactivate IME at startup
    deactivate_ime();

    // Setup terminal
    enable_raw_mode().map_err(TuiError::TerminalInit)?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture).map_err(TuiError::TerminalInit)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(TuiError::TerminalInit)?;

    // Spawn input reader thread (crossterm events are blocking)
    let mut input_rx = spawn_input_reader();

    // Fixed 16ms render interval (~60fps) - always render on every tick (tuitest pattern)
    let mut render_interval = tokio::time::interval(std::time::Duration::from_millis(16));
    render_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    // Fallback timers for daemon reconnection
    let mut last_refresh = std::time::Instant::now();
    let mut last_resubscribe_attempt = std::time::Instant::now();
    let refresh_interval = std::time::Duration::from_secs(5);
    let resubscribe_interval = std::time::Duration::from_secs(10);

    // Only need pending_action for async operations
    let mut pending_action: Option<AsyncAction> = None;

    // Main loop with tokio::select!
    loop {
        tokio::select! {
            biased; // Check branches in priority order

            // 1. Highest priority: keyboard input
            Some(event) = input_rx.recv() => {
                match event {
                    Event::Key(key) => {
                        // Sync input handling - returns optional async action
                        if let Some(action) = handle_input_sync(&mut app, key) {
                            // If already have a pending action, execute it immediately
                            if let Some(old_action) = pending_action.take() {
                                let _ = app.execute_async_action(old_action).await;
                            }
                            pending_action = Some(action);
                        }
                    }
                    Event::Resize(cols, rows) => {
                        let _ = app.resize_terminal(rows, cols).await;
                    }
                    Event::Mouse(mouse) => {
                        handle_mouse_sync(&mut app, mouse);
                    }
                    _ => {}
                }
            }

            // 2. Terminal PTY output - process data and handle terminal queries
            Some(data) = async {
                match app.terminal_stream.as_mut() {
                    Some(stream) => stream.output_rx.recv().await,
                    None => std::future::pending().await,
                }
            } => {
                // Debug: log escape sequences in received data
                if data.contains(&0x1b) {
                    let escaped: String = data.iter().map(|&b| {
                        if b == 0x1b { "ESC".to_string() }
                        else if b < 32 { format!("^{}", (b + 64) as char) }
                        else { (b as char).to_string() }
                    }).collect();
                    tracing::debug!("PTY output contains escape: {}", escaped);
                }

                // Check for terminal query sequences and respond
                if let Some(response) = detect_terminal_query(&data, &app.terminal.parser) {
                    tracing::debug!("Detected terminal query, sending response: {:?}", response);
                    // Send response back to PTY
                    let _ = app.send_to_terminal(response).await;
                }

                if let Ok(mut parser) = app.terminal.parser.lock() {
                    // Save user's scroll position before processing PTY output
                    let scroll_offset = parser.screen().scrollback();

                    parser.process(&data);

                    // If user is viewing history (offset > 0), restore scroll position
                    // This prevents new output from pulling user back to bottom
                    if scroll_offset > 0 {
                        parser.screen_mut().set_scrollback(scroll_offset);
                    }
                }
            }

            // 3. Daemon events - update state
            Some(event) = async {
                match app.event_rx.as_mut() {
                    Some(rx) => rx.recv().await,
                    None => std::future::pending().await,
                }
            } => {
                if let Some(action) = app.handle_daemon_event(event) {
                    // If already have a pending action, execute it immediately
                    if let Some(old_action) = pending_action.take() {
                        let _ = app.execute_async_action(old_action).await;
                    }
                    pending_action = Some(action);
                }
            }

            // 4. Render tick - ALWAYS RENDER (tuitest pattern)
            _ = render_interval.tick() => {
                // Drain all pending PTY data before rendering to avoid showing intermediate states
                // (e.g., blank screen during clear+redraw sequences from TUI frameworks like ink)
                // First, collect all pending data to avoid borrow conflicts
                let pending_data: Vec<Vec<u8>> = if let Some(stream) = app.terminal_stream.as_mut() {
                    let mut data_vec = Vec::new();
                    while let Ok(data) = stream.output_rx.try_recv() {
                        data_vec.push(data);
                    }
                    data_vec
                } else {
                    Vec::new()
                };

                // Process all collected PTY data
                for data in pending_data {
                    // Check for terminal query sequences and respond
                    if let Some(response) = detect_terminal_query(&data, &app.terminal.parser) {
                        let _ = app.send_to_terminal(response).await;
                    }

                    if let Ok(mut parser) = app.terminal.parser.lock() {
                        let scroll_offset = parser.screen().scrollback();
                        parser.process(&data);
                        if scroll_offset > 0 {
                            parser.screen_mut().set_scrollback(scroll_offset);
                        }
                    }
                }

                // Execute pending async action
                if let Some(action) = pending_action.take() {
                    if let Err(e) = app.execute_async_action(action).await {
                        app.error_message = Some(format!("{}", e));
                    }
                }

                // Check if we need to resubscribe (event channel disconnected)
                if app.needs_resubscribe() {
                    // Fallback: Periodic session refresh while disconnected
                    if last_refresh.elapsed() >= refresh_interval {
                        let _ = app.refresh_sessions().await;
                        last_refresh = std::time::Instant::now();
                    }

                    // Periodically attempt to resubscribe
                    if last_resubscribe_attempt.elapsed() >= resubscribe_interval {
                        app.try_resubscribe().await;
                        last_resubscribe_attempt = std::time::Instant::now();
                    }
                }

                // Always render - no dirty checks needed (tuitest pattern)
                // Use synchronized update to prevent flicker
                execute!(terminal.backend_mut(), BeginSynchronizedUpdate)
                    .map_err(TuiError::Render)?;
                terminal.draw(|f| draw(f, &app)).map_err(TuiError::Render)?;
                execute!(terminal.backend_mut(), EndSynchronizedUpdate)
                    .map_err(TuiError::Render)?;
            }
        }

        // Check if should quit (from app or signal handler)
        if app.should_quit || should_exit.load(Ordering::Relaxed) {
            app.should_quit = true; // Ensure clean shutdown
            break;
        }
    }

    // Cleanup
    app.disconnect_stream();

    // Restore terminal
    disable_raw_mode().map_err(TuiError::TerminalRestore)?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )
    .map_err(TuiError::TerminalRestore)?;
    terminal.show_cursor().map_err(TuiError::TerminalRestore)?;

    // Activate IME at exit
    activate_ime();

    Ok(RunResult::Quit)
}

/// Detect terminal query sequences and generate appropriate responses
/// Returns the combined response to send back to the PTY, if any
fn detect_terminal_query(
    data: &[u8],
    parser: &std::sync::Arc<std::sync::Mutex<vt100::Parser>>,
) -> Option<Vec<u8>> {
    // Look for common terminal query sequences:
    // CSI 6 n - Device Status Report (cursor position query)
    // CSI ? 6 n - DECXCPR (extended cursor position)
    // CSI c - Device Attributes (DA1)
    // CSI > c - Secondary Device Attributes (DA2)
    // CSI ? u - Kitty keyboard protocol query

    let csi_6n = b"\x1b[6n";           // Cursor position query
    let csi_0c = b"\x1b[c";            // DA1
    let csi_0_c = b"\x1b[0c";          // DA1 variant
    let csi_gt_c = b"\x1b[>c";         // DA2 (Secondary Device Attributes)
    let csi_gt_0c = b"\x1b[>0c";       // DA2 variant
    let csi_qmark_u = b"\x1b[?u";      // Kitty keyboard protocol query
    let osc_10_query = b"\x1b]10;?\x1b\\";  // OSC 10 foreground color query (ST terminator)
    let osc_10_query_bel = b"\x1b]10;?\x07"; // OSC 10 foreground color query (BEL terminator)
    let osc_11_query = b"\x1b]11;?\x1b\\";  // OSC 11 background color query (ST terminator)
    let osc_11_query_bel = b"\x1b]11;?\x07"; // OSC 11 background color query (BEL terminator)

    let mut responses: Vec<u8> = Vec::new();

    // Check for cursor position query (CSI 6 n)
    if contains_sequence(data, csi_6n) {
        // Get cursor position from parser
        let (row, col) = if let Ok(p) = parser.lock() {
            let screen = p.screen();
            (
                screen.cursor_position().0 as u16 + 1,
                screen.cursor_position().1 as u16 + 1,
            )
        } else {
            (1, 1) // Default to 1,1 if we can't get position
        };
        tracing::debug!("Responding to CSI 6n with position ({}, {})", row, col);
        responses.extend(format!("\x1b[{};{}R", row, col).into_bytes());
    }

    // Check for Device Attributes query (CSI c or CSI 0 c)
    if contains_sequence(data, csi_0c) || contains_sequence(data, csi_0_c) {
        // Respond as VT100 compatible terminal with advanced video
        // ESC [ ? 1 ; 2 c means "VT100 with Advanced Video Option"
        tracing::debug!("Responding to DA1 query");
        responses.extend(b"\x1b[?1;2c");
    }

    // Check for Secondary Device Attributes query (CSI > c or CSI > 0 c)
    if contains_sequence(data, csi_gt_c) || contains_sequence(data, csi_gt_0c) {
        // Respond as xterm-compatible: ESC [ > 41 ; version ; 0 c
        // 41 = xterm, version = 0 (unknown), 0 = no keyboard type
        tracing::debug!("Responding to DA2 query");
        responses.extend(b"\x1b[>41;0;0c");
    }

    // Check for Kitty keyboard protocol query
    if contains_sequence(data, csi_qmark_u) {
        // Respond with flags=0 (no enhanced keyboard)
        tracing::debug!("Responding to Kitty keyboard query");
        responses.extend(b"\x1b[?0u");
    }

    // Check for OSC 10 foreground color query
    if contains_sequence(data, osc_10_query) || contains_sequence(data, osc_10_query_bel) {
        // Respond with a default light gray foreground color
        // Format: OSC 10 ; rgb:RR/GG/BB ST
        tracing::debug!("Responding to OSC 10 foreground color query");
        responses.extend(b"\x1b]10;rgb:d0/d0/d0\x1b\\");
    }

    // Check for OSC 11 background color query
    if contains_sequence(data, osc_11_query) || contains_sequence(data, osc_11_query_bel) {
        // Respond with a default dark background color
        // Format: OSC 11 ; rgb:RR/GG/BB ST
        tracing::debug!("Responding to OSC 11 background color query");
        responses.extend(b"\x1b]11;rgb:1e/1e/1e\x1b\\");
    }

    if responses.is_empty() {
        None
    } else {
        Some(responses)
    }
}

/// Check if data contains a specific byte sequence
fn contains_sequence(data: &[u8], pattern: &[u8]) -> bool {
    if pattern.is_empty() || data.len() < pattern.len() {
        return false;
    }
    data.windows(pattern.len()).any(|w| w == pattern)
}
