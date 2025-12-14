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
use ccm_config::{Config, KeybindMap};
use ccm_proto::daemon::Event as DaemonEvent;
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
    RightPanelView, SidebarState, TerminalState, TodoState,
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
    /// Focus restoration stack for popups/dialogs
    pub saved_focus_stack: Vec<Focus>,

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

            // 2. Terminal PTY output - just process data, no flags needed
            Some(data) = async {
                match app.terminal_stream.as_mut() {
                    Some(stream) => stream.output_rx.recv().await,
                    None => std::future::pending().await,
                }
            } => {
                if let Ok(mut parser) = app.terminal.parser.lock() {
                    parser.process(&data);
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
