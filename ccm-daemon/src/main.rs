//! CCM Daemon - Claude Code Manager Daemon

mod git;
mod pty;
mod repo;
mod server;
mod session;
mod state;

use crate::server::CcmDaemonService;
use crate::state::{AppState, SharedState};
use anyhow::Result;
use ccm_proto::daemon::ccm_daemon_server::CcmDaemonServer;
use std::sync::Arc;
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
        for r in repos {
            state_guard.repos.insert(r.id.clone(), r);
        }
        info!("Loaded {} repos from disk", state_guard.repos.len());
    }

    // Create Unix socket listener
    let listener = UnixListener::bind(&socket_path)?;
    info!("Listening on {:?}", socket_path);

    let incoming = UnixListenerStream::new(listener);

    // Create gRPC service
    let service = CcmDaemonService::new(state);

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
