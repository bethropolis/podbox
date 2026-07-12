use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use podbox::protocol::{
    GuestMessage, HostMessage, read_frame, write_frame, ALL_CAPABILITIES, PROTOCOL_VERSION,
};

fn temp_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("failed to create temp dir")
}

fn socket_path(dir: &std::path::Path) -> std::path::PathBuf {
    dir.join("test.sock")
}

/// Spawn a server that handles one connection, processes protocol messages,
/// and shuts down when the client disconnects.
fn spawn_server(
    listener: UnixListener,
    stored_clipboard: Arc<Mutex<Option<String>>>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let (mut stream, _) = match listener.accept() {
            Ok(pair) => pair,
            Err(e) => {
                eprintln!("server accept error: {e}");
                return;
            }
        };
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));

        loop {
            let msg_bytes = match read_frame(&mut stream) {
                Ok(Some(b)) => b,
                Ok(None) => return,
                Err(e) => {
                    eprintln!("server read error: {e}");
                    return;
                }
            };
            let msg: GuestMessage = match serde_json::from_slice(&msg_bytes) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("server deserialize error: {e}");
                    return;
                }
            };
            match msg {
                GuestMessage::Hello {
                    protocol_version,
                    guest_version: _,
                    container: _,
                    capabilities,
                } => {
                    if protocol_version != PROTOCOL_VERSION {
                        let _ = write_frame(&mut stream, &HostMessage::Shutdown);
                        return;
                    }
                    let mut accepted = Vec::new();
                    let mut rejected = Vec::new();
                    for cap in capabilities {
                        if ALL_CAPABILITIES.contains(&cap.as_str()) {
                            accepted.push(cap);
                        } else {
                            rejected.push(cap);
                        }
                    }
                    let _ = write_frame(
                        &mut stream,
                        &HostMessage::HelloAck {
                            accepted,
                            rejected,
                            idle_timeout_secs: 0,
                        },
                    );
                }
                GuestMessage::Notify {
                    summary: _,
                    body: _,
                    urgency: _,
                    actions,
                    app_name: _,
                } => {
                    let action_key = if actions.is_empty() {
                        String::new()
                    } else {
                        // Simulate the first action being clicked.
                        actions.first().map(|a| a.key.clone()).unwrap_or_default()
                    };
                    let _ = write_frame(
                        &mut stream,
                        &HostMessage::NotifyActionResult {
                            notification_id: 0,
                            action_key,
                        },
                    );
                }
                GuestMessage::XdgOpen { uri: _ } => {
                    // No response defined in protocol.
                }
                GuestMessage::ClipboardSet { text } => {
                    let mut guard = stored_clipboard.lock().unwrap();
                    *guard = Some(text);
                }
                GuestMessage::ClipboardGet => {
                    let text = stored_clipboard
                        .lock()
                        .unwrap()
                        .clone()
                        .unwrap_or_default();
                    let _ = write_frame(&mut stream, &HostMessage::ClipboardData { text });
                }
                GuestMessage::HostExec { cmd, args } => {
                    // Simulate host-exec by running the command locally.
                    let output = std::process::Command::new(&cmd)
                        .args(&args)
                        .output();
                    match output {
                        Ok(out) => {
                            if !out.stdout.is_empty() {
                                let _ = write_frame(
                                    &mut stream,
                                    &HostMessage::HostExecStdout {
                                        data: String::from_utf8_lossy(&out.stdout).to_string(),
                                    },
                                );
                            }
                            if !out.stderr.is_empty() {
                                let _ = write_frame(
                                    &mut stream,
                                    &HostMessage::HostExecStderr {
                                        data: String::from_utf8_lossy(&out.stderr).to_string(),
                                    },
                                );
                            }
                            let _ = write_frame(
                                &mut stream,
                                &HostMessage::HostExecDone {
                                    exit_code: out.status.code().unwrap_or(-1),
                                },
                            );
                        }
                        Err(e) => {
                            let _ = write_frame(
                                &mut stream,
                                &HostMessage::HostExecStderr {
                                    data: format!("command failed: {e}"),
                                },
                            );
                            let _ = write_frame(
                                &mut stream,
                                &HostMessage::HostExecDone { exit_code: -1 },
                            );
                        }
                    }
                }
                GuestMessage::RegisterSession => {}
                GuestMessage::Busy => {}
                GuestMessage::IdleTimeout => {}
            }
        }
    })
}

