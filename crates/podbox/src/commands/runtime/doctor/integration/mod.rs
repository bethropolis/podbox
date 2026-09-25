//! Integration-group checks for `podbox doctor`: the host↔container
//! bridges (Wayland, D-Bus, clipboard, host-exec, hardware, secrets).
//!
//! Slim dispatcher module; `see super` for the check surface. Wayland
//! socket checks live in [`wayland`], host tool presence in [`tools`],
//! `host-exec` allowlists in [`host_exec`], hardware presets in
//! [`hardware`], secrets in [`secrets`], and stale-artifact cleanup in
//! [`cleanup`].

mod cleanup;
mod hardware;
mod host_exec;
mod secrets;
mod tools;
mod wayland;

pub(crate) use cleanup::{check_dead_exports, check_stale_sockets};
pub(crate) use hardware::check_hardware;
pub(crate) use host_exec::check_host_exec;
pub(crate) use secrets::check_secrets;
pub(crate) use tools::{check_toolchain, check_xdg_user_dir};
pub(crate) use wayland::check_wayland;
