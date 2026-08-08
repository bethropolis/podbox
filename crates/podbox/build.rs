use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let version = std::env::var("PODBOX_VERSION")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(git_describe)
        .unwrap_or_else(|| std::env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION not set"));
    println!("cargo:rustc-env=PODBOX_VERSION={version}");

    embed_guest();
}

fn git_describe() -> Option<String> {
    let output = Command::new("git")
        .args(["describe", "--tags", "--always", "--dirty=-dirty"])
        .output()
        .ok()?;
    if output.status.success() {
        String::from_utf8(output.stdout)
            .ok()
            .map(|s| s.trim().to_string())
    } else {
        None
    }
}

fn embed_guest() {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let dest = out_dir.join("podbox_guest.rs");

    // The guest source only exists when building from the workspace (dev,
    // goreleaser, source checkout). The crates.io package ships `podbox-cli`
    // alone, so the workspace sibling is absent and custom image builds are
    // unsupported — the crate can only manage prebuilt images.
    let workspace_manifest = Path::new("../podbox-guest/Cargo.toml");
    if workspace_manifest.exists() {
        embed_guest_from_workspace(&dest);
    } else {
        std::fs::write(&dest, "pub static PODBOX_GUEST: Option<&[u8]> = None;")
            .expect("failed to write podbox_guest.rs");
        println!("cargo:warning=no podbox-guest workspace found; custom image builds unsupported (prebuilt images only)");
    }
}

fn embed_guest_from_workspace(dest: &Path) {
    println!("cargo:rerun-if-changed=../podbox-guest/src/");
    println!("cargo:rerun-if-changed=../podbox-guest/Cargo.toml");

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace_root = manifest_dir
        .parent()
        .expect("CARGO_MANIFEST_DIR has no parent")
        .parent()
        .expect("crates/podbox should have a grandparent workspace root");
    let guest_target = workspace_root.join("target").join("guest-build");
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    // Prefer a fully static musl build so the embedded guest works in any
    // container regardless of libc. Fall back to the host default target if
    // the musl target isn't installed.
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let musl_target = match arch.as_str() {
        "aarch64" => "aarch64-unknown-linux-musl",
        _ => "x86_64-unknown-linux-musl",
    };
    let musl_available = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .ok()
        .is_some_and(|o| String::from_utf8_lossy(&o.stdout).contains(musl_target));

    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let (guest_path, target_label) = if musl_available {
        let path = guest_target
            .join(musl_target)
            .join("release")
            .join("podbox-guest");
        let status = Command::new(&cargo)
            .args([
                "build",
                "--release",
                "--target",
                musl_target,
                "-p",
                "podbox-guest",
                "--target-dir",
            ])
            .arg(&guest_target)
            .status()
            .expect("Failed to launch cargo build for podbox-guest");
        assert!(status.success(), "podbox-guest musl build failed");
        (path, "musl / static")
    } else {
        let path = guest_target.join("release").join("podbox-guest");
        let status = Command::new(&cargo)
            .args(["build", "--release", "-p", "podbox-guest", "--target-dir"])
            .arg(&guest_target)
            .status()
            .expect("Failed to launch cargo build for podbox-guest");
        assert!(status.success(), "podbox-guest build failed");
        (path, "dynamic")
    };

    println!("cargo:warning=podbox-guest binary built ({target_label})");

    let guest_bytes = std::fs::read(&guest_path).expect("failed to read podbox-guest binary");

    std::fs::write(out_dir.join("podbox-guest"), &guest_bytes)
        .expect("failed to copy podbox-guest to OUT_DIR");

    let size = guest_bytes.len();
    let code = format!(
        r#"
pub static PODBOX_GUEST: Option<&[u8]> = Some({{
    const RAW: &[u8; {size}] = include_bytes!(concat!(env!("OUT_DIR"), "/podbox-guest"));
    RAW
}});
"#,
    );
    std::fs::write(dest, code).expect("failed to write podbox_guest.rs");
}
