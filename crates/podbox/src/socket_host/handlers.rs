//! Per-message protocol handlers for the host socket server.

mod desktop;
mod host_exec;

#[cfg(test)]
pub(crate) use desktop::{MAX_URI_LEN, UriError, validate_uri};
pub(super) use desktop::{
    handle_clipboard_get, handle_clipboard_set, handle_notify, handle_xdg_open,
};
pub(super) use host_exec::{HostExecV2Request, handle_host_exec, handle_host_exec_v2};
#[cfg(test)]
pub(super) use host_exec::{exit_code_and_signal, stream_child_output, validate_host_exec_args};
