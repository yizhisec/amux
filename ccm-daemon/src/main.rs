//! CCM Daemon - Claude Code Manager Daemon

mod claude_session;
mod diff;
pub mod error;
mod events;
mod git;
mod persistence;
mod pty;
mod repo;
mod review;
mod server;
mod session;
mod state;
mod todo;

use crate::events::EventBroadcaster;
use crate::server::CcmDaemonService;
use crate::state::{AppState, SharedState};
use anyhow::Result;
use ccm_proto::daemon::ccm_daemon_server::CcmDaemonServer;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UnixListener;
use tokio::sync::RwLock;
use tokio_stream::wrappers::UnixListenerStream;
use tonic::transport::Server;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("ccm_daemon=info".parse().unwrap()),
        )
        .init();

    // Ensure data directory exists
    AppState::ensure_data_dir()?;

    // Remove stale socket
    let socket_path = AppState::socket_path();
    if socket_path.exists() {
        std::fs::remove_file(&socket_path)?;
    }

    // Write PID file
    let pid_file = AppState::pid_file();
    std::fs::write(&pid_file, std::process::id().to_string())?;

    // Initialize state
    let state: SharedState = Arc::new(RwLock::new(AppState::new()));

    // Load persisted repos
    if let Ok(repos) = repo::load_repos() {
        let mut state_guard = state.write().await;
        let original_count = repos.len();
        let mut valid_repos = Vec::new();
        let mut seen_canonical_paths: std::collections::HashMap<std::path::PathBuf, String> =
            std::collections::HashMap::new();

        for r in repos {
            // Check if repo path still exists
            if !r.path.exists() {
                info!(
                    "Removing orphaned repo {} (path not found: {:?})",
                    r.name, r.path
                );
                continue;
            }

            // Check for duplicate canonical paths
            if let Ok(canonical_path) = r.path.canonicalize() {
                if let Some(existing_id) = seen_canonical_paths.get(&canonical_path) {
                    info!(
                        "Removing duplicate repo {} (same path as repo {})",
                        r.id, existing_id
                    );
                    continue;
                }
                seen_canonical_paths.insert(canonical_path, r.id.clone());
            }

            valid_repos.push(r.clone());
            state_guard.repos.insert(r.id.clone(), r);
        }
        // Save cleaned repo list if any were removed
        if valid_repos.len() < original_count {
            let _ = repo::save_repos(&valid_repos);
        }
        info!("Loaded {} repos from disk", state_guard.repos.len());
    }

    // Load persisted sessions
    if let Ok(sessions) = persistence::load_all_sessions() {
        let mut state_guard = state.write().await;
        for meta in sessions {
            // Check if repo still exists
            if !state_guard.repos.contains_key(&meta.repo_id) {
                info!("Removing orphaned session {} (repo not found)", meta.id);
                let _ = persistence::delete_session_data(&meta.id);
                continue;
            }
            // Check if worktree path still exists
            if !meta.worktree_path.exists() {
                info!(
                    "Removing orphaned session {} (worktree not found: {:?})",
                    meta.id, meta.worktree_path
                );
                let _ = persistence::delete_session_data(&meta.id);
                continue;
            }
            let session = session::Session::from_meta(meta);
            // Load terminal history
            if let Err(e) = session.load_history() {
                info!("Failed to load history for session {}: {}", session.id, e);
            }
            state_guard.sessions.insert(session.id.clone(), session);
        }
        info!("Restored {} sessions", state_guard.sessions.len());
    }

    // Create event broadcaster (before background task so it can emit events)
    let events = EventBroadcaster::new();

    // Spawn background task to update session names from Claude
    let state_for_bg = state.clone();
    let events_for_bg = events.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            interval.tick().await;
            let mut state_guard = state_for_bg.write().await;
            for session in state_guard.sessions.values_mut() {
                // Skip shell sessions - they don't have Claude names
                if session.is_shell {
                    continue;
                }
                if !session.name_updated_from_claude {
                    let old_name = session.name.clone();
                    session.update_name_from_claude();
                    if session.name_updated_from_claude {
                        let _ = persistence::save_session_meta(session);
                        // Emit name updated event
                        events_for_bg.emit_session_name_updated(
                            session.id.clone(),
                            old_name,
                            session.name.clone(),
                        );
                    }
                }
            }
        }
    });

    // Create Unix socket listener
    let listener = UnixListener::bind(&socket_path)?;
    info!("Listening on {:?}", socket_path);

    let incoming = UnixListenerStream::new(listener);

    // Create gRPC service
    let service = CcmDaemonService::new(state, events);

    // Start server
    Server::builder()
        .add_service(CcmDaemonServer::new(service))
        .serve_with_incoming(incoming)
        .await?;

    // Cleanup
    std::fs::remove_file(&socket_path).ok();
    std::fs::remove_file(&pid_file).ok();

    Ok(())
}
