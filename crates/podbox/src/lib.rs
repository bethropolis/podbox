//! `podbox` — Podman-native container environment manager.
//!
//! Turns a single TOML definition file into a fully integrated,
//! systemd-managed container environment with selective XDG directory
//! sharing, Wayland/audio passthrough, GPU acceleration, and desktop
//! integration (`.desktop` export, binary shims).

pub const VERSION: &str = env!("PODBOX_VERSION");

// Pure modules: config schema, validation, and Quadlet codegen. These have
// no Linux/host dependencies and compile for wasm32-unknown-unknown
// (see the `cli` feature below) so the website Studio can share them.
pub mod codegen;
pub mod config;
pub mod env;
pub mod error;
pub mod guest;
pub mod profiles;
pub mod protocol;
pub mod xdg;

// Linux / host CLI modules. Gated behind the `cli` feature (enabled by
// default) so `podbox-wasm` can depend on this crate with
// `default-features = false`.
#[cfg(feature = "cli")]
pub mod build;
#[cfg(feature = "cli")]
pub mod cli;
#[cfg(feature = "cli")]
pub mod compositor;
#[cfg(feature = "cli")]
pub mod diff;
#[cfg(feature = "cli")]
pub mod editor;
#[cfg(feature = "cli")]
pub mod export;
#[cfg(feature = "cli")]
pub mod history;
#[cfg(feature = "cli")]
pub mod labels;
#[cfg(feature = "cli")]
pub mod lock;
#[cfg(feature = "cli")]
pub mod podman;
#[cfg(feature = "cli")]
pub mod ports;
#[cfg(feature = "cli")]
pub mod process;
#[cfg(feature = "cli")]
pub mod quadlet_install;
#[cfg(feature = "cli")]
pub mod socket_host;
#[cfg(feature = "cli")]
pub mod systemd;
#[cfg(feature = "cli")]
pub mod ui;
#[cfg(feature = "cli")]
pub mod wizard;
