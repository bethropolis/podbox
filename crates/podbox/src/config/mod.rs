//! Definition-file schema for podbox.
//!
//! Slim re-export hub; the [`Config`] type itself lives in the [`schema`]
//! module.

pub mod schema;
pub mod defaults;
pub mod enums;
pub mod fs;
pub mod types;
pub mod validation;

pub use schema::{Config, SchemaVersion};
pub use defaults::EMBEDDED_DEFAULT;
pub use enums::{CapPreset, GpuMode, ImageSource, OnStop, PackageManager, XdgDirValue};
pub use fs::{
    active_context_path, clear_active_context, config_dir, expand_tilde, find_definition,
    list_configs, read_active_context, write_active_context,
};
pub use types::{
    ContainerConfig, DbusConfig, ExportConfig, HostExecConfig, HostExecEntry, ImageConfig,
    IntegrationConfig, LifecycleConfig, MountConfig, NetworkConfig, PackageConfig, RunConfig,
    SecurityConfig, SystemdConfig, WaylandConfig, XdgDirConfig,
};
