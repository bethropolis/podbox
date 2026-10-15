mod sandbox;

use sandbox::Sandbox;

/// `clone --copy-home` must copy the source config's actual home into the
/// clone's home: its contents (not a nested copy), and without the source's
/// dotfiles-provisioned stamp (a fresh box must re-provision).
#[test]
fn clone_copy_home_uses_configured_homes() {
    let sb = Sandbox::new("clone");
    // Source config with a non-default home.
    let mut cfg = podbox::config::Config::embedded();
    cfg.container.name = "srcbox".to_string();
    cfg.image.name = "srcbox".to_string();
    cfg.container.home = sb.root.join("custom-src-home");
    std::fs::write(
        sb.profiles().join("srcbox.toml"),
        toml::to_string_pretty(&cfg).unwrap(),
    )
    .unwrap();
    std::fs::create_dir_all(cfg.container.home.join("docs")).unwrap();
    std::fs::write(cfg.container.home.join("hello.txt"), "hi\n").unwrap();
    std::fs::write(cfg.container.home.join(".podbox-dotfiles.done"), "done\n").unwrap();

    let home = sb.root.join("home").to_string_lossy().into_owned();
    let out = sb.run_env(
        &["clone", "srcbox", "dstbox", "--copy-home"],
        &[("HOME", home.as_str())],
    );
    assert!(
        out.status.success(),
        "clone failed: {}",
        sandbox::text(&out)
    );

    // The clone's home sits next to the source home.
    let new_home = sb.root.join("dstbox");
    let cloned: podbox::config::Config =
        toml::from_str(&std::fs::read_to_string(sb.profiles().join("dstbox.toml")).unwrap())
            .unwrap();
    assert_eq!(cloned.container.home, new_home);

    // Contents land directly in the new home, not nested one deeper.
    assert_eq!(
        std::fs::read_to_string(new_home.join("hello.txt")).unwrap(),
        "hi\n"
    );
    assert!(new_home.join("docs").is_dir());
    assert!(
        !new_home.join("custom-src-home").exists(),
        "`cp -a src dst` nesting must not happen"
    );
    // The provisioning stamp must not carry over.
    assert!(
        !new_home.join(".podbox-dotfiles.done").exists(),
        "clone must re-provision dotfiles"
    );
    // The source is untouched.
    assert!(cfg.container.home.join("hello.txt").exists());
}
