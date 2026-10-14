//! Wayland firewall state and message filtering for the compositor proxy.
//!
//! Extracted verbatim from `compositor.rs`.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::Instant;

use super::tracker::Tracker;

pub(crate) struct FirewallState {
    blocked_interfaces: HashSet<String>,
    /// Global names hidden from the client, with their interface. A bind to
    /// one of these is refused, since the client was never told it exists.
    hidden_globals: HashMap<u32, String>,
    /// Connection-global Wayland object tracker for fd attribution.
    /// Shared by both bridge directions: object ids are unique per
    /// connection regardless of which side minted them.
    tracker: Tracker,
}

impl FirewallState {
    pub(crate) fn new(blocked_interfaces: Vec<String>) -> Self {
        Self {
            blocked_interfaces: blocked_interfaces.into_iter().collect(),
            hidden_globals: HashMap::new(),
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

/// Read a Wayland `string` arg at `off` (length word, bytes, NUL, 4-byte pad).
/// Returns the text without its NUL, or `None` when the bytes do not fit.
fn wl_string_at(msg: &[u8], off: usize) -> Option<&str> {
    let len = u32::from_ne_bytes(msg.get(off..off + 4)?.try_into().ok()?) as usize;
    if len < 2 {
        return None;
    }
    let bytes = msg.get(off + 4..off + 4 + len - 1)?;
    std::str::from_utf8(bytes).ok()
}

/// The global `name` argument at offset 8 of a registry message.
fn name_arg(msg: &[u8]) -> Option<u32> {
    Some(u32::from_ne_bytes(msg.get(8..12)?.try_into().ok()?))
}

/// Filter a host→client registry message.
///
/// - `wl_registry.global` (opcode 0) for a blocked interface is hidden, and its
///   name is remembered so a later bind to it can be refused.
/// - `wl_registry.global_remove` (opcode 1) for a hidden name is hidden too.
///
/// Both apply only to a `wl_registry` this connection created. Opcode 0 on any
/// other object is an ordinary event, not a global.
pub(crate) fn is_blocked_global(
    message_bytes: &[u8],
    obj: u32,
    opcode: u16,
    state: &Mutex<FirewallState>,
) -> bool {
    let mut guard = state.lock().unwrap_or_else(|e| e.into_inner());
    if !guard.tracker.is_registry(obj) {
        return false;
    }
    match opcode {
        // wl_registry::global: name (u32), interface (string), version (u32).
        0 => {
            let (Some(name), Some(iface)) =
                (name_arg(message_bytes), wl_string_at(message_bytes, 12))
            else {
                return false;
            };
            if !guard.blocked_interfaces.contains(iface) {
                return false;
            }
            let iface = iface.to_string();
            guard.hidden_globals.insert(name, iface);
            true
        }
        // wl_registry::global_remove: name (u32).
        1 => {
            name_arg(message_bytes).is_some_and(|name| guard.hidden_globals.remove(&name).is_some())
        }
        _ => false,
    }
}

/// Refuse a client→host `wl_registry.bind` to a hidden global, or to a blocked
/// interface under any name. Returns the reason, or `None` when the bind may
/// pass. A refused bind ends the connection; it is never forwarded.
pub(crate) fn forbidden_bind(
    message_bytes: &[u8],
    obj: u32,
    opcode: u16,
    state: &Mutex<FirewallState>,
) -> Option<String> {
    if opcode != 0 {
        return None;
    }
    let guard = state.lock().unwrap_or_else(|e| e.into_inner());
    if !guard.tracker.is_registry(obj) {
        return None;
    }
    let name = name_arg(message_bytes)?;
    if let Some(hidden) = guard.hidden_globals.get(&name) {
        return Some(format!("bind to hidden global {name} ({hidden})"));
    }
    let iface = wl_string_at(message_bytes, 12)?;
    if guard.blocked_interfaces.contains(iface) {
        return Some(format!("bind to blocked interface {iface}"));
    }
    None
}
