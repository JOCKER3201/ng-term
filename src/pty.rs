//! PTY: running the user's shell on a pseudoterminal (libc).

use std::ffi::CString;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::io::RawFd;
use std::path::Path;
use std::sync::mpsc::{channel, Receiver, Sender};

pub enum PtyEvent {
    Data(Vec<u8>),
    /// The shell has exited.
    Exited,
}

pub struct Pty {
    pub master: RawFd,
    pub child: libc::pid_t,
}

impl Pty {
    pub fn spawn(
        cols: u16,
        rows: u16,
        cwd: Option<&Path>,
    ) -> io::Result<(Pty, Receiver<PtyEvent>)> {
        // TERM like in eDEX-UI (xterm.js) — full colors.
        std::env::set_var("TERM", "xterm-256color");
        std::env::set_var("COLORTERM", "truecolor");

        let ws = libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let mut master: RawFd = -1;
        let mut slave: RawFd = -1;
        let ret = unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null(),
                &ws,
            )
        };
        if ret != 0 {
            return Err(io::Error::last_os_error());
        }

        // The master must not leak into forked children (and their execs):
        // without CLOEXEC every new shell inherits the PTY fds of the
        // already-open sessions.
        unsafe {
            let flags = libc::fcntl(master, libc::F_GETFD);
            if flags >= 0 {
                libc::fcntl(master, libc::F_SETFD, flags | libc::FD_CLOEXEC);
            }
        }

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into());
        let cwd_c = cwd.and_then(|p| CString::new(p.as_os_str().as_bytes()).ok());

        // For bash we inject the ng-term startup file (source ~/.bashrc +
        // opening files with the associated app when a name is typed).
        let shellrc = crate::config::shellrc_path();
        let use_rc = shell.rsplit('/').next() == Some("bash") && shellrc.is_file();
        let rc_c = CString::new(shellrc.as_os_str().as_bytes()).unwrap_or_default();
        let init_flag = CString::new("--init-file").unwrap();

        // Everything the child needs is built HERE, before fork(): the
        // child of a multithreaded process may only call async-signal-safe
        // functions between fork and exec — no allocation (CString, Vec).
        let prog = CString::new(shell.as_str()).unwrap_or_default();
        let argv0 = prog.clone();
        let argv: Vec<*const libc::c_char> = if use_rc {
            vec![argv0.as_ptr(), init_flag.as_ptr(), rc_c.as_ptr(), std::ptr::null()]
        } else {
            vec![argv0.as_ptr(), std::ptr::null()]
        };

        let pid = unsafe { libc::fork() };
        if pid < 0 {
            let err = io::Error::last_os_error();
            // fork failed: neither fd is owned by a Pty yet — close both.
            unsafe {
                libc::close(master);
                libc::close(slave);
            }
            return Err(err);
        }
        if pid == 0 {
            // Child process: attach the slave as the controlling terminal
            // and exec the shell. Only async-signal-safe calls below — the
            // CStrings/argv were allocated before the fork.
            unsafe {
                libc::close(master);
                libc::setsid();
                libc::ioctl(slave, libc::TIOCSCTTY, 0);
                libc::dup2(slave, 0);
                libc::dup2(slave, 1);
                libc::dup2(slave, 2);
                if slave > 2 {
                    libc::close(slave);
                }
                // Shell start directory (home directory by default).
                if let Some(ref d) = cwd_c {
                    libc::chdir(d.as_ptr());
                }
                libc::execvp(prog.as_ptr(), argv.as_ptr());
                // If execvp failed:
                libc::_exit(127);
            }
        }
        unsafe {
            libc::close(slave);
        }

        let (tx, rx): (Sender<PtyEvent>, Receiver<PtyEvent>) = channel();
        let fd = master;
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                let n = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
                if n <= 0 {
                    let _ = tx.send(PtyEvent::Exited);
                    break;
                }
                if tx.send(PtyEvent::Data(buf[..n as usize].to_vec())).is_err() {
                    break;
                }
            }
        });

        Ok((Pty { master, child: pid }, rx))
    }

    pub fn write(&self, data: &[u8]) {
        let mut off = 0;
        while off < data.len() {
            let n = unsafe {
                libc::write(
                    self.master,
                    data[off..].as_ptr() as *const libc::c_void,
                    data.len() - off,
                )
            };
            if n <= 0 {
                break;
            }
            off += n as usize;
        }
    }

    pub fn resize(&self, cols: u16, rows: u16) {
        let ws = libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        unsafe {
            libc::ioctl(self.master, libc::TIOCSWINSZ, &ws);
        }
    }

    /// Current working directory of the shell process (from /proc).
    pub fn child_cwd(&self) -> Option<std::path::PathBuf> {
        std::fs::read_link(format!("/proc/{}/cwd", self.child)).ok()
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        unsafe {
            libc::kill(self.child, libc::SIGHUP);
            libc::close(self.master);
            // Reap the child so it does not linger as a zombie. Closing
            // the master gives the shell EOF and SIGHUP terminates it, so
            // this returns promptly (a zombie is reaped immediately).
            let mut status = 0;
            libc::waitpid(self.child, &mut status, 0);
        }
    }
}
