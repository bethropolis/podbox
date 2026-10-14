//! Wayland object tracker for `SCM_RIGHTS` fd attribution.
//!
//! The bridge must forward each file descriptor with the exact message that
//! sent it. On a stream socket one `recvmsg` routinely returns several
//! coalesced messages plus their descriptors with no byte-level association,
//! so the bridge tracks *which* `(object, opcode)` pairs carry descriptors
//! and hands each read's descriptors to those messages in completion order.
//!
//! Without this, every `wl_shm.create_pool` sent in a flush with trailing
//! requests (the universal `create_pool` + `create_buffer` pair) arrives at
//! the host compositor without its descriptor, which kills the client with
//! `invalid arguments for wl_shm.create_pool`. The same fate awaits
//! clipboard pastes, keymaps, GPU buffers, and dmabuf format tables.
//!
//! The bearer table below is generated from the protocol XMLs by
//! `scripts/audit-wayland-fds.py` (`--check` enforces sync): every request
//! or event whose args include `type="fd"`, with opcodes verified against
//! `wayland.xml` + `wayland-protocols`. Request/event opcodes are
//! append-only, so they hold across interface versions. Most bearers carry
//! exactly one descriptor; `create_listener` carries two.
//!
//! Object ids are learned two ways: client binds (the interface name rides
//! in the bind message) and factory requests/events whose `new_id` arg mints
//! a trackable child. `new_id` carries no type, so factories stay
//! hand-written — but there are few of them and they rarely change, while
//! fd-bearing messages grow with every staging protocol.
//!
//! Scope: unrecognized traffic keeps the legacy behavior (leftover
//! descriptors flush with the last message of the read). Objects are never
//! un-tracked except via `wl_display.delete_id`; tracked objects are in
//! practice never destroyed, so id-reuse aliasing is accepted (documented).

use std::collections::{HashMap, HashSet};

/// (interface, opcode, client_to_host, fd_count): every fd-bearing message
/// known to the protocol XMLs. Regenerate via scripts/audit-wayland-fds.py.
const FD_BEARERS: &[(&str, u16, bool, u8)] = &[
    // Requests.
    ("wl_shm", 0, true, 1),                               // create_pool
    ("wl_data_offer", 1, true, 1),                        // receive
    ("zwp_primary_selection_offer_v1", 0, true, 1),       // receive
    ("zwp_linux_buffer_params_v1", 1, true, 1),           // add
    ("ext_data_control_offer_v1", 0, true, 1),            // receive
    ("wp_image_description_creator_icc_v1", 1, true, 1),  // set_icc_file
    ("wp_security_context_manager_v1", 1, true, 2),       // create_listener
    ("wp_linux_drm_syncobj_manager_v1", 2, true, 1),      // import_timeline
    ("zwp_linux_surface_synchronization_v1", 1, true, 1), // set_acquire_fence
    // Events.
    ("wl_keyboard", 0, false, 1),                     // keymap
    ("zwp_linux_dmabuf_feedback_v1", 1, false, 1),    // format_table
    ("ext_data_control_source_v1", 0, false, 1),      // send
    ("wl_data_source", 1, false, 1),                  // send
    ("wp_drm_lease_device_v1", 0, false, 1),          // drm_fd
    ("wp_drm_lease_v1", 0, false, 1),                 // lease_fd
    ("wp_image_description_info_v1", 1, false, 1),    // icc_file
    ("zwp_linux_buffer_release_v1", 0, false, 1),     // fenced_release
    ("zwp_primary_selection_source_v1", 0, false, 1), // send
];

/// The tracked roles an object id can play. Only roles that mint further
/// tracked ids or bear descriptors need variants.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Role {
    Registry,
    Shm,
    Seat,
    Keyboard,
    DataDeviceManager,
    DataDevice,
    DataOffer,
    DataSource,
    PrimaryManager,
    PrimaryDevice,
    PrimaryOffer,
    PrimarySource,
    ExtManager,
    ExtDevice,
    ExtSource,
    ExtOffer,
    Dmabuf,
    LinuxParams,
    DmabufFeedback,
    SyncobjManager,
    ExplicitSync,
    SurfaceSync,
    BufferRelease,
    LeaseDevice,
    LeaseRequest,
    Lease,
    ColorManager,
    IccCreator,
    Info,
    SecurityManager,
}

