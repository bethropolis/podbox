//! `.container` Quadlet section emitters.

pub(crate) use super::container_core::wayland_socket_path;
pub(super) use super::container_core::{
    emit_auto_update, emit_container_image, emit_network, emit_podman_args,
};
pub(super) use super::container_environment::emit_env;
pub(super) use super::container_service::emit_service_section;
pub(super) use super::container_unit::{emit_install_section, emit_quadlet, emit_unit};
pub(super) use super::container_volumes::emit_volumes;