fn connect_client(socket_path: &std::path::Path) -> UnixStream {
    // Retry a few times in case the server hasn't called accept() yet.
    for i in 0..10 {
        match UnixStream::connect(socket_path) {
            Ok(s) => {
                let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
                return s;
            }
            Err(_) if i < 9 => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => panic!("failed to connect after {i} retries: {e}"),
        }
    }
    unreachable!()
}

#[test]
fn hello_handshake() {
    let dir = temp_dir();
    let path = socket_path(&dir.path());
    let listener = UnixListener::bind(&path).unwrap();
    let clipboard = Arc::new(Mutex::new(None));
    let _server = spawn_server(listener, clipboard);

    let mut client = connect_client(&path);
    write_frame(
        &mut client,
        &GuestMessage::Hello {
            protocol_version: PROTOCOL_VERSION,
            guest_version: "0.5.2-test".into(),
            container: "test-container".into(),
            capabilities: vec!["notify".into(), "xdg_open".into()],
        },
    )
    .unwrap();

    let bytes = read_frame(&mut client).unwrap().unwrap();
    let response: HostMessage = serde_json::from_slice(&bytes).unwrap();
    match response {
        HostMessage::HelloAck {
            accepted,
            rejected,
            idle_timeout_secs,
        } => {
            assert_eq!(accepted, vec!["notify", "xdg_open"]);
            assert!(rejected.is_empty());
            assert_eq!(idle_timeout_secs, 0);
        }
        _ => panic!("expected HelloAck, got {response:?}"),
    }
}

#[test]
fn protocol_version_mismatch_triggers_shutdown() {
    let dir = temp_dir();
    let path = socket_path(&dir.path());
    let listener = UnixListener::bind(&path).unwrap();
    let clipboard = Arc::new(Mutex::new(None));
    let _server = spawn_server(listener, clipboard);

    let mut client = connect_client(&path);
    write_frame(
        &mut client,
        &GuestMessage::Hello {
            protocol_version: 999,
            guest_version: "0.5.2-test".into(),
            container: "test".into(),
            capabilities: vec![],
        },
    )
    .unwrap();

    let bytes = read_frame(&mut client).unwrap().unwrap();
    let response: HostMessage = serde_json::from_slice(&bytes).unwrap();
    match response {
        HostMessage::Shutdown => {}
        _ => panic!("expected Shutdown, got {response:?}"),
    }
}

#[test]
fn capability_negotiation_rejects_unknown() {
    let dir = temp_dir();
    let path = socket_path(&dir.path());
    let listener = UnixListener::bind(&path).unwrap();
    let clipboard = Arc::new(Mutex::new(None));
    let _server = spawn_server(listener, clipboard);

    let mut client = connect_client(&path);
    write_frame(
        &mut client,
        &GuestMessage::Hello {
            protocol_version: PROTOCOL_VERSION,
            guest_version: "0.5.2-test".into(),
            container: "test".into(),
            capabilities: vec!["unknown_cap_1".into(), "unknown_cap_2".into()],
        },
    )
    .unwrap();

    let bytes = read_frame(&mut client).unwrap().unwrap();
    let response: HostMessage = serde_json::from_slice(&bytes).unwrap();
    match response {
        HostMessage::HelloAck {
            accepted,
            rejected,
            idle_timeout_secs: _,
        } => {
            assert!(accepted.is_empty());
            assert_eq!(rejected.len(), 2);
            assert!(rejected.contains(&"unknown_cap_1".to_string()));
            assert!(rejected.contains(&"unknown_cap_2".to_string()));
        }
        _ => panic!("expected HelloAck, got {response:?}"),
    }
}

