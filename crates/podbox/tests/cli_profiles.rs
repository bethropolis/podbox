//! User profile precedence, import, and export contracts.

mod support;

use std::path::PathBuf;

use support::Sandbox;

/// User configs win over bundled profiles: a `dev.toml` on disk (here the
/// legacy root path, proving legacy configs count too) is what `profile
/// export` and `create` use — not the bundled `dev` template.
#[test]
fn user_config_wins_over_bundled_profile() {
    let sb = Sandbox::new(&["dev"]);
    // Sanity: the fixture really is on disk under that name.
    let out = sb
        .cmd()
        .args(["profile", "export", "dev"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("fedora:41"),
        "must print the user's dev.toml, got:\n{stdout}"
    );
    assert!(
        !stdout.contains("dev-latest"),
        "must not print the bundled dev template"
    );
}

/// `profile import` installs a file; re-import bails unless `--force`.
#[test]
fn profile_import_installs_and_force_overwrites() {
    let sb = Sandbox::new(&[]);
    let src = sb.dir.join("studio.toml");
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/full.toml");
    std::fs::copy(&fixture, &src).unwrap();
    let target = sb.dir.join("podbox").join("profiles").join("studio.toml");

    let out = sb
        .cmd()
        .args(["profile", "import", src.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(target.is_file(), "import must stage profiles/studio.toml");

    let out = sb
        .cmd()
        .args(["profile", "import", src.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "second import must refuse to clobber"
    );

    let out = sb
        .cmd()
        .args(["profile", "import", src.to_str().unwrap(), "--force"])
        .output()
        .unwrap();
    assert!(out.status.success(), "--force must overwrite");
}

/// `profile export --output` writes the TOML to a file.
#[test]
fn profile_export_output_writes_file() {
    let sb = Sandbox::new(&["myenv"]);
    let dest = sb.dir.join("exported.toml");
    let out = sb
        .cmd()
        .args([
            "profile",
            "export",
            "myenv",
            "--output",
            dest.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let body = std::fs::read_to_string(&dest).unwrap();
    assert!(body.contains("fedora:41"), "exported body:\n{body}");
}
