//! Definition-file schema for podbox.
//!
//! Slim re-export hub; the [`Config`] type itself lives in the [`schema`]
//! module.

pub mod defaults;
pub mod emit;
pub mod enums;
pub mod extends;
pub mod fs;
pub mod merge;
pub mod schema;
pub mod schema_defs;
pub mod types;
pub mod validation;

pub use defaults::EMBEDDED_DEFAULT;
pub use emit::toml_string;
pub use enums::{
    CapPreset, GpuMode, ImageSource, NetworkMode, OnStop, PackageManager, ServiceRestart,
    UsernsMode, XdgDirValue,
};
#[cfg(feature = "cli")]
pub use fs::select_config_interactive;
pub use fs::{
    active_context_path, clear_active_context, config_dir, default_home, expand_tilde,
    find_by_runtime_name, find_config_path, find_definition, find_legacy_root_configs,
    list_configs, missing_config_error, parse_package_list, profiles_dir, read_active_context,
    write_active_context,
};
pub use schema::{Config, IMAGE_REPO_PREFIX, SchemaVersion};
pub use schema_defs::{schema_defaults, schema_enums};
pub use types::{
    BuiltinCachePath, BuiltinCaches, ContainerConfig, ContainerEnvConfig, CustomCacheConfig,
    DbusConfig, DbusPreset, DotfilesCloneOn, DotfilesConfig, ExportConfig, HardwareConfig,
    HostCacheConfig, HostCachesConfig, HostExecConfig, HostExecEntry, ImageConfig,
    IntegrationConfig, LifecycleConfig, MountConfig, NetworkConfig, PackageConfig,
    ProvisionOnFailure, ProvisionStep, RunConfig, SecretEntry, SecretSource, SecretType,
    SecurityConfig, ServiceConfig, ServiceDetailConfig, SharedCachesConfig, StorageConfig,
    SystemdConfig, WaylandConfig, XdgDirConfig, XdgOpenConfig, all_builtin_cache_paths,
    builtin_cache_mount_names, builtin_cache_names,
};
pub use validation::{is_valid_env_key, validate_container_name};