#[test]
fn notify_without_actions_immediately_acknowledged() {
    let dir = temp_dir();
    let path = socket_path(&dir.path());
    let listener = UnixListener::bind(&path).unwrap();
    let clipboard = Arc::new(Mutex::new(None));
    let _server = spawn_server(listener, clipboard);

    let mut client = connect_client(&path);
    // Send hello first
    write_frame(
        &mut client,
        &GuestMessage::Hello {
            protocol_version: PROTOCOL_VERSION,
            guest_version: "test".into(),
            container: "test".into(),
            capabilities: vec![],
        },
    )
    .unwrap();
    let _ = read_frame(&mut client);

    // Send notify without actions
    write_frame(
        &mut client,
        &GuestMessage::Notify {
            summary: "Test Summary".into(),
            body: "Test Body".into(),
            urgency: "normal".into(),
            actions: vec![],
            app_name: "test".into(),
        },
    )
    .unwrap();

    let bytes = read_frame(&mut client).unwrap().unwrap();
    let response: HostMessage = serde_json::from_slice(&bytes).unwrap();
    match response {
        HostMessage::NotifyActionResult {
            notification_id: 0,
            action_key,
        } => {
            assert!(action_key.is_empty());
        }
        _ => panic!("expected NotifyActionResult, got {response:?}"),
    }
}

#[test]
fn notify_with_actions_returns_selected_action() {
    let dir = temp_dir();
    let path = socket_path(&dir.path());
    let listener = UnixListener::bind(&path).unwrap();
    let clipboard = Arc::new(Mutex::new(None));
    let _server = spawn_server(listener, clipboard);

    let mut client = connect_client(&path);
    write_frame(
        &mut client,
        &GuestMessage::Hello {
            protocol_version: PROTOCOL_VERSION,
            guest_version: "test".into(),
            container: "test".into(),
            capabilities: vec![],
        },
    )
    .unwrap();
    let _ = read_frame(&mut client);

    write_frame(
        &mut client,
        &GuestMessage::Notify {
            summary: "Test".into(),
            body: "Test".into(),
            urgency: "normal".into(),
            actions: vec![
                podbox::protocol::NotifyAction {
                    key: "ok".into(),
                    label: "OK".into(),
                },
                podbox::protocol::NotifyAction {
                    key: "cancel".into(),
                    label: "Cancel".into(),
                },
            ],
            app_name: "test".into(),
        },
    )
    .unwrap();

    let bytes = read_frame(&mut client).unwrap().unwrap();
    let response: HostMessage = serde_json::from_slice(&bytes).unwrap();
    match response {
        HostMessage::NotifyActionResult {
            notification_id: 0,
            action_key,
        } => {
            // Server simulates clicking the first action.
            assert_eq!(action_key, "ok");
        }
        _ => panic!("expected NotifyActionResult, got {response:?}"),
    }
}

#[test]
fn clipboard_roundtrip() {
    let dir = temp_dir();
    let path = socket_path(&dir.path());
    let listener = UnixListener::bind(&path).unwrap();
    let clipboard = Arc::new(Mutex::new(None));
    let _server = spawn_server(listener, Arc::clone(&clipboard));

    let mut client = connect_client(&path);
    write_frame(
        &mut client,
        &GuestMessage::Hello {
            protocol_version: PROTOCOL_VERSION,
            guest_version: "test".into(),
            container: "test".into(),
            capabilities: vec![],
        },
    )
    .unwrap();
    let _ = read_frame(&mut client);

    write_frame(
        &mut client,
        &GuestMessage::ClipboardSet {
            text: "hello from guest".into(),
        },
    )
    .unwrap();

    write_frame(&mut client, &GuestMessage::ClipboardGet).unwrap();

    let bytes = read_frame(&mut client).unwrap().unwrap();
    let response: HostMessage = serde_json::from_slice(&bytes).unwrap();
    match response {
        HostMessage::ClipboardData { text } => {
            assert_eq!(text, "hello from guest");
        }
        _ => panic!("expected ClipboardData, got {response:?}"),
    }
}

#[test]
fn clipboard_get_returns_empty_when_not_set() {
    let dir = temp_dir();
    let path = socket_path(&dir.path());
    let listener = UnixListener::bind(&path).unwrap();
    let clipboard = Arc::new(Mutex::new(None));
    let _server = spawn_server(listener, clipboard);

    let mut client = connect_client(&path);
    write_frame(
        &mut client,
        &GuestMessage::Hello {
            protocol_version: PROTOCOL_VERSION,
            guest_version: "test".into(),
            container: "test".into(),
            capabilities: vec![],
        },
    )
    .unwrap();
    let _ = read_frame(&mut client);

    write_frame(&mut client, &GuestMessage::ClipboardGet).unwrap();

    let bytes = read_frame(&mut client).unwrap().unwrap();
    let response: HostMessage = serde_json::from_slice(&bytes).unwrap();
    match response {
        HostMessage::ClipboardData { text } => {
            assert_eq!(text, "");
        }
        _ => panic!("expected ClipboardData, got {response:?}"),
    }
}

