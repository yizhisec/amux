//! CCM CLI - Claude Code Manager TUI

// Allow large error types - tonic::Status is large but boxing it would add complexity
#![allow(clippy::result_large_err)]

mod attach;
mod client;
pub mod error;
mod tui;

use client::Client;
use error::CliError;
use tracing::debug;

fn init_logging() {
    // TUI takes over stdout/stderr, so log to file if CCM_LOG is set
    // Usage: CCM_LOG=debug ccm
    if std::env::var("CCM_LOG").is_ok() {
        let log_dir = dirs::home_dir()
            .map(|h| h.join(".ccm").join("logs"))
            .unwrap_or_else(|| std::path::PathBuf::from("/tmp/ccm-logs"));

        let _ = std::fs::create_dir_all(&log_dir);
        let log_file = log_dir.join("cli.log");

        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_file)
            .expect("Failed to open log file");

        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::from_env("CCM_LOG")
                    .add_directive("ccm_cli=debug".parse().unwrap()),
            )
            .with_writer(file)
            .with_ansi(false)
            .init();
    }
}

#[tokio::main]
async fn main() -> Result<(), CliError> {
    init_logging();
    debug!("CCM CLI starting");

    // Connect to daemon
    let client = match Client::connect().await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: {}", e);
            eprintln!("Make sure ccm-daemon is running.");
            std::process::exit(1);
        }
    };

    // Auto-detect and add current directory if it's a git repo
    let mut app = tui::App::new(client).await?;
    if let Ok(cwd) = std::env::current_dir() {
        if cwd.join(".git").exists() {
            // Try to add, ignore errors (might already be added)
            let _ = app.client.add_repo(cwd.to_str().unwrap()).await;
            // Refresh to pick up the newly added repo
            let _ = app.refresh_all().await;
        }
    }

    // Run TUI
    tui::run_with_client(app).await?;

    Ok(())
}
