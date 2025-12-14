//! Error types for ccm-cli

use thiserror::Error;

/// Errors that can occur when connecting to the daemon
#[derive(Debug, Error)]
pub enum ClientError {
    #[error("daemon not running and failed to start: {0}")]
    DaemonStartFailed(#[source] std::io::Error),

    #[error("daemon failed to start within timeout")]
    DaemonTimeout,

    #[error("failed to connect to daemon: {0}")]
    ConnectionFailed(#[source] tonic::transport::Error),

    #[error("rpc error: {0}")]
    Rpc(#[from] tonic::Status),

    #[error("cannot find home directory")]
    NoHomeDir,
}

/// Errors that can occur in attach mode
#[derive(Debug, Error)]
pub enum AttachError {
    #[error("terminal error: {0}")]
    Terminal(#[source] std::io::Error),

    #[error("rpc error: {0}")]
    Rpc(#[from] tonic::Status),

    #[error("channel send error")]
    ChannelSend,
}

/// Errors that can occur in TUI operations
#[derive(Debug, Error)]
pub enum TuiError {
    #[error("terminal initialization failed: {0}")]
    TerminalInit(#[source] std::io::Error),

    #[error("terminal restore failed: {0}")]
    TerminalRestore(#[source] std::io::Error),

    #[error("event handling failed: {0}")]
    EventHandling(#[source] std::io::Error),

    #[error("render failed: {0}")]
    Render(#[source] std::io::Error),

    #[error("client error: {0}")]
    Client(#[from] ClientError),

    #[error("rpc error: {0}")]
    Rpc(#[from] tonic::Status),

    #[error("channel send error")]
    ChannelSend,

    #[error("config error: {0}")]
    Config(String),
}

/// Top-level CLI error type
#[derive(Debug, Error)]
pub enum CliError {
    #[error(transparent)]
    Client(#[from] ClientError),

    #[error(transparent)]
    Tui(#[from] TuiError),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result type alias for CLI operations
pub type Result<T> = std::result::Result<T, CliError>;
