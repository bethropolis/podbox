mod sandbox;

use sandbox::Sandbox;

#[cfg(unix)]
fn make_executable(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// The container PATH (`/run/podbox/path`, written by the guest daemon on
/// startup) must be read *after* the box is started. Reading it before
/// `ensure_running` misses it on every cold start and the session silently
/// falls back to the image default.
#[test]
fn run_picks_up_container_path_after_start() {
    let sb = Sandbox::new("enter-path");
    sb.write_config("mybox", "mybox");
    let state = sb.root.join("started");

    // Stateful podman stub: the box is stopped until something starts it,
    // and `/run/podbox/path` is only readable once started.
    std::fs::write(
        sb.root.join("bin/podman"),
        format!(
            "#!/bin/sh\nLOG='{}'\nSTATE='{}'\necho \"$@\" >> \"$LOG\"\ncase \"$1\" in\n  inspect)\n    if [ -f \"$STATE\" ]; then echo \"running deadbeef1234\"; exit 0; else echo \"no such container\" >&2; exit 1; fi;;\n  ps) if [ -f \"$STATE\" ]; then echo deadbeef1234; fi; exit 0;;\n  image) exit 0;;\n  exec)\n    if [ -f \"$STATE\" ]; then echo \"/custom/bin:/usr/bin\"; exit 0; else exit 1; fi;;\n  *) exit 0;;\nesac\n",
            sb.root.join("podman.log").display(),
            state.display(),
        ),
    )
    .unwrap();
    #[cfg(unix)]
    make_executable(&sb.root.join("bin/podman"));

    // Stub systemctl: starting the unit marks the box started.
    std::fs::write(
        sb.root.join("bin/systemctl"),
        format!(
            "#!/bin/sh\necho \"$@\" >> '{}'\ncase \"$2\" in\n  start|restart) touch '{}'; exit 0;;\n  is-active) exit 1;;\n  *) exit 0;;\nesac\n",
            sb.root.join("systemctl.log").display(),
            state.display(),
        ),
    )
    .unwrap();
    #[cfg(unix)]
    make_executable(&sb.root.join("bin/systemctl"));

    let out = sb.run(&["run", "mybox", "myapp"]);
    assert!(out.status.success(), "run failed: {}", sandbox::text(&out));

    let calls = sb.podman_calls();
    let last_exec = calls
        .iter()
        .rev()
        .find(|l| l.starts_with("exec "))
        .cloned()
        .unwrap_or_default();
    assert!(
        last_exec.contains("--env=PATH=/custom/bin:/usr/bin"),
        "final exec must carry the container PATH read after start, got: {last_exec:?}\nall calls: {calls:?}"
    );
}

/// A dry run must not shell out to read the container PATH: the box may be
/// stopped, and dry-run output must stay deterministic.
#[test]
fn run_dry_run_does_not_probe_container_path() {
    let sb = Sandbox::new("enter-path-dry");
    sb.write_config("mybox", "mybox");

    let out = sb.run(&["--dry-run", "run", "mybox", "myapp"]);
    assert!(
        out.status.success(),
        "dry run failed: {}",
        sandbox::text(&out)
    );
    let probe_calls: Vec<String> = sb
        .podman_calls()
        .into_iter()
        .filter(|l| l.starts_with("exec "))
        .collect();
    assert!(
        probe_calls.is_empty(),
        "dry run must not exec into the container, got: {probe_calls:?}"
    );
}
