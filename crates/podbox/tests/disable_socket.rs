mod sandbox;

use sandbox::Sandbox;

/// `podbox disable` must disable the systemd units, not just delete their
/// files. Deleting without `systemctl disable` orphans the enabled symlinks,
/// leaving the socket unit enabled across daemon-reloads.
#[test]
fn disable_disables_socket_unit() {
    let sb = Sandbox::new("disable");
    sb.write_config("mybox", "mybox");

    // Simulate an enabled box: unit files on disk.
    let udir = sb.root.join("config/systemd/user");
    std::fs::create_dir_all(&udir).unwrap();
    std::fs::write(udir.join("mybox.socket"), "[Socket]\n").unwrap();
    std::fs::write(udir.join("mybox-host.service"), "[Service]\n").unwrap();

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
        std::fs::set_permissions(
            sb.root.join("bin/systemctl"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }

    let out = sb.run(&["disable", "mybox", "--force"]);
    assert!(
        out.status.success(),
        "disable failed: {}",
        sandbox::text(&out)
    );

    let syscalls = std::fs::read_to_string(sb.root.join("systemctl.log")).unwrap_or_default();
    let disable_line = syscalls.lines().find(|l| l.contains("disable"));
    assert!(
        disable_line.is_some_and(|l| l.contains("mybox.socket")),
        "disable must run `systemctl disable` on the socket unit, systemctl saw: {syscalls:?}"
    );

    assert!(
        !udir.join("mybox.socket").exists(),
        "socket unit file must still be removed"
    );
    assert!(
        !udir.join("mybox-host.service").exists(),
        "host service file must still be removed"
    );
}
