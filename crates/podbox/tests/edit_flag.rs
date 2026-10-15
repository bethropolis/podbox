mod sandbox;

use sandbox::Sandbox;

/// `--edit` on build/start/enter must open the config the command actually
/// runs against. It used to resolve only `-C`/auto-detect, so
/// `--config <path> build --edit` and `build <name> --edit` opened (or
/// failed on) the wrong file.
#[test]
fn edit_uses_positional_name() {
    let sb = Sandbox::new("edit");
    sb.write_config("aaa", "aaa");
    sb.write_config("mybox", "mybox");

    let out = sb.run(&["--dry-run", "build", "mybox", "--edit"]);
    let text = sandbox::text(&out);
    assert!(out.status.success(), "build --edit failed: {text}");
    assert!(
        text.contains("mybox.toml"),
        "--edit must open the positional container's config, got: {text}"
    );
    assert!(
        !text.contains("aaa.toml"),
        "--edit must not fall back to another config, got: {text}"
    );
}

/// An explicit `--config` path wins over every name lookup.
#[test]
fn edit_uses_explicit_config_path() {
    let sb = Sandbox::new("edit-config");
    sb.write_config("aaa", "aaa");
    let custom = sb.write_config("custom", "custom");

    let out = sb.run(&[
        "--config",
        &custom.to_string_lossy(),
        "--dry-run",
        "start",
        "--edit",
    ]);
    let text = sandbox::text(&out);
    assert!(out.status.success(), "start --edit failed: {text}");
    assert!(
        text.contains("custom.toml"),
        "--edit must open the explicit --config file, got: {text}"
    );
}
