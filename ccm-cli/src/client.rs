//! gRPC client for CCM daemon

use anyhow::{Context, Result};
use ccm_proto::daemon::ccm_daemon_client::CcmDaemonClient;
use ccm_proto::daemon::*;
use hyper_util::rt::TokioIo;
use std::path::PathBuf;
use tokio::net::UnixStream;
use tonic::transport::{Channel, Endpoint, Uri};
use tower::service_fn;

/// CCM daemon client
pub struct Client {
    inner: CcmDaemonClient<Channel>,
}

impl Client {
    /// Connect to the daemon via Unix socket
    pub async fn connect() -> Result<Self> {
        let socket_path = Self::socket_path();

        // Check if daemon is running
        if !socket_path.exists() {
            anyhow::bail!("Daemon is not running. Start it with: ccm-daemon");
        }

        // Connect via Unix socket
        let channel = Endpoint::try_from("http://[::]:50051")?
            .connect_with_connector(service_fn(move |_: Uri| {
                let path = socket_path.clone();
                async move {
                    let stream = UnixStream::connect(path).await?;
                    Ok::<_, std::io::Error>(TokioIo::new(stream))
                }
            }))
            .await
            .context("Failed to connect to daemon")?;

        Ok(Self {
            inner: CcmDaemonClient::new(channel),
        })
    }

    fn socket_path() -> PathBuf {
        dirs::home_dir()
            .expect("Cannot find home directory")
            .join(".ccm")
            .join("daemon.sock")
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

    #[allow(dead_code)]
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
}
