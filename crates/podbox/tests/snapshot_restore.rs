mod sandbox;

use sandbox::Sandbox;

/// `podbox restore` must recreate the container from the snapshot image.
///
/// Retagging `:latest` and running `podman start` restarts the *old*
/// container (podman keeps the filesystem the container was created with),
/// so a restore that never removes the container silently keeps the
/// unrestored state. The fixed flow is stop → rm → tag → unit restart.
#[test]
fn restore_recreates_container_from_snapshot() {
    let sb = Sandbox::new("restore");
    sb.write_config("mybox", "mybox");

    // Simulate an enabled box: restore recreates via the Quadlet unit.
    let qdir = sb.root.join("config/containers/systemd");
    std::fs::create_dir_all(&qdir).unwrap();
    std::fs::write(qdir.join("mybox.container"), "[Container]\n").unwrap();

    // Stub podman: every call succeeds and is logged; the snapshot image
    // "exists".
    std::fs::write(
        sb.root.join("bin/podman"),
        format!(
            "#!/bin/sh\necho \"$@\" >> '{}'\nexit 0\n",
            sb.root.join("podman.log").display()
        ),
    )
    .unwrap();

    // Stub systemctl so no real user manager is touched; log the calls.
    std::fs::write(
        sb.root.join("bin/systemctl"),
        format!(
            "#!/bin/sh\necho \"$@\" >> '{}'\nexit 0\n",
            sb.root.join("systemctl.log").display()
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for bin in ["podman", "systemctl"] {
            std::fs::set_permissions(
                sb.root.join(format!("bin/{bin}")),
                std::fs::Permissions::from_mode(0o755),
            )
            .unwrap();
        }
    }

    let out = sb.run(&["restore", "good", "mybox"]);
    assert!(
        out.status.success(),
        "restore failed: {}",
        sandbox::text(&out)
    );

    let podman_calls = sb.podman_calls();
    let rm_pos = podman_calls
        .iter()
        .position(|l| l.split_whitespace().take(2).collect::<Vec<_>>() == ["rm", "-f"]);
    let tag_pos = podman_calls.iter().position(|l| l.starts_with("tag "));
    assert!(
        rm_pos.is_some(),
        "restore must remove the container so it is recreated, got: {podman_calls:?}"
    );
    assert!(
        tag_pos.is_some(),
        "restore must tag the snapshot as :latest, got: {podman_calls:?}"
    );
    assert!(
        rm_pos < tag_pos,
        "remove must precede the retag, got: {podman_calls:?}"
    );
    assert!(
        !podman_calls.iter().any(|l| l == "start mybox"),
        "restore must not just `podman start` the stale container, got: {podman_calls:?}"
    );

    let syscalls = std::fs::read_to_string(sb.root.join("systemctl.log")).unwrap_or_default();
    assert!(
        syscalls.contains("restart mybox.service"),
        "restore must recreate via the Quadlet unit, systemctl saw: {syscalls:?}"
    );
}

/// Restore on a box that was never enabled has no unit to recreate the
/// container from; it must fail before destroying anything.
#[test]
fn restore_refuses_without_quadlet_unit() {
    let sb = Sandbox::new("restore-no-unit");
    sb.write_config("mybox", "mybox");

    let out = sb.run(&["restore", "good", "mybox"]);
    assert!(
        !out.status.success(),
        "restore without a Quadlet unit should fail, got: {}",
        sandbox::text(&out)
    );
    assert!(
        sb.mutating_podman_calls().is_empty(),
        "failed restore must not mutate podman state, got: {:?}",
        sb.podman_calls()
    );
}
