use anyhow::Result;

use super::super::ctx::Ctx;
use super::super::handlers;
use super::super::resolve::{self, ConfigResolveError};
use podbox::cli::Cli;
use podbox::ui;

/// `--edit` on build, start and enter: open the config in the editor, or only
/// say so under `--dry-run` (an editor session is not a dry-run action).
///
/// Edits the file the command actually runs against: the explicit `--config`
/// path when given, otherwise the config for the positional name (which wins
/// over `-C`, mirroring resolution) or `-C` itself.
pub(super) fn edit_config_for_command(ctx: &Ctx, cli: &Cli) -> Result<()> {
    let config_path = match &cli.config {
        Some(path) => path.clone(),
        None => {
            let name =
                handlers::extract_positional_name(&cli.command).or_else(|| cli.container.clone());
            resolve::resolve_config_path(name.as_deref())
                .map_err(ConfigResolveError::into_report)?
        }
    };
    if ctx.dry_run {
        ui::would(&format!("open '{}' in $EDITOR", config_path.display()));
        return Ok(());
    }
    let ed = podbox::editor::resolve()?;
    podbox::editor::open(&ed, &config_path)
}
