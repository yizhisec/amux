//! CCM CLI - Claude Code Manager TUI

mod client;
pub mod error;
mod tui;

use client::Client;
use error::CliError;

#[tokio::main]
async fn main() -> Result<(), CliError> {
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
