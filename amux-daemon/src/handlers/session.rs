//! Session management handlers

use super::get_repo_and_open_git;
use crate::error::{DaemonError, SessionError};
use crate::events::EventBroadcaster;
use crate::git::GitOps;
use crate::persistence;
use crate::providers::ProviderRef;
use crate::session::{self, Session, SessionStatus};
use crate::state::SharedState;
use amux_config::{DEFAULT_TERMINAL_COLS, DEFAULT_TERMINAL_ROWS};
use amux_proto::daemon::*;
use tonic::{Response, Status};

// Proto session status enum values
mod session_status {
    pub enum SessionStatus {
        Running = 1,
        Stopped = 2,
    }
}

/// List all sessions (optionally filtered by repo_id and/or branch)
pub async fn list_sessions(
    state: &SharedState,
    req: ListSessionsRequest,
) -> Result<Response<ListSessionsResponse>, Status> {
    // Try to update session names from provider's first message
    {
        let mut state = state.write().await;
        for session in state.sessions.values_mut() {
            if !session.name_updated_from_provider {
                session.update_name_from_provider();
                if session.name_updated_from_provider {
                    let _ = persistence::save_session_meta(session);
                }
            }
        }
    }

    let state = state.read().await;
    let sessions: Vec<SessionInfo> = state
        .sessions
        .values()
        .filter(|s| {
            req.repo_id.as_ref().is_none_or(|id| &s.repo_id == id)
                && req.branch.as_ref().is_none_or(|b| &s.branch == b)
        })
        .map(|s| SessionInfo {
            id: s.id.clone(),
            name: s.name.clone(),
            repo_id: s.repo_id.clone(),
            branch: s.branch.clone(),
            worktree_path: s.worktree_path.to_string_lossy().to_string(),
            status: match s.status() {
                SessionStatus::Running => session_status::SessionStatus::Running as i32,
                SessionStatus::Stopped => session_status::SessionStatus::Stopped as i32,
            },
            provider_session_id: s.provider_session_id().map(|s| s.to_string()),
            is_shell: Some(s.is_shell()),
            provider: Some(s.provider.clone()),
        })
        .collect();

    Ok(Response::new(ListSessionsResponse { sessions }))
}

/// Create a new session
pub async fn create_session(
    state: &SharedState,
    events: &EventBroadcaster,
    req: CreateSessionRequest,
) -> Result<Response<SessionInfo>, Status> {
    let (repo, git_repo) = get_repo_and_open_git(state, &req.repo_id).await?;

    let mut state_guard = state.write().await;

    // Find or create worktree
    let worktree_path = match GitOps::find_worktree_path(&git_repo, &req.branch) {
        Some(path) => path,
        None => {
            // Auto-create worktree (uses HEAD as base for new branch)
            GitOps::create_worktree(&git_repo, &req.branch, &repo.path, None)
                .map_err(|e| Status::from(DaemonError::from(e)))?
        }
    };

    // Create session with auto-generated provider session ID
    let id = session::generate_session_id();
    let is_shell = req.is_shell.unwrap_or(false);
    let prompt = req.prompt;

    // Validate provider and model for non-shell sessions
    let (provider, model) = if is_shell {
        // Shell sessions don't use providers
        ("shell".to_string(), None)
    } else {
        // Validate provider and model using cached registry
        let provider_ref = ProviderRef::new(
            &state_guard.provider_registry,
            req.provider.as_deref(),
            req.model.as_deref(),
        )
        .map_err(|e| Status::invalid_argument(e.to_string()))?;

        (provider_ref.name, Some(provider_ref.model))
    };

    // Generate session name based on provider (e.g., claude-1, codex-2)
    let existing_names: Vec<String> = state_guard
        .sessions
        .values()
        .filter(|s| s.repo_id == req.repo_id && s.branch == req.branch)
        .map(|s| s.name.clone())
        .collect();

    let name = req
        .name
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| session::generate_session_name(&provider, &existing_names));

    // Debug: log the provider being used
    tracing::info!("Creating session with provider: {}, model: {:?}", provider, model);

    // Shell sessions don't need provider session ID
    let provider_session_id = if is_shell {
        None
    } else {
        Some(uuid::Uuid::new_v4().to_string())
    };

    let mut session = Session::new(
        id.clone(),
        name.clone(),
        req.repo_id.clone(),
        req.branch.clone(),
        worktree_path.clone(),
        provider,
        provider_session_id.clone(),
        is_shell,
        model,
        prompt,
    );

    // Start session with provided size (or defaults)
    let rows = req.rows.map(|r| r as u16).unwrap_or(DEFAULT_TERMINAL_ROWS);
    let cols = req.cols.map(|c| c as u16).unwrap_or(DEFAULT_TERMINAL_COLS);
    session
        .start_with_size(&state_guard.provider_registry, rows, cols)
        .map_err(|e| Status::from(DaemonError::Session(SessionError::Start(e.to_string()))))?;

    let info = SessionInfo {
        id: session.id.clone(),
        name: session.name.clone(),
        repo_id: session.repo_id.clone(),
        branch: session.branch.clone(),
        worktree_path: session.worktree_path.to_string_lossy().to_string(),
        status: session_status::SessionStatus::Running as i32,
        provider_session_id,
        is_shell: Some(session.is_shell()),
        provider: Some(session.provider.clone()),
    };

    // Save session metadata to disk
    if let Err(e) = persistence::save_session_meta(&session) {
        tracing::warn!("Failed to persist session metadata: {}", e);
    }

    state_guard.sessions.insert(id, session);

    // Emit session created event
    events.emit_session_created(info.clone());

    Ok(Response::new(info))
}