impl Role {
    /// Map a bound interface name to its tracked role, if any.
    fn from_interface(name: &str) -> Option<Role> {
        match name {
            "wl_shm" => Some(Role::Shm),
            "wl_seat" => Some(Role::Seat),
            "wl_data_device_manager" => Some(Role::DataDeviceManager),
            "zwp_primary_selection_device_manager_v1" => Some(Role::PrimaryManager),
            "ext_data_control_manager_v1" => Some(Role::ExtManager),
            "zwp_linux_dmabuf_v1" => Some(Role::Dmabuf),
            "wp_linux_drm_syncobj_manager_v1" => Some(Role::SyncobjManager),
            "zwp_linux_explicit_synchronization_v1" => Some(Role::ExplicitSync),
            "wp_drm_lease_device_v1" => Some(Role::LeaseDevice),
            "wp_color_manager_v1" => Some(Role::ColorManager),
            "wp_security_context_manager_v1" => Some(Role::SecurityManager),
            _ => None,
        }
    }

    /// Canonical interface name, for bearer-table lookup.
    fn interface(self) -> &'static str {
        match self {
            Role::Registry => "wl_registry",
            Role::Shm => "wl_shm",
            Role::Seat => "wl_seat",
            Role::Keyboard => "wl_keyboard",
            Role::DataDeviceManager => "wl_data_device_manager",
            Role::DataDevice => "wl_data_device",
            Role::DataOffer => "wl_data_offer",
            Role::DataSource => "wl_data_source",
            Role::PrimaryManager => "zwp_primary_selection_device_manager_v1",
            Role::PrimaryDevice => "zwp_primary_selection_device_v1",
            Role::PrimaryOffer => "zwp_primary_selection_offer_v1",
            Role::PrimarySource => "zwp_primary_selection_source_v1",
            Role::ExtManager => "ext_data_control_manager_v1",
            Role::ExtDevice => "ext_data_control_device_v1",
            Role::ExtSource => "ext_data_control_source_v1",
            Role::ExtOffer => "ext_data_control_offer_v1",
            Role::Dmabuf => "zwp_linux_dmabuf_v1",
            Role::LinuxParams => "zwp_linux_buffer_params_v1",
            Role::DmabufFeedback => "zwp_linux_dmabuf_feedback_v1",
            Role::SyncobjManager => "wp_linux_drm_syncobj_manager_v1",
            Role::ExplicitSync => "zwp_linux_explicit_synchronization_v1",
            Role::SurfaceSync => "zwp_linux_surface_synchronization_v1",
            Role::BufferRelease => "zwp_linux_buffer_release_v1",
            Role::LeaseDevice => "wp_drm_lease_device_v1",
            Role::LeaseRequest => "wp_drm_lease_request_v1",
            Role::Lease => "wp_drm_lease_v1",
            Role::ColorManager => "wp_color_manager_v1",
            Role::IccCreator => "wp_image_description_creator_icc_v1",
            Role::Info => "wp_image_description_info_v1",
            Role::SecurityManager => "wp_security_context_manager_v1",
        }
    }
}

#[derive(Default)]
pub(crate) struct Tracker {
    /// Object ids known to be `wl_registry`s (from `wl_display.get_registry`).
    registries: HashSet<u32>,
    /// Object id → tracked role (both client- and server-allocated ids share
    /// one numeric space per connection).
    objects: HashMap<u32, Role>,
}

fn read_u32(msg: &[u8], off: usize) -> Option<u32> {
    msg.get(off..off + 4)
        .map(|b| u32::from_ne_bytes(b.try_into().unwrap()))
}

/// Read a Wayland `string` arg at `off`: length word, bytes, NUL, 4-pad.
/// Returns the string and the offset of the next arg.
fn read_string(msg: &[u8], off: usize) -> Option<(String, usize)> {
    let len = read_u32(msg, off)? as usize;
    if len < 1 || msg.len() < off + 4 + len {
        return None;
    }
    let s = std::str::from_utf8(msg.get(off + 4..off + 4 + len - 1)?).ok()?;
    let next = off + 4 + len.next_multiple_of(4);
    Some((s.to_string(), next))
}

/// The trailing `new_id` arg: last 4 bytes of the message.
fn read_new_id_tail(msg: &[u8]) -> Option<u32> {
    if msg.len() < 12 {
        return None;
    }
    read_u32(msg, msg.len() - 4)
}

impl Tracker {
    /// True when `obj` is a `wl_registry` this connection created.
    pub(crate) fn is_registry(&self, obj: u32) -> bool {
        self.registries.contains(&obj)
    }

