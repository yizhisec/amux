//! File system watching for git worktrees
//!
//! This module implements file system monitoring for git repositories to detect
//! changes that affect git status and emit events for real-time UI updates.

use crate::events::EventBroadcaster;
use anyhow::Result;
use notify::{RecursiveMode, Watcher};
use notify_debouncer_full::{new_debouncer, DebouncedEvent, Debouncer, FileIdMap};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{debug, error};

/// Debounce duration for file change events (300ms)
const DEBOUNCE_DURATION: Duration = Duration::from_millis(300);

/// File watcher for a single worktree
pub struct GitFileWatcher {
    repo_id: String,
    branch: String,
    _debouncer: Debouncer<notify::RecommendedWatcher, FileIdMap>,
}

impl GitFileWatcher {
    /// Create a new file watcher for a worktree
    pub fn new(
        repo_id: String,
        branch: String,
        worktree_path: PathBuf,
        events: EventBroadcaster,
    ) -> Result<Self> {
        let repo_id_clone = repo_id.clone();
        let branch_clone = branch.clone();

        // Create debounced watcher
        let debouncer = new_debouncer(
            DEBOUNCE_DURATION,
            None,
            move |result: Result<Vec<DebouncedEvent>, Vec<notify::Error>>| {
                match result {
                    Ok(debounced_events) => {
                        debug!(
                            "File watcher triggered for {}/{}: {} events",
                            repo_id_clone,
                            branch_clone,
                            debounced_events.len()
                        );

                        // Filter out non-relevant events
                        let relevant_events: Vec<_> = debounced_events
                            .iter()
                            .filter(|e| Self::is_relevant_for_git_status(&e.event))
                            .collect();

                        if !relevant_events.is_empty() {
                            debug!(
                                "Git status changed for {}/{} ({} relevant out of {} total events)",
                                repo_id_clone,
                                branch_clone,
                                relevant_events.len(),
                                debounced_events.len()
                            );
                            events.emit_git_status_changed(
                                repo_id_clone.clone(),
                                branch_clone.clone(),
                            );
                        } else {
                            debug!(
                                "No relevant changes for {}/{} (filtered out {} events)",
                                repo_id_clone,
                                branch_clone,
                                debounced_events.len()
                            );
                        }
                    }
                    Err(errors) => {
                        error!("File watcher errors: {:?}", errors);
                    }
                }
            },
        )?;

        let mut watcher = Self {
            repo_id,
            branch,
            _debouncer: debouncer,
        };

        // Watch the worktree directory recursively
        watcher
            ._debouncer
            .watcher()
            .watch(&worktree_path, RecursiveMode::Recursive)?;

        // Also watch .git metadata for staging/branch changes
        let git_dir = worktree_path.join(".git");
        if git_dir.is_dir() {
            // Main worktree: watch .git/index and .git/HEAD
            let git_index = git_dir.join("index");
            let git_head = git_dir.join("HEAD");

            if git_index.exists() {
                watcher
                    ._debouncer
                    .watcher()
                    .watch(&git_index, RecursiveMode::NonRecursive)?;
            }

            if git_head.exists() {
                watcher
                    ._debouncer
                    .watcher()
                    .watch(&git_head, RecursiveMode::NonRecursive)?;
            }
        } else if git_dir.is_file() {
            // Additional worktree: .git is a file pointing to the real git dir
            // Watch it to detect changes
            watcher
                ._debouncer
                .watcher()
                .watch(&git_dir, RecursiveMode::NonRecursive)?;
        }

        debug!(
            "Started file watcher for {}/{} at {}",
            watcher.repo_id,
            watcher.branch,
            worktree_path.display()
        );

        Ok(watcher)
    }

    /// Check if a notify event is relevant for git status
    ///
    /// This filters out temporary files, IDE files, git internals, etc.
    fn is_relevant_for_git_status(event: &notify::Event) -> bool {
        event.paths.iter().any(|path| {
            let path_str = path.to_string_lossy();

            // Git metadata that affects status
            if path_str.contains(".git/index")
                || path_str.contains(".git/HEAD")
                || path_str.contains(".git/MERGE_HEAD")
                || path_str.contains(".git/CHERRY_PICK_HEAD")
            {
                return true;
            }

            // .gitignore changes affect untracked files
            if path_str.ends_with(".gitignore") {
                return true;
            }

            // Working directory files (but exclude git internals)
            if path_str.contains(".git/") {
                // Exclude git internal directories that don't affect status
                return !path_str.contains(".git/objects/")
                    && !path_str.contains(".git/logs/")
                    && !path_str.contains(".git/refs/remotes/")
                    && !path_str.contains(".git/config")
                    && !path_str.contains(".git/hooks/");
            }

            // Working directory files - exclude temp/IDE files
            !path_str.ends_with('~')
                && !path_str.contains(".swp")
                && !path_str.contains(".tmp")
                && !path_str.contains("/.idea/")
                && !path_str.contains("/.vscode/")
                && !path_str.contains("/.fleet/")
                && !path_str.ends_with(".DS_Store")
        })
    }
}

/// Manager for all file watchers
pub struct WatcherManager {
    watchers: Arc<Mutex<HashMap<String, GitFileWatcher>>>,
    events: EventBroadcaster,
}

impl WatcherManager {
    /// Create a new watcher manager
    pub fn new(events: EventBroadcaster) -> Self {
        Self {
            watchers: Arc::new(Mutex::new(HashMap::new())),
            events,
        }
    }

    /// Start watching a worktree
    ///
    /// The key is formatted as "repo_id/branch" for easy lookup.
    pub async fn watch_worktree(
        &self,
        repo_id: String,
        branch: String,
        worktree_path: PathBuf,
    ) -> Result<()> {
        let key = format!("{}/{}", repo_id, branch);

        let watcher = GitFileWatcher::new(repo_id, branch, worktree_path, self.events.clone())?;

        self.watchers.lock().await.insert(key, watcher);
        Ok(())
    }

    /// Stop watching a worktree
    pub async fn unwatch_worktree(&self, repo_id: &str, branch: &str) {
        let key = format!("{}/{}", repo_id, branch);
        if let Some(_watcher) = self.watchers.lock().await.remove(&key) {
            debug!("Stopped watching {}/{}", repo_id, branch);
        }
    }

    /// Stop all watchers
    #[allow(dead_code)]
    pub async fn stop_all(&self) {
        self.watchers.lock().await.clear();
        debug!("Stopped all file watchers");
    }
}
