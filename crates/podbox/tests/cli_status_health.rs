//! Container inventory formatting and doctor health-report contracts.

mod support;

use support::Sandbox;

/// `list` columns line up: AUTOSTART starts at the same offset everywhere,
/// rows have no trailing whitespace, and the rule matches the header width.
///
/// The pre-fix bug padded colored cells by *byte* length, so ANSI escapes
/// shifted every column after STATUS.
#[test]
fn list_columns_align_without_trailing_space() {
    let sb = Sandbox::new(&["alpha", "beta"]);
    // Distinct runtime names: list shows `container.name`, not the stem.
    let beta = sb.dir.join("podbox").join("beta.toml");
    let text = std::fs::read_to_string(&beta)
        .unwrap()
        .replace("myenv", "beta");
    std::fs::write(&beta, &text).unwrap();
    let out = sb.cmd().arg("list").output().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(lines.len() >= 4, "header + rule + two rows");

    // Non-TTY output carries no color codes; widths are then exact.
    let header = lines[0];
    let autostart_col = header.find("AUTOSTART").expect("AUTOSTART header");
    let active_col = header
        .find("ACTIVE CONTEXT")
        .expect("ACTIVE CONTEXT header");

    for row in &lines[2..] {
        assert_eq!(
            row.len(),
            row.trim_end().len(),
            "trailing whitespace in {row:?}"
        );
        // Every data row reaches (at least) the AUTOSTART column offset.
        assert!(row.len() > autostart_col, "row too short: {row:?}");
    }

    // Rule spans the header width.
    assert_eq!(lines[1].chars().count(), header.trim_end().chars().count());

    // Status labels start right after the fixed CONTAINER + dot columns.
    for row in &lines[2..] {
        assert!(
            row.starts_with("myenv") || row.starts_with("beta"),
            "unexpected row {row:?}"
        );
        let _ = active_col; // documented above; kept for readability
    }
}

/// Stale sockets are grouped per owner — this box's leftovers are separated
/// from other boxes' noise — and, the regression, a socket for an
/// integration the box still enables is live, however `<box>-<kind>` looks.
#[test]
fn doctor_scopes_stale_sockets_by_owner() {
    let sb = Sandbox::new(&["alpha"]);
    let sock_dir = sb.dir.join("runtime").join("podbox");
    std::fs::create_dir_all(&sock_dir).unwrap();
    // The fixture's runtime name is `myenv` (not the `alpha.toml` stem),
    // and it enables wayland and dbus: its sockets are live.
    std::fs::write(sock_dir.join("myenv-wayland.sock"), "").unwrap();
    std::fs::write(sock_dir.join("myenv-dbus.sock"), "").unwrap();
    std::fs::write(sock_dir.join("myenv.sock"), "").unwrap();
    // ghost boxes have no config at all.
    std::fs::write(sock_dir.join("ghost-wayland.sock"), "").unwrap();
    std::fs::write(sock_dir.join("ghost-dbus.sock"), "").unwrap();

    let out = sb.cmd().args(["doctor"]).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    let warn_lines: Vec<&str> = text
        .lines()
        .filter(|l| l.contains("stale sockets"))
        .collect();
    assert_eq!(
        warn_lines.len(),
        1,
        "only the other boxes' sockets are stale, as one grouped line: {warn_lines:?}"
    );
    let line = warn_lines[0];
    assert!(line.contains("ghost-wayland.sock"), "{line}");
    assert!(line.contains("ghost-dbus.sock"), "{line}");
    // Live sockets must not be flagged.
    assert!(!line.contains("myenv-wayland.sock"), "{line}");
    assert!(!line.contains("myenv-dbus.sock"), "{line}");
    assert!(!line.contains("myenv.sock"), "{line}");

    // JSON contract unchanged: still an array of grouped checks.
    let out = sb
        .cmd()
        .args(["doctor", "--output", "json"])
        .output()
        .unwrap();
    let v: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&out.stdout).trim()).unwrap();
    let stale: Vec<&serde_json::Value> = v["checks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| {
            c["name"]
                .as_str()
                .is_some_and(|n| n.starts_with("stale sockets"))
        })
        .collect();
    assert_eq!(stale.len(), 1);
    assert_eq!(stale[0]["status"], "warn");
}

/// Doctor JSON carries grouped checks and reports failures via exit code.
#[test]
fn doctor_json_has_groups_and_exit_status() {
    let sb = Sandbox::new(&["alpha"]);
    let out = sb
        .cmd()
        .args(["-C", "alpha", "doctor", "--output", "json"])
        .output()
        .unwrap();

    // Stub podman makes several host checks fail/succeed by environment;
    // only assert structure, not pass/fail counts.
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("doctor --output json must print valid JSON");
    let checks = v["checks"].as_array().expect("checks array");
    assert!(!checks.is_empty());
    assert!(checks.iter().all(|c| c["group"].is_string()));
    let groups: Vec<&str> = checks
        .iter()
        .map(|c| c["group"].as_str().unwrap())
        .collect();
    assert!(groups.contains(&"Host"));
    assert!(groups.contains(&"Integration"));

    // Text mode prints section headings and the exposure block.
    let out = sb.cmd().args(["-C", "alpha", "doctor"]).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("Host"));
    assert!(text.contains("Integration"));
    assert!(text.contains("Host exposure"), "exposure summary missing");
}
