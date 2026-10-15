mod sandbox;

use sandbox::Sandbox;

#[cfg(unix)]
fn make_executable(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn stub_podman(sb: &Sandbox) {
    std::fs::write(
        sb.root.join("bin/podman"),
        format!(
            "#!/bin/sh\necho \"$@\" >> '{}'\nif [ \"$3\" = \"sh\" ]; then echo \"/home/bob\"; exit 0; fi\ncase \"$4\" in\n  /home/bob/.local/share/applications/foo.desktop) printf '%s' '[Desktop Entry]\nName=Foo\nExec=/usr/bin/foo %u\nTryExec=/usr/bin/foo\nIcon=foo\n'; exit 0;;\n  /usr/share/icons/hicolor/48x48/apps/foo.png) printf '%s' 'PNGDATA'; exit 0;;\n  *) exit 1;;\nesac\n",
            sb.root.join("podman.log").display()
        ),
    )
    .unwrap();
    #[cfg(unix)]
    make_executable(&sb.root.join("bin/podman"));
}

/// Exported launchers must be host-valid: user-installed apps are found via
/// the container user's home, the container-side TryExec (which would hide
/// the launcher) is dropped, and Icon points at the copied host icon.
#[test]
fn export_app_rewrites_launcher_for_host() {
    let sb = Sandbox::new("export-app");
    sb.write_config("mybox", "mybox");
    stub_podman(&sb);

    let out = sb.run(&["-C", "mybox", "export", "app", "foo"]);
    assert!(
        out.status.success(),
        "export app failed: {}",
        sandbox::text(&out)
    );

    let launcher = sb.root.join("data/applications/podbox-mybox-foo.desktop");
    let content = std::fs::read_to_string(&launcher)
        .expect("exported launcher must exist (user-dirs lookup must find it)");
    assert!(
        content.contains("--container \"mybox\" run /usr/bin/foo %u"),
        "Exec must be rewritten, got:\n{content}"
    );
    assert!(
        !content.lines().any(|l| l.starts_with("TryExec=")),
        "container-side TryExec must be dropped, got:\n{content}"
    );
    let icon_line = content
        .lines()
        .find(|l| l.starts_with("Icon="))
        .unwrap_or("Icon=<missing>");
    let expected_icon = sb
        .root
        .join("data/icons/podbox/mybox/foo.png")
        .to_string_lossy()
        .into_owned();
    assert_eq!(
        icon_line,
        format!("Icon={expected_icon}"),
        "Icon must point at the copied host icon"
    );
    assert_eq!(
        std::fs::read(sb.root.join("data/icons/podbox/mybox/foo.png")).unwrap(),
        b"PNGDATA"
    );
}

/// `export clean` for `box` must not delete `box2`'s launchers.
#[test]
fn export_clean_keeps_sibling_container_launchers() {
    let sb = Sandbox::new("export-clean");
    sb.write_config("mybox", "mybox");
    let apps = sb.root.join("data/applications");
    std::fs::create_dir_all(&apps).unwrap();
    std::fs::write(apps.join("podbox-mybox-x.desktop"), "[Desktop Entry]\n").unwrap();
    std::fs::write(apps.join("podbox-mybox2-y.desktop"), "[Desktop Entry]\n").unwrap();

    let out = sb.run(&["-C", "mybox", "export", "clean"]);
    assert!(
        out.status.success(),
        "export clean failed: {}",
        sandbox::text(&out)
    );
    assert!(
        !apps.join("podbox-mybox-x.desktop").exists(),
        "own launcher must be removed"
    );
    assert!(
        apps.join("podbox-mybox2-y.desktop").exists(),
        "sibling container launcher must be kept"
    );
}
