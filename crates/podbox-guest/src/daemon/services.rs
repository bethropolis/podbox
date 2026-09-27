use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum GuestServiceConfig {
    Short(String),
    Detailed {
        command: String,
        #[serde(default)]
        env: BTreeMap<String, String>,
        #[serde(default = "default_restart")]
        restart: String,
    },
}

fn default_restart() -> String {
    "on-failure".into()
}

struct SupervisedService {
    name: String,
    command: String,
    env: BTreeMap<String, String>,
    restart: String,
    child: Option<Child>,
    restart_at: std::time::Instant,
    backoff: std::time::Duration,
}

pub(super) fn start_service_supervisor() {
    let Ok(payload) = std::env::var("PODBOX_SERVICES_JSON") else {
        return;
    };
    let Ok(configs) = serde_json::from_str::<BTreeMap<String, GuestServiceConfig>>(&payload) else {
        tracing::error!("guest: invalid PODBOX_SERVICES_JSON payload");
        return;
    };
    if configs.is_empty() {
        return;
    }
    let uid = std::env::var("HOST_UID")
        .ok()
        .and_then(|v| v.parse::<u32>().ok());
    let gid = std::env::var("HOST_GID")
        .ok()
        .and_then(|v| v.parse::<u32>().ok());
    let mut services: Vec<SupervisedService> = configs
        .into_iter()
        .map(|(name, config)| {
            let (command, env, restart) = match config {
                GuestServiceConfig::Short(command) => {
                    (command, BTreeMap::new(), "on-failure".into())
                }
                GuestServiceConfig::Detailed {
                    command,
                    env,
                    restart,
                } => (command, env, restart),
            };
            SupervisedService {
                name,
                command,
                env,
                restart,
                child: None,
                restart_at: std::time::Instant::now(),
                backoff: std::time::Duration::from_millis(100),
            }
        })
        .collect();
    let _ = std::fs::create_dir_all("/run/podbox/services");
    let (ready_sender, ready_receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut reported_ready = false;
        loop {
            for service in &mut services {
                if let Some(child) = &mut service.child {
                    match child.try_wait() {
                        Ok(None) => continue,
                        Ok(Some(status)) => {
                            service.child = None;
                            if service.restart == "always"
                                || (service.restart == "on-failure" && !status.success())
                            {
                                service.restart_at = std::time::Instant::now() + service.backoff;
                                service.backoff =
                                    (service.backoff * 2).min(std::time::Duration::from_secs(10));
                            } else {
                                service.restart = "never".into();
                            }
                        }
                        Err(error) => {
                            tracing::warn!(
                                "guest: unable to poll service {}: {error}",
                                service.name
                            );
                            service.child = None;
                            service.restart_at = std::time::Instant::now() + service.backoff;
                        }
                    }
                }
                if service.child.is_none()
                    && service.restart != "never"
                    && std::time::Instant::now() >= service.restart_at
                {
                    let log_path = format!("/run/podbox/services/{}.log", service.name);
                    let Ok(log) = OpenOptions::new().create(true).append(true).open(log_path)
                    else {
                        continue;
                    };
                    let Ok(log_err) = log.try_clone() else {
                        continue;
                    };
                    let mut command = Command::new("/bin/sh");
                    command
                        .arg("-lc")
                        .arg(&service.command)
                        .env(
                            "HOME",
                            "/home/".to_string()
                                + &std::env::var("HOST_USER").unwrap_or_else(|_| "user".into()),
                        )
                        .envs(&service.env)
                        .stdout(Stdio::from(log))
                        .stderr(Stdio::from(log_err));
                    command.process_group(0);
                    if let Some(uid) = uid {
                        command.uid(uid);
                    }
                    if let Some(gid) = gid {
                        command.gid(gid);
                    }
                    match command.spawn() {
                        Ok(child) => {
                            service.child = Some(child);
                        }
                        Err(error) => {
                            tracing::warn!(
                                "guest: failed to start service {}: {error}",
                                service.name
                            );
                            service.restart_at = std::time::Instant::now() + service.backoff;
                            service.backoff =
                                (service.backoff * 2).min(std::time::Duration::from_secs(10));
                        }
                    }
                }
            }
            let pgids = services
                .iter()
                .filter_map(|service| service.child.as_ref().map(Child::id))
                .map(|pid| pid.to_string())
                .collect::<Vec<_>>()
                .join("\n");
            let _ = std::fs::write("/run/podbox/services/pids", pgids);
            if !reported_ready {
                let _ = ready_sender.send(());
                reported_ready = true;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    });
    // Do not handshake with the host until the supervisor has attempted each
    // initial spawn and published service process groups for idle scans.
    if ready_receiver
        .recv_timeout(std::time::Duration::from_secs(10))
        .is_err()
    {
        tracing::error!("guest: service supervisor failed to initialize");
    }
}

pub(super) fn is_service_process(pid: i32) -> bool {
    let Ok(groups) = std::fs::read_to_string("/run/podbox/services/pids") else {
        return false;
    };
    let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
        return false;
    };
    let Some((_, tail)) = stat.rsplit_once(')') else {
        return false;
    };
    let Some(pgrp) = tail
        .split_whitespace()
        .nth(2)
        .and_then(|v| v.parse::<i32>().ok())
    else {
        return false;
    };
    groups
        .lines()
        .any(|group| group.parse::<i32>().ok() == Some(pgrp))
}