    /// Learn ids minted by client→host requests: registries, binds, and the
    /// `get_*`/`create_*` factories whose children are tracked. Factories
    /// take the new id as the first arg.
    pub(crate) fn observe_client_request(&mut self, obj: u32, opcode: u16, msg: &[u8]) {
        // wl_display.get_registry — opcode 1, single new_id arg.
        if obj == 1 && opcode == 1 {
            if let Some(id) = read_u32(msg, 8) {
                self.registries.insert(id);
            }
            return;
        }
        // wl_registry.bind — opcode 0: name, interface string, version, new_id.
        if opcode == 0 && self.registries.contains(&obj) {
            let (iface, _) = match read_string(msg, 12) {
                Some(v) => v,
                None => return,
            };
            let new_id = match read_new_id_tail(msg) {
                Some(v) => v,
                None => return,
            };
            if let Some(role) = Role::from_interface(&iface) {
                self.objects.insert(new_id, role);
                if role == Role::Registry {
                    self.registries.insert(new_id);
                }
            }
            return;
        }
        let child = match (self.objects.get(&obj), opcode) {
            (Some(Role::DataDeviceManager), 0) => Some(Role::DataSource),
            (Some(Role::DataDeviceManager), 1) => Some(Role::DataDevice),
            (Some(Role::PrimaryManager), 0) => Some(Role::PrimarySource),
            (Some(Role::PrimaryManager), 1) => Some(Role::PrimaryDevice),
            (Some(Role::Seat), 1) => Some(Role::Keyboard),
            (Some(Role::Dmabuf), 1) => Some(Role::LinuxParams),
            (Some(Role::Dmabuf), 2) => Some(Role::DmabufFeedback),
            (Some(Role::Dmabuf), 3) => Some(Role::DmabufFeedback),
            (Some(Role::ExtManager), 0) => Some(Role::ExtSource),
            (Some(Role::ExtManager), 1) => Some(Role::ExtDevice),
            (Some(Role::ExplicitSync), 1) => Some(Role::SurfaceSync),
            (Some(Role::SurfaceSync), 2) => Some(Role::BufferRelease),
            (Some(Role::LeaseDevice), 0) => Some(Role::LeaseRequest),
            (Some(Role::LeaseRequest), 1) => Some(Role::Lease),
            (Some(Role::ColorManager), 4) => Some(Role::IccCreator),
            (Some(Role::IccCreator), 0) => Some(Role::Info),
            _ => None,
        };
        if let Some(role) = child {
            if let Some(id) = read_u32(msg, 8) {
                self.objects.insert(id, role);
            }
        }
    }

    /// Learn ids minted by host→client events, and forget `delete_id` ones.
    /// Offer events mint offer ids (sole new_id arg).
    pub(crate) fn observe_server_event(&mut self, obj: u32, opcode: u16, msg: &[u8]) {
        // wl_display.delete_id — opcode 1, single id arg.
        if obj == 1 && opcode == 1 {
            if let Some(id) = read_u32(msg, 8) {
                self.objects.remove(&id);
                self.registries.remove(&id);
            }
            return;
        }
        let child = match (self.objects.get(&obj), opcode) {
            (Some(Role::DataDevice), 0) => Some(Role::DataOffer),
            (Some(Role::PrimaryDevice), 0) => Some(Role::PrimaryOffer),
            (Some(Role::ExtDevice), 0) => Some(Role::ExtOffer),
            _ => None,
        };
        if let Some(role) = child {
            if let Some(id) = read_u32(msg, 8) {
                self.objects.insert(id, role);
            }
        }
    }

