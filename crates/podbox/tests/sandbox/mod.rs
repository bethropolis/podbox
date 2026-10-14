//! Shared harness for CLI tests that must not touch the real host.
//!
//! Each `Sandbox` owns an isolated XDG tree and a stub `podman` on PATH. The
//! stub appends its arguments to a log and exits 0, so a test can check which
//! podman calls a command made, and that none of them mutated anything.

#![allow(dead_code)] // each test binary uses a different subset

use std::path::PathBuf;
use std::process::{Command, Output};

use assert_cmd::prelude::*;

pub struct Sandbox {
    pub root: PathBuf,
    bin: PathBuf,
    log: PathBuf,
}

impl Sandbox {
    pub fn new(tag: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("podbox-sandbox-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let bin = root.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(root.join("config/podbox/profiles")).unwrap();
        let log = root.join("podman.log");
        let podman = bin.join("podman");
        std::fs::write(
            &podman,
            format!("#!/bin/sh\necho \"$@\" >> '{}'\nexit 0\n", log.display()),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&podman, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        Self { root, bin, log }
    }

    pub fn profiles(&self) -> PathBuf {
        self.root.join("config/podbox/profiles")
    }

    /// Write a valid config whose runtime name is `runtime`, as `<file>.toml`.
    pub fn write_config(&self, file: &str, runtime: &str) -> PathBuf {
        let mut cfg = podbox::config::Config::embedded();
        cfg.container.name = runtime.to_string();
        cfg.image.name = runtime.to_string();
        let path = self.profiles().join(format!("{file}.toml"));
        std::fs::write(&path, toml::to_string_pretty(&cfg).unwrap()).unwrap();
        path
    }

    /// Podman invocations made so far, one line per call.
    pub fn podman_calls(&self) -> Vec<String> {
        std::fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// Podman verbs that change state. A dry run must make none of these.
    pub fn mutating_podman_calls(&self) -> Vec<String> {
        const MUTATING: &[&str] = &[
            "commit", "tag", "start", "stop", "restart", "rm", "rmi", "build", "exec", "run",
            "pull", "kill", "create", "volume", "quadlet",
        ];
        self.podman_calls()
            .into_iter()
            .filter(|line| {
                let verb = line.split_whitespace().next().unwrap_or("");
                MUTATING.contains(&verb)
            })
            .collect()
    }

    pub fn run(&self, args: &[&str]) -> Output {
        self.run_env(args, &[])
    }

    pub fn run_env(&self, args: &[&str], extra_env: &[(&str, &str)]) -> Output {
        let path = format!(
            "{}:{}",
            self.bin.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let mut cmd = Command::cargo_bin("podbox").unwrap();
        cmd.env("PATH", path)
            .env("XDG_CONFIG_HOME", self.root.join("config"))
            .env("XDG_DATA_HOME", self.root.join("data"))
            .env("XDG_RUNTIME_DIR", self.root.join("run"))
            .env_remove("PODBOX_CONTAINER")
            .args(args);
        for (k, v) in extra_env {
            cmd.env(k, v);
        }
        cmd.output().unwrap()
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

pub fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}
