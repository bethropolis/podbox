//! Shared CLI sandbox for integration tests that must not touch the real host.
//!
//! Each sandbox owns an isolated XDG tree and puts a stub `podman` first on
//! PATH, so commands can run without podman or the developer's real config.

use std::fs::Permissions;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

use assert_cmd::prelude::*;

/// An isolated config home, XDG tree, and stub podman executable.
pub struct Sandbox {
    /// Root directory containing the sandbox's config, state, and binaries.
    pub dir: PathBuf,
}

impl Sandbox {
    /// Create a fresh temp dir with a stub podman and an isolated XDG config home.
    pub fn new(configs: &[&str]) -> Self {
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("podbox-test-{}-{n}", std::process::id()));
        let cfg = dir.join("podbox");
        std::fs::create_dir_all(&cfg).unwrap();
        for name in configs {
            // Copy a valid fixture so commands that fully parse the config
            // (not just stat it) succeed.
            let fixture =
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/full.toml");
            std::fs::copy(&fixture, cfg.join(format!("{name}.toml"))).unwrap();
        }

        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let stub = bin.join("podman");
        let mut f = std::fs::File::create(&stub).unwrap();
        writeln!(f, "#!/bin/sh\nexit 0").unwrap();
        drop(f);
        std::fs::set_permissions(&stub, Permissions::from_mode(0o755)).unwrap();

        Self { dir }
    }

    /// `dirs::config_dir()` appends "podbox" to $XDG_CONFIG_HOME, so the
    /// env var must point at the sandbox root, not at the inner podbox dir.
    fn path_env(&self) -> String {
        format!(
            "{}:{}",
            self.dir.join("bin").display(),
            std::env::var("PATH").unwrap_or_default()
        )
    }

    /// Build a `podbox` command pointed entirely at this sandbox.
    pub fn cmd(&self) -> Command {
        let mut c = Command::cargo_bin("podbox").unwrap();
        c.env("XDG_CONFIG_HOME", &self.dir)
            .env("XDG_DATA_HOME", self.dir.join("data"))
            .env("XDG_STATE_HOME", self.dir.join("state"))
            .env("XDG_RUNTIME_DIR", self.dir.join("runtime"))
            .env_remove("PODBOX_CONTAINER")
            .env("PATH", self.path_env());
        c
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
