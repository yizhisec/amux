//! gRPC client for CCM daemon

use crate::error::ClientError;
use ccm_proto::daemon::ccm_daemon_client::CcmDaemonClient;
use ccm_proto::daemon::*;
use hyper_util::rt::TokioIo;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;
use tokio::net::UnixStream;
use tonic::transport::{Channel, Endpoint, Uri};
use tower::service_fn;

type Result<T> = std::result::Result<T, ClientError>;

/// CCM daemon client
pub struct Client {
    inner: CcmDaemonClient<Channel>,
}

impl Client {
    /// Connect to the daemon via Unix socket, auto-starting if needed
    pub async fn connect() -> Result<Self> {
        let socket_path = Self::socket_path()?;

        // If socket doesn't exist, start daemon
        if !socket_path.exists() {
            Self::start_daemon()?;
            Self::wait_for_daemon(&socket_path).await?;
        }

        // Try to connect
        match Self::try_connect(&socket_path).await {
            Ok(client) => Ok(client),
            Err(_) => {
                // Connection failed, possibly stale socket - clean up and retry
                let _ = std::fs::remove_file(&socket_path);
                Self::start_daemon()?;
                Self::wait_for_daemon(&socket_path).await?;
                Self::try_connect(&socket_path).await
            }
        }
    }

    /// Start the daemon process in background
    fn start_daemon() -> Result<()> {
        // Try to find ccm-daemon in the same directory as current executable
        let daemon_path = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("ccm-daemon")))
            .filter(|p| p.exists());

        let daemon_cmd = daemon_path.as_deref().unwrap_or(Path::new("ccm-daemon"));

        Command::new(daemon_cmd)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(ClientError::DaemonStartFailed)?;
        Ok(())
    }

    /// Wait for daemon socket to become available
    async fn wait_for_daemon(socket_path: &Path) -> Result<()> {
        for _ in 0..50 {
            // Wait up to 5 seconds
            if socket_path.exists() {
                // Give daemon a moment to start listening
                tokio::time::sleep(Duration::from_millis(50)).await;
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Err(ClientError::DaemonTimeout)
    }

    /// Try to connect to daemon
    async fn try_connect(socket_path: &Path) -> Result<Self> {
        let path = socket_path.to_path_buf();
        let channel = Endpoint::try_from("http://[::]:50051")
            .map_err(ClientError::ConnectionFailed)?
            .connect_with_connector(service_fn(move |_: Uri| {
                let path = path.clone();
                async move {
                    let stream = UnixStream::connect(path).await?;
                    Ok::<_, std::io::Error>(TokioIo::new(stream))
                }
            }))
            .await
            .map_err(ClientError::ConnectionFailed)?;

        Ok(Self {
            inner: CcmDaemonClient::new(channel),
        })
    }

    fn socket_path() -> Result<PathBuf> {
        Ok(dirs::home_dir()
            .ok_or(ClientError::NoHomeDir)?
            .join(".ccm")
            .join("daemon.sock"))
    }

    // ============ Repo ============

    pub async fn add_repo(&mut self, path: &str) -> Result<RepoInfo> {
        let response = self
            .inner
            .add_repo(AddRepoRequest {
                path: path.to_string(),
            })
            .await?;
        Ok(response.into_inner())
    }

    pub async fn list_repos(&mut self) -> Result<Vec<RepoInfo>> {
        let response = self.inner.list_repos(Empty {}).await?;
        Ok(response.into_inner().repos)
    }

    #[allow(dead_code)]
    pub async fn remove_repo(&mut self, id: &str) -> Result<()> {
        self.inner
            .remove_repo(RemoveRepoRequest { id: id.to_string() })
            .await?;
        Ok(())
    }

    // ============ Worktree ============

    pub async fn list_worktrees(&mut self, repo_id: &str) -> Result<Vec<WorktreeInfo>> {
        let response = self
            .inner
            .list_worktrees(ListWorktreesRequest {
                repo_id: repo_id.to_string(),
            })
            .await?;
        Ok(response.into_inner().worktrees)
    }

    pub async fn create_worktree(&mut self, repo_id: &str, branch: &str) -> Result<WorktreeInfo> {
        let response = self
            .inner
            .create_worktree(CreateWorktreeRequest {
                repo_id: repo_id.to_string(),
                branch: branch.to_string(),
            })
            .await?;
        Ok(response.into_inner())
    }

    pub async fn remove_worktree(&mut self, repo_id: &str, branch: &str) -> Result<()> {
        self.inner
            .remove_worktree(RemoveWorktreeRequest {
                repo_id: repo_id.to_string(),
                branch: branch.to_string(),
            })
            .await?;
        Ok(())
    }

    pub async fn delete_branch(&mut self, repo_id: &str, branch: &str) -> Result<()> {
        self.inner
            .delete_branch(DeleteBranchRequest {
                repo_id: repo_id.to_string(),
                branch: branch.to_string(),
            })
            .await?;
        Ok(())
    }

    // ============ Session ============

    pub async fn list_sessions(
        &mut self,
        repo_id: Option<&str>,
        branch: Option<&str>,
    ) -> Result<Vec<SessionInfo>> {
        let response = self
            .inner
            .list_sessions(ListSessionsRequest {
                repo_id: repo_id.map(String::from),
                branch: branch.map(String::from),
            })
            .await?;
        Ok(response.into_inner().sessions)
    }

    pub async fn create_session(
        &mut self,
        repo_id: &str,
        branch: &str,
        name: Option<&str>,
    ) -> Result<SessionInfo> {
        let response = self
            .inner
            .create_session(CreateSessionRequest {
                repo_id: repo_id.to_string(),
                branch: branch.to_string(),
                name: name.map(String::from),
            })
            .await?;
        Ok(response.into_inner())
    }

    pub async fn rename_session(
        &mut self,
        session_id: &str,
        new_name: &str,
    ) -> Result<SessionInfo> {
        let response = self
            .inner
            .rename_session(RenameSessionRequest {
                session_id: session_id.to_string(),
                new_name: new_name.to_string(),
            })
            .await?;
        Ok(response.into_inner())
    }

    pub async fn destroy_session(&mut self, session_id: &str) -> Result<()> {
        self.inner
            .destroy_session(DestroySessionRequest {
                session_id: session_id.to_string(),
            })
            .await?;
        Ok(())
    }

    // ============ Attach ============

    pub fn inner_mut(&mut self) -> &mut CcmDaemonClient<Channel> {
        &mut self.inner
    }

    // ============ Events ============

    /// Subscribe to events from the daemon
    pub async fn subscribe_events(
        &mut self,
        repo_id: Option<&str>,
    ) -> Result<tonic::Streaming<Event>> {
        let response = self
            .inner
            .subscribe_events(SubscribeEventsRequest {
                repo_id: repo_id.map(String::from),
            })
            .await?;
        Ok(response.into_inner())
    }
}
