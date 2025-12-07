//! Event subscription handlers

use crate::events::EventBroadcaster;
use ccm_proto::daemon::*;
use std::pin::Pin;
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream, Stream};
use tonic::{Response, Status};

/// Type alias for the event stream
pub type SubscribeEventsStream =
    Pin<Box<dyn Stream<Item = Result<Event, Status>> + Send + 'static>>;

/// Subscribe to daemon events
pub async fn subscribe_events(
    events: &EventBroadcaster,
    req: SubscribeEventsRequest,
) -> Result<Response<SubscribeEventsStream>, Status> {
    let repo_filter = req.repo_id;

    // Subscribe to event broadcaster
    let mut event_rx = events.subscribe();

    // Create output channel for filtered events
    let (tx, rx) = mpsc::channel::<Result<Event, Status>>(32);

    // Spawn task to forward events with filtering
    tokio::spawn(async move {
        loop {
            match event_rx.recv().await {
                Ok(event) => {
                    // Apply repo_id filter if specified
                    let should_send = match (&repo_filter, &event.event) {
                        (None, _) => true, // No filter, send all
                        (Some(filter_repo_id), Some(event::Event::SessionCreated(e))) => e
                            .session
                            .as_ref()
                            .map(|s| &s.repo_id == filter_repo_id)
                            .unwrap_or(false),
                        (Some(filter_repo_id), Some(event::Event::SessionDestroyed(e))) => {
                            &e.repo_id == filter_repo_id
                        }
                        // Name/status updates don't have repo_id, send all for now
                        // TUI can filter client-side if needed
                        (Some(_), Some(event::Event::SessionNameUpdated(_))) => true,
                        (Some(_), Some(event::Event::SessionStatusChanged(_))) => true,
                        // Worktree events
                        (Some(filter_repo_id), Some(event::Event::WorktreeAdded(e))) => e
                            .worktree
                            .as_ref()
                            .map(|w| &w.repo_id == filter_repo_id)
                            .unwrap_or(false),
                        (Some(filter_repo_id), Some(event::Event::WorktreeRemoved(e))) => {
                            &e.repo_id == filter_repo_id
                        }
                        (_, None) => false,
                    };

                    if should_send {
                        // Clone the Arc'd event
                        if tx.send(Ok((*event).clone())).await.is_err() {
                            // Client disconnected
                            break;
                        }
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("Event subscriber lagged, missed {} events", n);
                    // Continue receiving
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    // Broadcaster closed, exit
                    break;
                }
            }
        }
    });

    Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
}