    /// How many descriptors does this message carry? Zero for anything
    /// unrecognized (legacy fallback handles those).
    pub(crate) fn fd_arity(&self, obj: u32, opcode: u16, client_to_host: bool) -> u8 {
        let Some(role) = self.objects.get(&obj) else {
            return 0;
        };
        let iface = role.interface();
        FD_BEARERS
            .iter()
            .find(|(name, op, dir, _)| *name == iface && *op == opcode && *dir == client_to_host)
            .map(|(_, _, _, count)| *count)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a wire message: header + raw arg bytes.
    fn message(obj: u32, opcode: u16, args: &[u8]) -> Vec<u8> {
        let size = u32::try_from(8 + args.len()).expect("message size fits u32");
        let mut buf = Vec::with_capacity(size as usize);
        buf.extend_from_slice(&obj.to_ne_bytes());
        buf.extend_from_slice(&((size << 16) | u32::from(opcode)).to_ne_bytes());
        buf.extend_from_slice(args);
        buf
    }

    fn string_arg(s: &str, out: &mut Vec<u8>) {
        let raw = s.as_bytes();
        let len = u32::try_from(raw.len() + 1).expect("string len fits u32");
        out.extend_from_slice(&len.to_ne_bytes());
        out.extend_from_slice(raw);
        out.push(0);
        while !out.len().is_multiple_of(4) {
            out.push(0);
        }
    }

    fn bind_msg(name: u32, iface: &str, new_id: u32) -> Vec<u8> {
        let mut args = Vec::new();
        args.extend_from_slice(&name.to_ne_bytes());
        string_arg(iface, &mut args);
        args.extend_from_slice(&1u32.to_ne_bytes()); // version
        args.extend_from_slice(&new_id.to_ne_bytes());
        message(2, 0, &args)
    }

    fn registry() -> Tracker {
        let mut t = Tracker::default();
        t.observe_client_request(1, 1, &message(1, 1, &2u32.to_ne_bytes()));
        t
    }

    #[test]
    fn tracks_registry_and_shm_bind() {
        let mut t = registry();
        assert!(t.registries.contains(&2));
        // bind(wl_shm) as id 9
        t.observe_client_request(2, 0, &bind_msg(5, "wl_shm", 9));
        assert_eq!(t.objects.get(&9), Some(&Role::Shm));
        assert_eq!(t.fd_arity(9, 0, true), 1); // create_pool
        assert_eq!(t.fd_arity(9, 1, true), 0); // release
    }

    #[test]
    fn ignores_untracked_interfaces() {
        let mut t = registry();
        t.observe_client_request(2, 0, &bind_msg(3, "wl_compositor", 7));
        assert_eq!(t.fd_arity(7, 0, true), 0);
        assert_eq!(t.fd_arity(99, 0, true), 0); // unknown id
    }

    #[test]
    fn tracks_seat_keyboard_for_keymap_events() {
        let mut t = registry();
        t.observe_client_request(2, 0, &bind_msg(1, "wl_seat", 4));
        // seat.get_keyboard(new id 20)
        t.observe_client_request(4, 1, &message(4, 1, &20u32.to_ne_bytes()));
        assert_eq!(t.fd_arity(20, 0, false), 1); // keymap
        assert_eq!(t.fd_arity(20, 3, false), 0); // key
    }

    #[test]
    fn tracks_data_offer_chain() {
        let mut t = registry();
        t.observe_client_request(2, 0, &bind_msg(2, "wl_data_device_manager", 5));
        // get_data_device(new id 6, seat 4)
        let mut args = 6u32.to_ne_bytes().to_vec();
        args.extend_from_slice(&4u32.to_ne_bytes());
        t.observe_client_request(5, 1, &message(5, 1, &args));
        // server: data_offer(new id 30)
        t.observe_server_event(6, 0, &message(6, 0, &30u32.to_ne_bytes()));
        assert_eq!(t.fd_arity(30, 1, true), 1); // receive
        assert_eq!(t.fd_arity(30, 0, true), 0); // accept
    }

    #[test]
    fn tracks_data_source_send_events() {
        let mut t = registry();
        t.observe_client_request(2, 0, &bind_msg(2, "wl_data_device_manager", 5));
        // create_data_source(new id 6)
        t.observe_client_request(5, 0, &message(5, 0, &6u32.to_ne_bytes()));
        assert_eq!(t.fd_arity(6, 1, false), 1); // send
        assert_eq!(t.fd_arity(6, 0, false), 0); // target
    }

    #[test]
    fn tracks_dmabuf_feedback_for_format_table() {
        let mut t = registry();
        t.observe_client_request(2, 0, &bind_msg(9, "zwp_linux_dmabuf_v1", 40));
        // get_default_feedback(new id 41)
        t.observe_client_request(40, 2, &message(40, 2, &41u32.to_ne_bytes()));
        assert_eq!(t.fd_arity(41, 1, false), 1); // format_table
        assert_eq!(t.fd_arity(41, 0, false), 0); // done
        // get_surface_feedback(new id 42, surface 7)
        let mut args = 42u32.to_ne_bytes().to_vec();
        args.extend_from_slice(&7u32.to_ne_bytes());
        t.observe_client_request(40, 3, &message(40, 3, &args));
        assert_eq!(t.fd_arity(42, 1, false), 1);
    }

    #[test]
    fn tracks_explicit_sync_fence_flow() {
        let mut t = registry();
        t.observe_client_request(
            2,
            0,
            &bind_msg(9, "zwp_linux_explicit_synchronization_v1", 40),
        );
        // get_synchronization(new id 41, surface 7)
        let mut args = 41u32.to_ne_bytes().to_vec();
        args.extend_from_slice(&7u32.to_ne_bytes());
        t.observe_client_request(40, 1, &message(40, 1, &args));
        assert_eq!(t.fd_arity(41, 1, true), 1); // set_acquire_fence
        // get_release(new id 42)
        t.observe_client_request(41, 2, &message(41, 2, &42u32.to_ne_bytes()));
        assert_eq!(t.fd_arity(42, 0, false), 1); // fenced_release
    }

    #[test]
    fn tracks_drm_lease_fd_flow() {
        let mut t = registry();
        t.observe_client_request(2, 0, &bind_msg(9, "wp_drm_lease_device_v1", 40));
        assert_eq!(t.fd_arity(40, 0, false), 1); // drm_fd
        // create_lease_request(new id 41)
        t.observe_client_request(40, 0, &message(40, 0, &41u32.to_ne_bytes()));
        // submit(new id 42)
        t.observe_client_request(41, 1, &message(41, 1, &42u32.to_ne_bytes()));
        assert_eq!(t.fd_arity(42, 0, false), 1); // lease_fd
    }

    #[test]
    fn tracks_syncobj_import_on_manager() {
        let mut t = registry();
        t.observe_client_request(2, 0, &bind_msg(9, "wp_linux_drm_syncobj_manager_v1", 40));
        // import_timeline(new id 41, fd) — bearer on the manager itself.
        assert_eq!(t.fd_arity(40, 2, true), 1);
        assert_eq!(t.fd_arity(40, 1, true), 0); // get_surface
    }

    #[test]
    fn tracks_icc_creator_chain() {
        let mut t = registry();
        t.observe_client_request(2, 0, &bind_msg(9, "wp_color_manager_v1", 40));
        // create_icc_creator(new id 41)
        t.observe_client_request(40, 4, &message(40, 4, &41u32.to_ne_bytes()));
        assert_eq!(t.fd_arity(41, 1, true), 1); // set_icc_file
        // create(new id 42)
        t.observe_client_request(41, 0, &message(41, 0, &42u32.to_ne_bytes()));
        assert_eq!(t.fd_arity(42, 1, false), 1); // icc_file
    }

    #[test]
    fn tracks_security_listener_two_fds() {
        let mut t = registry();
        t.observe_client_request(2, 0, &bind_msg(9, "wp_security_context_manager_v1", 40));
        assert_eq!(t.fd_arity(40, 1, true), 2); // create_listener
    }

    #[test]
    fn tracks_ext_data_control_chain() {
        let mut t = registry();
        t.observe_client_request(2, 0, &bind_msg(9, "ext_data_control_manager_v1", 40));
        // get_data_device(new id 41, seat 4)
        let mut args = 41u32.to_ne_bytes().to_vec();
        args.extend_from_slice(&4u32.to_ne_bytes());
        t.observe_client_request(40, 1, &message(40, 1, &args));
        // server: data_offer(new id 42)
        t.observe_server_event(41, 0, &message(41, 0, &42u32.to_ne_bytes()));
        assert_eq!(t.fd_arity(42, 0, true), 1); // receive
        // create_data_source(new id 43)
        t.observe_client_request(40, 0, &message(40, 0, &43u32.to_ne_bytes()));
        assert_eq!(t.fd_arity(43, 0, false), 1); // send
    }

    #[test]
    fn delete_id_forgets_objects() {
        let mut t = registry();
        t.observe_client_request(2, 0, &bind_msg(5, "wl_shm", 9));
        assert_eq!(t.fd_arity(9, 0, true), 1);
        t.observe_server_event(1, 1, &message(1, 1, &9u32.to_ne_bytes()));
        assert_eq!(t.fd_arity(9, 0, true), 0);
    }

    #[test]
    fn malformed_messages_are_ignored() {
        let mut t = Tracker::default();
        t.observe_client_request(1, 1, &[1, 2, 3]); // too short
        t.observe_client_request(2, 0, &message(2, 0, &[0u8; 4])); // bad string
        assert!(t.registries.is_empty());
        assert!(t.objects.is_empty());
    }
}
