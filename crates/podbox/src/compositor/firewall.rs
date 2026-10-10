//! Wayland firewall state and message filtering for the compositor proxy.
//!
//! Extracted verbatim from `compositor.rs`.

use std::collections::HashSet;
use std::sync::Mutex;
use std::time::Instant;

use super::tracker::Tracker;

pub(crate) struct FirewallState {
    blocked_interfaces: HashSet<String>,
    /// Connection-global Wayland object tracker for fd attribution.
    /// Shared by both bridge directions: object ids are unique per
    /// connection regardless of which side minted them.
    tracker: Tracker,
}

impl FirewallState {
    pub(crate) fn new(blocked_interfaces: Vec<String>) -> Self {
        Self {
            blocked_interfaces: blocked_interfaces.into_iter().collect(),
            tracker: Tracker::default(),
        }
    }
}

/// Learn ids minted by a client→host request (registries, binds, factories).
pub(crate) fn track_client_request(
    state: &Mutex<FirewallState>,
    obj: u32,
    opcode: u16,
    msg: &[u8],
) {
    state
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .tracker
        .observe_client_request(obj, opcode, msg);
}

/// Learn ids minted by a host→client event (offers), forget `delete_id`s.
pub(crate) fn track_server_event(state: &Mutex<FirewallState>, obj: u32, opcode: u16, msg: &[u8]) {
    state
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .tracker
        .observe_server_event(obj, opcode, msg);
}

/// How many descriptors does this message carry? Only recognized bearers
/// take from the fd queue; every other message takes nothing.
pub(crate) fn fd_arity(
    state: &Mutex<FirewallState>,
    obj: u32,
    opcode: u16,
    client_to_host: bool,
) -> u8 {
    let guard = state.lock().unwrap_or_else(|e| e.into_inner());
    guard.tracker.fd_arity(obj, opcode, client_to_host)
}

pub(crate) fn rate_allow(bucket: &mut f64, last_refill: &mut Instant) -> bool {
    const RATE: f64 = 10_000.0;
    let now = Instant::now();
    let elapsed = now.duration_since(*last_refill).as_secs_f64();
    *bucket = (*bucket + elapsed * RATE).min(RATE);
    *last_refill = now;
    if *bucket >= 1.0 {
        *bucket -= 1.0;
        true
    } else {
        false
    }
}

/// Check whether a host→client message is a `wl_registry::global` event
/// announcing a blocked interface.
pub(crate) fn is_blocked_global(
    message_bytes: &[u8],
    opcode: u16,
    state: &Mutex<FirewallState>,
) -> bool {
    // wl_registry::global (opcode 0) format:
    //   8 bytes header (object_id, size+opcode=0)
    //   4 bytes name (u32)
    //   4 bytes interface string length (u32, includes NUL)
    //   N bytes interface string (padded to 4 bytes)
    //   4 bytes version (u32)
    if opcode != 0 || message_bytes.len() < 16 {
        return false;
    }

    let str_len = u32::from_ne_bytes(message_bytes[12..16].try_into().unwrap()) as usize;

    // Guard against integer overflow on 32-bit platforms
    if message_bytes
        .len()
        .checked_sub(16)
        .is_none_or(|rem| rem < str_len)
    {
        return false;
    }

    if str_len < 2 {
        return false;
    }

    // Exclude the null terminator at the end.
    let interface_bytes = &message_bytes[16..16 + str_len - 1];
    let interface_name = match std::str::from_utf8(interface_bytes) {
        Ok(s) => s,
        Err(_) => return false,
    };

    let guard = state.lock().unwrap_or_else(|e| e.into_inner());
    guard.blocked_interfaces.contains(interface_name)
}
