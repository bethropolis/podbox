mod sandbox;

use sandbox::Sandbox;

#[cfg(unix)]
fn make_executable(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// A failed `podbox enable` on the Podman 6 path must put the previous
/// units back: the upgrade deletes the old flat units before invoking
/// `podman quadlet install --application`, so without a rollback a failure
/// leaves the box with no working units at all.
#[test]
fn failed_install_restores_previous_units() {
    let sb = Sandbox::new("rollback");
    sb.write_config("mybox", "mybox");

    // Previous state: flat-layout units from an older install.
    let qdir = sb.root.join("config/containers/systemd");
    let udir = sb.root.join("config/systemd/user");
    std::fs::create_dir_all(&qdir).unwrap();
    std::fs::create_dir_all(&udir).unwrap();
    std::fs::write(qdir.join("mybox.container"), "[Container]\n# old-flat\n").unwrap();
    std::fs::write(udir.join("mybox.socket"), "[Socket]\n# old-socket\n").unwrap();

    // Podman 6 stub whose `quadlet install` fails.
    std::fs::write(
        sb.root.join("bin/podman"),
        format!(
            "#!/bin/sh\necho \"$@\" >> '{}'\nif [ \"$1\" = \"version\" ]; then echo \"6.0.0\"; exit 0; fi\nif [ \"$1\" = \"quadlet\" ]; then echo \"boom\" >&2; exit 1; fi\nexit 0\n",
            sb.root.join("podman.log").display()
        ),
    )
    .unwrap();
    #[cfg(unix)]
    make_executable(&sb.root.join("bin/podman"));

    // Stub systemctl so no real user manager is touched.
    std::fs::write(sb.root.join("bin/systemctl"), "#!/bin/sh\nexit 0\n").unwrap();
    #[cfg(unix)]
    make_executable(&sb.root.join("bin/systemctl"));

    // Keep the container home inside the sandbox.
    let home = sb.root.join("home").to_string_lossy().into_owned();
    let out = sb.run_env(&["enable", "mybox"], &[("HOME", home.as_str())]);
    assert!(
        !out.status.success(),
        "enable with a failing podman should fail, got: {}",
        sandbox::text(&out)
    );

    assert_eq!(
        std::fs::read_to_string(qdir.join("mybox.container")).unwrap(),
        "[Container]\n# old-flat\n",
        "previous flat unit must be restored after a failed install"
    );
    assert_eq!(
        std::fs::read_to_string(udir.join("mybox.socket")).unwrap(),
        "[Socket]\n# old-socket\n",
        "previous socket unit must be restored after a failed install"
    );
    assert!(
        !qdir.join("mybox").exists() || !qdir.join("mybox/mybox.container").exists(),
        "half-written application layout must not be left behind"
    );
    let err = sandbox::text(&out);
    assert!(
        err.contains("restored"),
        "failure must say the previous units were restored, got: {err}"
    );
}
