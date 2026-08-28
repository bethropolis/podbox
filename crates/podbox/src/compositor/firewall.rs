//! Wayland firewall state and message filtering for the compositor proxy.
//!
//! Extracted verbatim from `compositor.rs`.

use std::collections::HashSet;
use std::sync::Mutex;
use std::time::Instant;

pub(crate) struct FirewallState {
    blocked_interfaces: HashSet<String>,
}

impl FirewallState {
    pub(crate) fn new(blocked_interfaces: Vec<String>) -> Self {
        Self {
            blocked_interfaces: blocked_interfaces.into_iter().collect(),
        }
    }
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
