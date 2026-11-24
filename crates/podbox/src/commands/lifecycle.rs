//! Container lifecycle commands: build, enable/disable, start/stop,
//! update, remove. Snapshot/restore live in [`snapshot`].

mod checkpoint;
mod install;
mod remove;
mod snapshot;
mod state;
mod update;

pub use install::{run_build, run_disable, run_enable};
pub use remove::{run_remove, run_remove_stale};
pub use snapshot::{run_restore, run_snapshot, run_snapshot_list, run_snapshot_prune};
pub use state::{run_restart, run_start, run_stop, run_stop_all};
pub use update::{run_rollback, run_update};
