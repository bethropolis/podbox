//! Runtime command surface: session argv/orchestration, the git bridge, the
//! stdin watchdog, doctor, status, and logs.
//!
//! `enter`, `exec`, and `run` live in [`session`]; their `podman exec` argv and
//! environment helpers live in [`session::args`].

pub(crate) use context::RunContext;

mod context;
mod doctor;
mod git_bridge;
mod logs;
mod session;
mod status;
mod watchdog;

pub use doctor::run_doctor;
pub use doctor::try_fix_bare_memory_for_target;
pub use logs::run_logs;
pub(super) use session::args_to_string;
pub use session::{run_exec, run_run, run_shell_enter};
pub use status::run_status;
pub use watchdog::run_stdin_watchdog;