#[test]
fn host_exec_echo_roundtrip() {
    let dir = temp_dir();
    let path = socket_path(&dir.path());
    let listener = UnixListener::bind(&path).unwrap();
    let clipboard = Arc::new(Mutex::new(None));
    let _server = spawn_server(listener, clipboard);

    let mut client = connect_client(&path);
    write_frame(
        &mut client,
        &GuestMessage::Hello {
            protocol_version: PROTOCOL_VERSION,
            guest_version: "test".into(),
            container: "test".into(),
            capabilities: vec![],
        },
    )
    .unwrap();
    let _ = read_frame(&mut client);

    write_frame(
        &mut client,
        &GuestMessage::HostExec {
            cmd: "echo".into(),
            args: vec!["hello from test".into()],
        },
    )
    .unwrap();

    let bytes = read_frame(&mut client).unwrap().unwrap();
    let response: HostMessage = serde_json::from_slice(&bytes).unwrap();
    match response {
        HostMessage::HostExecStdout { data } => {
            assert!(data.contains("hello from test"), "got stdout: {data:?}");
        }
        other => panic!("expected HostExecStdout, got {other:?}"),
    }

    let bytes = read_frame(&mut client).unwrap().unwrap();
    let response: HostMessage = serde_json::from_slice(&bytes).unwrap();
    match response {
        HostMessage::HostExecDone { exit_code } => {
            assert_eq!(exit_code, 0);
        }
        other => panic!("expected HostExecDone, got {other:?}"),
    }
}

#[test]
fn host_exec_failing_command_reports_stderr_and_error_code() {
    let dir = temp_dir();
    let path = socket_path(&dir.path());
    let listener = UnixListener::bind(&path).unwrap();
    let clipboard = Arc::new(Mutex::new(None));
    let _server = spawn_server(listener, clipboard);

    let mut client = connect_client(&path);
    write_frame(
        &mut client,
        &GuestMessage::Hello {
            protocol_version: PROTOCOL_VERSION,
            guest_version: "test".into(),
            container: "test".into(),
            capabilities: vec![],
        },
    )
    .unwrap();
    let _ = read_frame(&mut client);

    write_frame(
        &mut client,
        &GuestMessage::HostExec {
            cmd: "sh".into(),
            args: vec!["-c".into(), "exit 42".into()],
        },
    )
    .unwrap();

    let bytes = read_frame(&mut client).unwrap().unwrap();
    let response: HostMessage = serde_json::from_slice(&bytes).unwrap();
    match response {
        HostMessage::HostExecDone { exit_code } => {
            assert_eq!(exit_code, 42);
        }
        other => panic!("expected HostExecDone, got {other:?}"),
    }
}

#[test]
fn host_exec_nonexistent_command_reports_error() {
    let dir = temp_dir();
    let path = socket_path(&dir.path());
    let listener = UnixListener::bind(&path).unwrap();
    let clipboard = Arc::new(Mutex::new(None));
    let _server = spawn_server(listener, clipboard);

    let mut client = connect_client(&path);
    write_frame(
        &mut client,
        &GuestMessage::Hello {
            protocol_version: PROTOCOL_VERSION,
            guest_version: "test".into(),
            container: "test".into(),
            capabilities: vec![],
        },
    )
    .unwrap();
    let _ = read_frame(&mut client);

    write_frame(
        &mut client,
        &GuestMessage::HostExec {
            cmd: "nonexistent-command-12345".into(),
            args: vec![],
        },
    )
    .unwrap();

    let bytes = read_frame(&mut client).unwrap().unwrap();
    let response: HostMessage = serde_json::from_slice(&bytes).unwrap();
    match response {
        HostMessage::HostExecStderr { data } => {
            assert!(
                data.contains("failed") || data.contains("No such file"),
                "got stderr: {data:?}"
            );
        }
        other => panic!("expected HostExecStderr, got {other:?}"),
    }
}
