mod sandbox;

use sandbox::{Sandbox, text};

fn host_terminal_env() -> [(&'static str, &'static str); 6] {
    [
        ("TERM", "xterm-ghostty"),
        ("COLORTERM", "truecolor"),
        ("TERM_PROGRAM", "ghostty"),
        ("TERM_PROGRAM_VERSION", "1.2.3"),
        ("KITTY_WINDOW_ID", "7"),
        ("KITTY_LISTEN_ON", "unix:/tmp/kitty.sock"),
    ]
}

#[test]
fn dry_run_enter_forwards_host_terminal_identity() {
    let sb = Sandbox::new("terminal-env-enter");
    sb.write_config("mybox", "mybox");

    let out = sb.run_env(&["--dry-run", "enter", "mybox"], &host_terminal_env());
    let output = text(&out);
    assert!(out.status.success(), "{output}");
    for expected in [
        "--env=TERM=xterm-ghostty",
        "--env=COLORTERM=truecolor",
        "--env=TERM_PROGRAM=ghostty",
        "--env=TERM_PROGRAM_VERSION=1.2.3",
        "--env=KITTY_WINDOW_ID=7",
    ] {
        assert!(output.contains(expected), "missing {expected}: {output}");
    }
    assert!(
        !output.contains("KITTY_LISTEN_ON"),
        "terminal sockets must stay on the host: {output}"
    );
}

#[test]
fn dry_run_exec_forwards_the_same_terminal_identity() {
    let sb = Sandbox::new("terminal-env-exec");
    sb.write_config("mybox", "mybox");

    let out = sb.run_env(
        &["--dry-run", "exec", "mybox", "sh", "-c", "true"],
        &host_terminal_env(),
    );
    let output = text(&out);
    assert!(out.status.success(), "{output}");
    assert!(output.contains("--env=TERM=xterm-ghostty"), "{output}");
    assert!(output.contains("--env=TERM_PROGRAM=ghostty"), "{output}");
    assert!(!output.contains("KITTY_LISTEN_ON"), "{output}");
}

#[test]
fn explicit_environment_overrides_host_terminal_identity() {
    let sb = Sandbox::new("terminal-env-explicit");
    sb.write_config("mybox", "mybox");

    let out = sb.run_env(
        &["--dry-run", "enter", "-e", "TERM=screen-256color", "mybox"],
        &host_terminal_env(),
    );
    let output = text(&out);
    assert!(out.status.success(), "{output}");
    assert!(output.contains("--env=TERM=screen-256color"), "{output}");
    assert!(!output.contains("--env=TERM=xterm-ghostty"), "{output}");
    assert!(output.contains("--env=COLORTERM=truecolor"), "{output}");
}

#[test]
fn configured_environment_overrides_host_terminal_identity() {
    let sb = Sandbox::new("terminal-env-config");
    let path = sb.write_config("mybox", "mybox");
    let mut config =
        podbox::config::Config::parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
    config
        .container
        .env
        .values
        .insert("TERM".into(), "screen-256color".into());
    std::fs::write(&path, toml::to_string_pretty(&config).unwrap()).unwrap();

    let out = sb.run_env(&["--dry-run", "enter", "mybox"], &host_terminal_env());
    let output = text(&out);
    assert!(out.status.success(), "{output}");
    assert!(!output.contains("--env=TERM=xterm-ghostty"), "{output}");
    assert!(!output.contains("--env=TERM="), "{output}");
    assert!(output.contains("--env=COLORTERM=truecolor"), "{output}");
}

#[test]
fn forwarding_term_does_not_duplicate_the_automatic_value() {
    let sb = Sandbox::new("terminal-env-forward");
    let path = sb.write_config("mybox", "mybox");
    let mut config =
        podbox::config::Config::parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
    config.container.env.forward = vec!["TERM".into()];
    std::fs::write(&path, toml::to_string_pretty(&config).unwrap()).unwrap();

    let out = sb.run_env(&["--dry-run", "enter", "mybox"], &host_terminal_env());
    let output = text(&out);
    assert!(out.status.success(), "{output}");
    assert_eq!(output.matches("--env=TERM=xterm-ghostty").count(), 1);
}

#[test]
fn empty_host_terminal_variables_are_not_forwarded() {
    let sb = Sandbox::new("terminal-env-empty");
    sb.write_config("mybox", "mybox");

    let out = sb.run_env(
        &["--dry-run", "enter", "mybox"],
        &[("TERM", ""), ("COLORTERM", "")],
    );
    let output = text(&out);
    assert!(out.status.success(), "{output}");
    assert!(!output.contains("--env=TERM="), "{output}");
    assert!(!output.contains("--env=COLORTERM="), "{output}");
}
