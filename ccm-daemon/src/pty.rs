//! PTY process management

use anyhow::{anyhow, Context, Result};
use nix::fcntl::{fcntl, FcntlArg, OFlag};
use nix::libc;
use nix::pty::{openpty, Winsize};
use nix::sys::signal::{kill, Signal};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::{close, dup2, execvp, fork, read, setsid, write, ForkResult, Pid};
use std::ffi::CString;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::path::Path;

/// PTY process handle
pub struct PtyProcess {
    /// Master file descriptor
    master_fd: OwnedFd,
    /// Child process ID
    child_pid: Pid,
}

impl PtyProcess {
    /// Spawn a new PTY process running `claude` in the given working directory
    pub fn spawn(working_dir: &Path) -> Result<Self> {
        let winsize = Winsize {
            ws_row: 24,
            ws_col: 80,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };

        // Open PTY
        let pty = openpty(&winsize, None).context("Failed to open PTY")?;

        // Fork
        match unsafe { fork() }.context("Failed to fork")? {
            ForkResult::Parent { child } => {
                // Parent: close slave, keep master
                close(pty.slave.as_raw_fd()).ok();

                // Set master to non-blocking
                let master_fd = pty.master;
                fcntl(master_fd.as_raw_fd(), FcntlArg::F_SETFL(OFlag::O_NONBLOCK)).ok();

                Ok(Self {
                    master_fd,
                    child_pid: child,
                })
            }
            ForkResult::Child => {
                // Child: setup PTY and exec claude
                close(pty.master.as_raw_fd()).ok();

                // Create new session
                setsid().ok();

                // Set controlling terminal
                unsafe {
                    libc::ioctl(pty.slave.as_raw_fd(), libc::TIOCSCTTY, 0);
                }

                // Redirect stdio to slave
                dup2(pty.slave.as_raw_fd(), libc::STDIN_FILENO).ok();
                dup2(pty.slave.as_raw_fd(), libc::STDOUT_FILENO).ok();
                dup2(pty.slave.as_raw_fd(), libc::STDERR_FILENO).ok();

                if pty.slave.as_raw_fd() > libc::STDERR_FILENO {
                    close(pty.slave.as_raw_fd()).ok();
                }

                // Change to working directory
                std::env::set_current_dir(working_dir).ok();

                // Exec claude
                let cmd = CString::new("claude").unwrap();
                let args: Vec<CString> = vec![cmd.clone()];
                execvp(&cmd, &args).ok();

                // If exec fails, exit
                std::process::exit(1);
            }
        }
    }

    /// Get the master file descriptor (for polling)
    pub fn master_fd(&self) -> RawFd {
        self.master_fd.as_raw_fd()
    }

    /// Read data from PTY (non-blocking)
    pub fn read(&self, buf: &mut [u8]) -> Result<usize> {
        match read(self.master_fd.as_raw_fd(), buf) {
            Ok(n) => Ok(n),
            Err(nix::errno::Errno::EAGAIN) | Err(nix::errno::Errno::EWOULDBLOCK) => Ok(0),
            Err(e) => Err(anyhow!("PTY read error: {}", e)),
        }
    }

    /// Write data to PTY
    pub fn write(&self, data: &[u8]) -> Result<usize> {
        write(&self.master_fd, data)
            .map_err(|e| anyhow!("PTY write error: {}", e))
    }

    /// Resize the PTY window
    pub fn resize(&self, rows: u16, cols: u16) -> Result<()> {
        let winsize = Winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };

        unsafe {
            if libc::ioctl(self.master_fd.as_raw_fd(), libc::TIOCSWINSZ, &winsize) < 0 {
                return Err(anyhow!("Failed to resize PTY"));
            }
        }
        Ok(())
    }

    /// Check if the process is still running
    pub fn is_running(&self) -> bool {
        match waitpid(self.child_pid, Some(WaitPidFlag::WNOHANG)) {
            Ok(WaitStatus::StillAlive) => true,
            _ => false,
        }
    }

    /// Kill the process
    pub fn kill(&self) -> Result<()> {
        kill(self.child_pid, Signal::SIGTERM).ok();
        // Give it a moment, then force kill if needed
        std::thread::sleep(std::time::Duration::from_millis(100));
        if self.is_running() {
            kill(self.child_pid, Signal::SIGKILL).ok();
        }
        Ok(())
    }

    /// Get the child PID
    pub fn pid(&self) -> Pid {
        self.child_pid
    }
}

impl Drop for PtyProcess {
    fn drop(&mut self) {
        self.kill().ok();
    }
}
