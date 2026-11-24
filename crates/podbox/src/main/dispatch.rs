//! Command classification and resolution dispatch.

mod classification;
mod doctor;
mod edit;
mod no_config;
mod publish;
mod with_config;

pub(crate) use classification::{
    ConfigNeed, command_output_format, needs_config, needs_podman, target_name, validate_cli_names,
};
pub(crate) use no_config::dispatch_no_config;
pub(crate) use with_config::dispatch_with_config;