/// Rename a session
pub async fn rename_session(
    state: &SharedState,
    events: &EventBroadcaster,
    req: RenameSessionRequest,
) -> Result<Response<SessionInfo>, Status> {
    let mut state = state.write().await;

    let session = state.sessions.get_mut(&req.session_id).ok_or_else(|| {
        Status::from(DaemonError::Session(SessionError::NotFound(
            req.session_id.clone(),
        )))
    })?;

    let old_name = session.name.clone();
    session.name = req.new_name.clone();
    session.name_updated_from_provider = true; // Mark as manually updated

    // Save updated metadata
    if let Err(e) = persistence::save_session_meta(session) {
        tracing::warn!("Failed to persist session metadata after rename: {}", e);
    }

    let info = SessionInfo {
        id: session.id.clone(),
        name: session.name.clone(),
        repo_id: session.repo_id.clone(),
        branch: session.branch.clone(),
        worktree_path: session.worktree_path.to_string_lossy().to_string(),
        status: match session.status() {
            SessionStatus::Running => session_status::SessionStatus::Running as i32,
            SessionStatus::Stopped => session_status::SessionStatus::Stopped as i32,
        },
        provider_session_id: session.provider_session_id().map(|s| s.to_string()),
        is_shell: Some(session.is_shell()),
        provider: Some(session.provider.clone()),
    };

    // Emit session name updated event
    events.emit_session_name_updated(req.session_id, old_name, req.new_name);

    Ok(Response::new(info))
}

/// Destroy a session
pub async fn destroy_session(
    state: &SharedState,
    events: &EventBroadcaster,
    req: DestroySessionRequest,
) -> Result<Response<Empty>, Status> {
    let mut state = state.write().await;

    let mut session = state.sessions.remove(&req.session_id).ok_or_else(|| {
        Status::from(DaemonError::Session(SessionError::NotFound(
            req.session_id.clone(),
        )))
    })?;

    // Capture info for event before stopping
    let session_id = session.id.clone();
    let repo_id = session.repo_id.clone();
    let branch = session.branch.clone();

    if let Err(e) = session.stop() {
        tracing::warn!("Failed to stop session {}: {}", req.session_id, e);
    }

    // Delete persisted session data
    if let Err(e) = persistence::delete_session_data(&req.session_id) {
        tracing::warn!("Failed to delete session data: {}", e);
    }

    // Emit session destroyed event
    events.emit_session_destroyed(session_id, repo_id, branch);

    Ok(Response::new(Empty {}))
}

/// Stop a session (kill PTY but keep metadata)
pub async fn stop_session(
    state: &SharedState,
    events: &EventBroadcaster,
    req: StopSessionRequest,
) -> Result<Response<Empty>, Status> {
    let mut state = state.write().await;

    let session = state.sessions.get_mut(&req.session_id).ok_or_else(|| {
        Status::from(DaemonError::Session(SessionError::NotFound(
            req.session_id.clone(),
        )))
    })?;

    // Get status before stopping
    let old_status = match session.status() {
        SessionStatus::Running => session_status::SessionStatus::Running as i32,
        SessionStatus::Stopped => session_status::SessionStatus::Stopped as i32,
    };

    // Stop session (kill PTY)
    if let Err(e) = session.stop() {
        tracing::warn!("Failed to stop session {}: {}", req.session_id, e);
        return Err(Status::internal(format!("Failed to stop session: {}", e)));
    }

    // Save terminal history
    if let Err(e) = persistence::save_session_history(session) {
        tracing::warn!("Failed to save session history: {}", e);
    }

    let new_status = session_status::SessionStatus::Stopped as i32;

    // Emit status changed event if status actually changed
    if old_status != new_status {
        events.emit_session_status_changed(req.session_id, old_status, new_status);
    }

    Ok(Response::new(Empty {}))
}
