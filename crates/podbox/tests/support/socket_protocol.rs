//! Shared harness for the socket-protocol integration tests.
//!
//! Every test runs [`podbox::socket_host::serve_connection`] (the same loop
//! `podbox serve` runs per connection) over a `UnixStream::pair()`. Host side
//! effects go to a recording fake, so nothing touches the real desktop.

use std::collections::BTreeMap;
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use podbox::config::{Config, HostExecEntry};
use podbox::protocol::{
    GuestMessage, HostMessage, NotifyAction, PROTOCOL_VERSION, read_frame, write_frame,
};
use podbox::socket_host::{CwdMap, HostActions, serve_connection};

/// Records every host side effect. Clipboard is a single slot, as on a desktop.
#[derive(Default)]
pub struct Recorder {
    pub calls: Mutex<Vec<String>>,
    pub clipboard: Mutex<String>,
    /// Action key returned from `notify` when actions are offered.
    pub chosen_key: String,
}

impl Recorder {
    // Some independent test binaries only verify protocol frames, not recorded calls.
    #[allow(dead_code)]
    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

impl HostActions for Recorder {
    fn notify(
        &self,
        summary: &str,
        _body: &str,
        urgency: &str,
        app_name: &str,
        actions: &[NotifyAction],
    ) -> anyhow::Result<String> {
        self.calls.lock().unwrap().push(format!(
            "notify:{summary}:{urgency}:{app_name}:{}",
            actions.len()
        ));
        Ok(self.chosen_key.clone())
    }

    fn open_uri(&self, uri: &str) -> anyhow::Result<()> {
        self.calls.lock().unwrap().push(format!("open:{uri}"));
        Ok(())
    }

    fn clipboard_set(&self, text: &str) -> anyhow::Result<()> {
        self.calls.lock().unwrap().push("clipboard_set".into());
        *self.clipboard.lock().unwrap() = text.to_string();
        Ok(())
    }

    fn clipboard_get(&self) -> anyhow::Result<String> {
        self.calls.lock().unwrap().push("clipboard_get".into());
        Ok(self.clipboard.lock().unwrap().clone())
    }

    fn stop_unit(&self, name: &str) -> anyhow::Result<()> {
        self.calls.lock().unwrap().push(format!("stop:{name}"));
        Ok(())
    }
}

/// A config with every integration switched on and a two-entry host-exec
/// allowlist. Absolute paths, so `canonicalize` works without PATH lookup.
pub fn enabled_config() -> Config {
    let mut cfg = Config::embedded();
    cfg.integration.notify = true;
    cfg.integration.xdg_open.enabled = true;
    cfg.integration.clipboard = true;
    cfg.integration.host_exec.enabled = true;
    cfg.integration.host_exec.allowlist = Some(BTreeMap::from([
        (
            "echo".to_string(),
            HostExecEntry::Simple("/bin/echo".into()),
        ),
        ("sh".to_string(), HostExecEntry::Simple("/bin/sh".into())),
    ]));
    cfg
}

/// Run the real loop on one end of a socket pair. Returns the guest end and
/// the server thread, whose result shows how the connection ended.
// The cwd test binary uses its local `start_cwd` helper to pin a fixture map.
#[allow(dead_code)]
pub fn start(cfg: Config, rec: Arc<Recorder>) -> (UnixStream, JoinHandle<anyhow::Result<()>>) {
    let (client, mut server) = UnixStream::pair().unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let cwd_map = CwdMap::for_daemon(&cfg);
    let handle =
        std::thread::spawn(move || serve_connection(&mut server, &cfg, "dev", &*rec, &cwd_map));
    (client, handle)
}

/// Close the guest end and wait for the server loop to return.
pub fn finish(client: UnixStream, handle: JoinHandle<anyhow::Result<()>>) -> anyhow::Result<()> {
    drop(client);
    handle.join().expect("server thread panicked")
}

pub fn send(c: &mut UnixStream, msg: &GuestMessage) {
    write_frame(c, msg).unwrap();
}

pub fn recv(c: &mut UnixStream) -> HostMessage {
    let bytes = read_frame(c)
        .unwrap()
        .expect("server closed the connection");
    serde_json::from_slice(&bytes).unwrap()
}

pub fn hello(caps: &[&str]) -> GuestMessage {
    GuestMessage::Hello {
        protocol_version: PROTOCOL_VERSION,
        guest_version: "test".into(),
        container: "dev".into(),
        capabilities: caps.iter().map(|c| (*c).to_string()).collect(),
    }
}
