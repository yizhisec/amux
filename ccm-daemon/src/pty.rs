//! PTY process management

use crate::error::PtyError;
use nix::fcntl::{fcntl, FcntlArg, OFlag};
use nix::libc;
use nix::pty::{openpty, Winsize};
use nix::sys::signal::{kill, Signal};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::{close, dup2, execvp, fork, read, setsid, write, ForkResult, Pid};
use std::ffi::CString;
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd, RawFd};
use std::path::Path;

/// Claude session mode
#[derive(Debug, Clone)]
pub enum ClaudeSessionMode {
    /// Run plain shell (no Claude)
    Shell,
    /// No specific session - just run claude
    None,
    /// New session with specific ID
    New(String),
    /// Resume existing session
    Resume(String),
}

/// PTY process handle
pub struct PtyProcess {
    /// Master file descriptor
    master_fd: OwnedFd,
    /// Child process ID
    child_pid: Pid,
}

impl PtyProcess {
    /// Spawn a new PTY process running `claude` in the given working directory
    #[allow(dead_code)]
    pub fn spawn(working_dir: &Path) -> Result<Self, PtyError> {
        Self::spawn_with_session(working_dir, ClaudeSessionMode::None)
    }

    /// Spawn a new PTY process running `claude` with optional session ID
    pub fn spawn_with_session(
        working_dir: &Path,
        session_mode: ClaudeSessionMode,
    ) -> Result<Self, PtyError> {
        let winsize = Winsize {
            ws_row: 24,
            ws_col: 80,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };

        // Open PTY
        let pty = openpty(&winsize, None).map_err(PtyError::Open)?;

        // Get raw fds before fork (consume OwnedFd to avoid double-close)
        let master_raw = pty.master.into_raw_fd();
        let slave_raw = pty.slave.into_raw_fd();

        // Fork
        match unsafe { fork() }.map_err(PtyError::Fork)? {
            ForkResult::Parent { child } => {
                // Parent: close slave, keep master
                close(slave_raw).ok();

                // Set master to non-blocking
                fcntl(master_raw, FcntlArg::F_SETFL(OFlag::O_NONBLOCK)).ok();

                // Wrap master_raw back into OwnedFd
                let master_fd = unsafe { OwnedFd::from_raw_fd(master_raw) };

                Ok(Self {
                    master_fd,
                    child_pid: child,
                })
            }
            ForkResult::Child => {
                // Child: setup PTY and exec claude
                close(master_raw).ok();

                // Create new session
                setsid().ok();

                // Set controlling terminal
                unsafe {
                    libc::ioctl(slave_raw, libc::TIOCSCTTY, 0);
                }

                // Redirect stdio to slave
                dup2(slave_raw, libc::STDIN_FILENO).ok();
                dup2(slave_raw, libc::STDOUT_FILENO).ok();
                dup2(slave_raw, libc::STDERR_FILENO).ok();

                if slave_raw > libc::STDERR_FILENO {
                    close(slave_raw).ok();
                }

                // Change to working directory
                std::env::set_current_dir(working_dir).ok();

                // Build command with args based on session mode
                let (cmd, args): (CString, Vec<CString>) = match session_mode {
                    ClaudeSessionMode::Shell => {
                        // Run user's default shell
                        let shell =
                            std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
                        let cmd = CString::new(shell.clone()).unwrap();
                        let args = vec![CString::new(shell).unwrap()];
                        (cmd, args)
                    }
                    ClaudeSessionMode::None => {
                        let cmd = CString::new("claude").unwrap();
                        (cmd.clone(), vec![cmd])
                    }
                    ClaudeSessionMode::New(id) => {
                        let cmd = CString::new("claude").unwrap();
                        let args = vec![
                            cmd.clone(),
                            CString::new("--session-id").unwrap(),
                            CString::new(id).unwrap(),
                        ];
                        (cmd, args)
                    }
                    ClaudeSessionMode::Resume(id) => {
                        let cmd = CString::new("claude").unwrap();
                        let args = vec![
                            cmd.clone(),
                            CString::new("--resume").unwrap(),
                            CString::new(id).unwrap(),
                        ];
                        (cmd, args)
                    }
                };
                execvp(&cmd, &args).ok();

                // If exec fails, exit
                std::process::exit(1);
            }
        }
    }

    /// Get the master file descriptor (for polling)
    #[allow(dead_code)]
    pub fn master_fd(&self) -> RawFd {
        self.master_fd.as_raw_fd()
    }

    /// Read data from PTY (non-blocking)
    pub fn read(&self, buf: &mut [u8]) -> Result<usize, PtyError> {
        match read(self.master_fd.as_raw_fd(), buf) {
            Ok(n) => Ok(n),
            Err(nix::errno::Errno::EAGAIN) => Ok(0),
            Err(e) => Err(PtyError::Read(e)),
        }
    }

    /// Write data to PTY
    pub fn write(&self, data: &[u8]) -> Result<usize, PtyError> {
        write(&self.master_fd, data).map_err(PtyError::Write)
    }

    /// Resize the PTY window
    pub fn resize(&self, rows: u16, cols: u16) -> Result<(), PtyError> {
        let winsize = Winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };

        unsafe {
            if libc::ioctl(self.master_fd.as_raw_fd(), libc::TIOCSWINSZ, &winsize) < 0 {
                return Err(PtyError::Resize(nix::errno::Errno::last()));
            }
        }
        Ok(())
    }

    /// Check if the process is still running
    pub fn is_running(&self) -> bool {
        matches!(
            waitpid(self.child_pid, Some(WaitPidFlag::WNOHANG)),
            Ok(WaitStatus::StillAlive)
        )
    }

    /// Kill the process
    pub fn kill(&self) -> Result<(), PtyError> {
        kill(self.child_pid, Signal::SIGTERM).map_err(PtyError::Kill)?;
        // Give it a moment, then force kill if needed
        std::thread::sleep(std::time::Duration::from_millis(100));
        if self.is_running() {
            kill(self.child_pid, Signal::SIGKILL).map_err(PtyError::Kill)?;
        }
        Ok(())
    }

    /// Get the child PID
    #[allow(dead_code)]
    pub fn pid(&self) -> Pid {
        self.child_pid
    }
}

impl Drop for PtyProcess {
    fn drop(&mut self) {
        self.kill().ok();
    }
}
