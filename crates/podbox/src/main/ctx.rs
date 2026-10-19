//! Dispatch-level context carried into command handlers.

use podbox::cli::Cli;

/// Global CLI state a command handler needs beyond its own arguments.
///
/// Currently only the dry-run flag: created so the dispatch layer has one
/// struct to thread instead of loose parameters, and so a `--dry-run`
/// command can never be plumbed through by accident — handlers take `&Ctx`
/// rather than re-derive this from the raw CLI.
#[derive(Clone, Copy)]
pub(crate) struct Ctx {
    pub dry_run: bool,
}

impl Ctx {
    pub(crate) fn from_cli(cli: &Cli) -> Self {
        Self {
            dry_run: cli.dry_run,
        }
    }
}
