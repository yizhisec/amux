//! Error types for ccm-daemon

use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur in repository operations
#[derive(Debug, Error)]
pub enum RepoError {
    #[error("path is not a git repository: {0}")]
    NotAGitRepo(PathBuf),

    #[error("repository not found: {0}")]
    NotFound(String),

    #[error("repository already exists: {0}")]
    AlreadyExists(String),

    #[error("failed to canonicalize path: {0}")]
    PathCanonicalize(#[source] std::io::Error),

    #[error("failed to load repos: {0}")]
    Load(#[source] std::io::Error),

    #[error("failed to save repos: {0}")]
    Save(#[source] std::io::Error),

    #[error("failed to parse repos file: {0}")]
    Parse(#[source] serde_json::Error),
}

/// Errors that can occur in git operations
#[derive(Debug, Error)]
pub enum GitError {
    #[error("failed to open repository at {path}: {source}")]
    OpenRepo {
        path: PathBuf,
        #[source]
        source: git2::Error,
    },

    #[error("repository has no working directory")]
    NoWorkdir,

    #[error("cannot get current branch name")]
    NoBranchName,

    #[error("branch '{0}' not found")]
    BranchNotFound(String),

    #[error("cannot delete branch '{branch}': {reason}")]
    CannotDeleteBranch { branch: String, reason: String },

    #[error("worktree already exists at: {0}")]
    WorktreeExists(PathBuf),

    #[error("path exists and is not a git worktree: {0}")]
    PathNotWorktree(PathBuf),

    #[error("git operation failed: {0}")]
    Git(#[from] git2::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Errors that can occur in session operations
#[derive(Debug, Error)]
pub enum SessionError {
    #[error("session not found: {0}")]
    NotFound(String),

    #[error("session already running: {0}")]
    AlreadyRunning(String),

    #[error("session not running: {0}")]
    NotRunning(String),

    #[error("failed to spawn pty: {0}")]
    PtySpawn(#[source] PtyError),

    #[error("failed to load session metadata: {0}")]
    LoadMeta(#[source] std::io::Error),

    #[error("failed to save session: {0}")]
    Save(#[source] std::io::Error),

    #[error("failed to parse session metadata: {0}")]
    ParseMeta(#[source] serde_json::Error),
}

/// Errors that can occur in PTY operations
#[derive(Debug, Error)]
pub enum PtyError {
    #[error("failed to open pty: {0}")]
    Open(#[source] nix::Error),

    #[error("failed to fork process: {0}")]
    Fork(#[source] nix::Error),

    #[error("failed to read from pty: {0}")]
    Read(#[source] nix::Error),

    #[error("failed to write to pty: {0}")]
    Write(#[source] nix::Error),

    #[error("failed to resize pty: {0}")]
    Resize(#[source] nix::Error),

    #[error("failed to kill process: {0}")]
    Kill(#[source] nix::Error),

    #[error("process already exited")]
    ProcessExited,
}

/// Errors that can occur in persistence operations
#[derive(Debug, Error)]
pub enum PersistenceError {
    #[error("failed to create data directory: {0}")]
    CreateDir(#[source] std::io::Error),

    #[error("failed to read file {path}: {source}")]
    ReadFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to write file {path}: {source}")]
    WriteFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse json: {0}")]
    ParseJson(#[from] serde_json::Error),

    #[error("failed to remove directory {path}: {source}")]
    RemoveDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Top-level daemon error type
#[derive(Debug, Error)]
pub enum DaemonError {
    #[error(transparent)]
    Repo(#[from] RepoError),

    #[error(transparent)]
    Git(#[from] GitError),

    #[error(transparent)]
    Session(#[from] SessionError),

    #[error(transparent)]
    Pty(#[from] PtyError),

    #[error(transparent)]
    Persistence(#[from] PersistenceError),

    #[error("internal error: {0}")]
    Internal(String),
}

/// Convert DaemonError to tonic::Status for gRPC responses
impl From<DaemonError> for tonic::Status {
    fn from(err: DaemonError) -> Self {
        match &err {
            DaemonError::Repo(RepoError::NotFound(_))
            | DaemonError::Session(SessionError::NotFound(_))
            | DaemonError::Git(GitError::BranchNotFound(_)) => {
                tonic::Status::not_found(err.to_string())
            }
            DaemonError::Repo(RepoError::AlreadyExists(_))
            | DaemonError::Session(SessionError::AlreadyRunning(_))
            | DaemonError::Git(GitError::WorktreeExists(_)) => {
                tonic::Status::already_exists(err.to_string())
            }
            DaemonError::Repo(RepoError::NotAGitRepo(_))
            | DaemonError::Git(GitError::CannotDeleteBranch { .. }) => {
                tonic::Status::invalid_argument(err.to_string())
            }
            _ => tonic::Status::internal(err.to_string()),
        }
    }
}

/// Result type alias for daemon operations
pub type Result<T> = std::result::Result<T, DaemonError>;
