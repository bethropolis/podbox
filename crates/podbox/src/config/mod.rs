//! Definition-file schema for podbox.
//!
//! Slim re-export hub; the [`Config`] type itself lives in the [`schema`]
//! module.

pub mod defaults;
pub mod enums;
pub mod extends;
pub mod fs;
pub mod merge;
pub mod schema;
pub mod types;
pub mod validation;

pub use defaults::EMBEDDED_DEFAULT;
pub use enums::{CapPreset, GpuMode, ImageSource, OnStop, PackageManager, XdgDirValue};
pub use fs::{
    active_context_path, clear_active_context, config_dir, expand_tilde, find_config_path,
    find_definition, find_legacy_root_configs, list_configs, profiles_dir, read_active_context,
    write_active_context,
};
pub use schema::{Config, SchemaVersion};
pub use types::{
    ContainerConfig, ContainerEnvConfig, CustomCacheConfig, DbusConfig, DotfilesCloneOn,
    DotfilesConfig, ExportConfig, HardwareConfig, HostCacheConfig, HostCachesConfig,
    HostExecConfig, HostExecEntry, ImageConfig, IntegrationConfig, LifecycleConfig, MountConfig,
    NetworkConfig, PackageConfig, RunConfig, SHARED_CACHE_NAMES, SecretEntry, SecretSource,
    SecretType, SecurityConfig, ServiceConfig, ServiceDetailConfig, SharedCachesConfig,
    StorageConfig, SystemdConfig, WaylandConfig, XdgDirConfig,
};
