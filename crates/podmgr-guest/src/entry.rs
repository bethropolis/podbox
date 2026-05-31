use std::ffi::CString;
use std::os::fd::AsRawFd;

use nix::libc;
use nix::unistd::{execv, execvp, fork, ForkResult};

/// Fork a daemon process, then exec the user command.
///
/// Parent: execs the user shell/command (replaces this process).
/// Child: redirects stdio to /dev/null, then execs `podmgr-guest --daemon`.
pub fn run(cmd: &[String]) -> ! {
    match unsafe { fork() } {
        Ok(ForkResult::Child) => {
            // Child: become the daemon
            let dev_null_r = std::fs::File::open("/dev/null").unwrap_or_else(|_| unsafe { libc::_exit(1) });
            let dev_null_w = std::fs::OpenOptions::new()
                .write(true)
                .open("/dev/null")
                .unwrap_or_else(|_| unsafe { libc::_exit(1) });

            let _ = nix::unistd::dup2(dev_null_r.as_raw_fd(), 0);
            let _ = nix::unistd::dup2(dev_null_w.as_raw_fd(), 1);
            let _ = nix::unistd::dup2(dev_null_w.as_raw_fd(), 2);

            let program = CString::new("/usr/local/bin/podmgr-guest").unwrap();
            let arg = CString::new("--daemon").unwrap();
            let _ = execv(&program, &[&program, &arg]);
            unsafe { libc::_exit(1) }
        }
        Ok(ForkResult::Parent { .. }) => {
            // Parent: exec the user command
            if cmd.is_empty() {
                let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into());
                let program = CString::new(shell.as_bytes()).unwrap();
                // Login shell convention: arg0 starts with '-'
                let arg0 = CString::new(format!("-{}", shell)).unwrap();
                let _ = execv(&program, &[&arg0]);
            } else {
                let args: Vec<CString> = cmd
                    .iter()
                    .map(|s| CString::new(s.as_bytes()).unwrap_or_else(|_| {
                        eprintln!("podmgr-guest: command argument contains null byte");
                        std::process::exit(1)
                    }))
                    .collect();
                let args_refs: Vec<&CString> = args.iter().collect();
                let _ = execvp(&args_refs[0], &args_refs);
            }
            std::process::exit(1)
        }
        Err(_) => {
            std::process::exit(1)
        }
    }
}
