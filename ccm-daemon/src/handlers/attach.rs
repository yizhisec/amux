//! Session attach/detach handlers

use crate::error::{DaemonError, SessionError};
use crate::events::EventBroadcaster;
use crate::persistence;
use crate::session::SessionStatus;
use crate::state::SharedState;
use ccm_proto::daemon::*;
use std::pin::Pin;
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream, Stream, StreamExt};
use tonic::{Response, Status, Streaming};

/// Type alias for the attach session stream
pub type AttachSessionStream =
    Pin<Box<dyn Stream<Item = Result<AttachOutput, Status>> + Send + 'static>>;

/// Attach to a session
pub async fn attach_session(
    state: SharedState,
    events: EventBroadcaster,
    mut input_stream: Streaming<AttachInput>,
) -> Result<Response<AttachSessionStream>, Status> {
    // Get session ID from first message
    let first_msg = input_stream
        .next()
        .await
        .ok_or_else(|| Status::invalid_argument("No input received"))?
        .map_err(|e| Status::internal(e.to_string()))?;

    let session_id = first_msg.session_id.clone();

    // Verify session exists and start if needed (handles restored sessions)
    {
        let mut state = state.write().await;
        let session = state.sessions.get_mut(&session_id).ok_or_else(|| {
            Status::from(DaemonError::Session(SessionError::NotFound(
                session_id.clone(),
            )))
        })?;

        // Start session if not running
        if session.status() == SessionStatus::Stopped {
            tracing::info!("Starting stopped session: {}", session_id);
            session.start().map_err(|e| {
                Status::from(DaemonError::Session(SessionError::Start(e.to_string())))
            })?;

            // Save updated metadata (in case claude_session_id was auto-generated)
            if let Err(e) = persistence::save_session_meta(session) {
                tracing::warn!("Failed to persist session metadata: {}", e);
            }

            // Emit status changed event (Stopped -> Running)
            tracing::info!(
                "Emitting SessionStatusChanged event for {}: Stopped -> Running",
                session_id
            );
            events.emit_session_status_changed(
                session_id.clone(),
                2, // SESSION_STATUS_STOPPED
                1, // SESSION_STATUS_RUNNING
            );
        }
    }

    // Create output channel
    let (tx, rx) = mpsc::channel(32);
    let state_clone = state.clone();
    let session_id_clone = session_id.clone();

    // Send history buffer first
    {
        let state = state_clone.read().await;
        if let Some(session) = state.sessions.get(&session_id_clone) {
            let history = session.get_screen_state();
            if !history.is_empty() {
                let output = AttachOutput { data: history };
                let _ = tx.send(Ok(output)).await;
            }
        }
    }

    // Spawn task to read from PTY and send to client
    tokio::spawn(async move {
        let mut buf = [0u8; 4096];
        let mut save_counter = 0u32;
        let mut name_check_counter = 0u32;
        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

            // Periodically try to update session name from Claude's first message
            name_check_counter += 1;
            if name_check_counter >= 50 {
                // Check every ~0.5 seconds
                name_check_counter = 0;
                let mut state = state_clone.write().await;
                if let Some(session) = state.sessions.get_mut(&session_id_clone) {
                    if !session.name_updated_from_claude {
                        session.update_name_from_claude();
                        if session.name_updated_from_claude {
                            // Save updated metadata
                            let _ = persistence::save_session_meta(session);
                        }
                    }
                }
            }

            let state = state_clone.read().await;
            if let Some(session) = state.sessions.get(&session_id_clone) {
                match session.read(&mut buf) {
                    Ok(n) if n > 0 => {
                        // Store output in session buffer
                        session.process_output(&buf[..n]);

                        let output = AttachOutput {
                            data: buf[..n].to_vec(),
                        };
                        if tx.send(Ok(output)).await.is_err() {
                            // Client disconnected, save history before exit
                            let _ = persistence::save_session_history(session);
                            break;
                        }

                        // Periodically save history (every ~1 second of output)
                        save_counter += 1;
                        if save_counter >= 100 {
                            save_counter = 0;
                            let _ = persistence::save_session_history(session);
                        }
                    }
                    Ok(_) => {}
                    Err(_) => {
                        // PTY error, save history before exit
                        let _ = persistence::save_session_history(session);
                        break;
                    }
                }
            } else {
                break;
            }
        }
    });

    // Spawn task to read from client and write to PTY
    let state_clone = state.clone();
    tokio::spawn(async move {
        // Process first message data if any
        if !first_msg.data.is_empty() {
            let state = state_clone.read().await;
            if let Some(session) = state.sessions.get(&session_id) {
                session.write(&first_msg.data).ok();
            }
        }

        // Handle resize from first message
        if let (Some(rows), Some(cols)) = (first_msg.rows, first_msg.cols) {
            let state = state_clone.read().await;
            if let Some(session) = state.sessions.get(&session_id) {
                session.resize(rows as u16, cols as u16).ok();
            }
        }

        // Process remaining messages
        while let Some(Ok(msg)) = input_stream.next().await {
            let state = state_clone.read().await;
            if let Some(session) = state.sessions.get(&msg.session_id) {
                // Write data
                if !msg.data.is_empty() {
                    session.write(&msg.data).ok();
                }

                // Handle resize
                if let (Some(rows), Some(cols)) = (msg.rows, msg.cols) {
                    session.resize(rows as u16, cols as u16).ok();
                }
            }
        }
    });

    Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
}
