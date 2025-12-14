//! Event broadcasting system for real-time updates

use amux_proto::daemon::{
    Event, GitStatusChangedEvent, SessionCreatedEvent, SessionDestroyedEvent,
    SessionNameUpdatedEvent, SessionStatusChangedEvent, WorktreeAddedEvent, WorktreeInfo,
    WorktreeRemovedEvent,
};
use std::sync::Arc;
use tokio::sync::broadcast;

/// Event channel capacity
const EVENT_CHANNEL_CAPACITY: usize = 256;

/// Event broadcaster for distributing events to subscribers
#[derive(Clone)]
pub struct EventBroadcaster {
    sender: broadcast::Sender<Arc<Event>>,
}

impl EventBroadcaster {
    /// Create a new event broadcaster
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        Self { sender }
    }

    /// Subscribe to events
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<Event>> {
        self.sender.subscribe()
    }

    /// Broadcast an event to all subscribers
    pub fn broadcast(&self, event: Event) {
        // Ignore send errors (no subscribers is fine)
        let _ = self.sender.send(Arc::new(event));
    }

    /// Emit a session created event
    pub fn emit_session_created(&self, session: amux_proto::daemon::SessionInfo) {
        self.broadcast(Event {
            event: Some(amux_proto::daemon::event::Event::SessionCreated(
                SessionCreatedEvent {
                    session: Some(session),
                },
            )),
        });
    }

    /// Emit a session destroyed event
    pub fn emit_session_destroyed(&self, session_id: String, repo_id: String, branch: String) {
        self.broadcast(Event {
            event: Some(amux_proto::daemon::event::Event::SessionDestroyed(
                SessionDestroyedEvent {
                    session_id,
                    repo_id,
                    branch,
                },
            )),
        });
    }

    /// Emit a session name updated event
    pub fn emit_session_name_updated(
        &self,
        session_id: String,
        old_name: String,
        new_name: String,
    ) {
        self.broadcast(Event {
            event: Some(amux_proto::daemon::event::Event::SessionNameUpdated(
                SessionNameUpdatedEvent {
                    session_id,
                    old_name,
                    new_name,
                },
            )),
        });
    }

    /// Emit a session status changed event
    #[allow(dead_code)]
    pub fn emit_session_status_changed(
        &self,
        session_id: String,
        old_status: i32,
        new_status: i32,
    ) {
        self.broadcast(Event {
            event: Some(amux_proto::daemon::event::Event::SessionStatusChanged(
                SessionStatusChangedEvent {
                    session_id,
                    old_status,
                    new_status,
                },
            )),
        });
    }

    /// Emit a worktree added event
    pub fn emit_worktree_added(&self, worktree: WorktreeInfo) {
        self.broadcast(Event {
            event: Some(amux_proto::daemon::event::Event::WorktreeAdded(
                WorktreeAddedEvent {
                    worktree: Some(worktree),
                },
            )),
        });
    }

    /// Emit a worktree removed event
    pub fn emit_worktree_removed(&self, repo_id: String, branch: String) {
        self.broadcast(Event {
            event: Some(amux_proto::daemon::event::Event::WorktreeRemoved(
                WorktreeRemovedEvent { repo_id, branch },
            )),
        });
    }

    /// Emit a git status changed event
    pub fn emit_git_status_changed(&self, repo_id: String, branch: String) {
        self.broadcast(Event {
            event: Some(amux_proto::daemon::event::Event::GitStatusChanged(
                GitStatusChangedEvent { repo_id, branch },
            )),
        });
    }
}

impl Default for EventBroadcaster {
    fn default() -> Self {
        Self::new()
    }
}
