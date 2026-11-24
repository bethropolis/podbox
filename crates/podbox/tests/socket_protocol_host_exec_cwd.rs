//! V2 host-exec working-directory mapping, fallback, and refusal tests.

#[path = "support/socket_protocol.rs"]
mod socket_protocol;

use std::os::fd::{AsRawFd, OwnedFd, RawFd};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use podbox::config::{Config, HostExecEntry};
use podbox::protocol::{CAP_HOST_EXEC, CAP_HOST_EXEC_FDS, GuestMessage, HostMessage, write_frame};
use podbox::socket_host::{CwdMap, serve_connection};
use socket_protocol::{Recorder, enabled_config, finish, hello, recv, send};

/// Hello with `host_exec_fds` granted.
fn hello_fds() -> GuestMessage {
    hello(&[CAP_HOST_EXEC, CAP_HOST_EXEC_FDS])
}

/// Serialize a `HostExecV2` frame and send it with descriptors attached,
/// exactly the way the guest shim does.
fn send_v2_with_fds(c: &mut UnixStream, msg: &GuestMessage, fds: &[OwnedFd]) {
    let mut wire = Vec::new();
    write_frame(&mut wire, msg).unwrap();
    let raws: Vec<RawFd> = fds.iter().map(AsRawFd::as_raw_fd).collect();
    podbox::protocol::sysutil::send_bytes_with_fds(c, &[&wire], &raws).unwrap();
}

/// A pipe whose read end the child will take as one of its stdio fds.
/// Sending the descriptors over SCM_RIGHTS transfers the *same pipe*, so
/// the bytes the guest writes are the bytes the child reads — with no
/// protocol frame carrying them at all.
fn pipe_pair() -> (OwnedFd, OwnedFd) {
    let (read, write) = std::io::pipe().unwrap();
    (read.into(), write.into())
}

/// A config that also allowlists `sleep` for the relay test.
fn fds_config() -> Config {
    let mut cfg = enabled_config();
    cfg.integration
        .host_exec
        .allowlist
        .as_mut()
        .unwrap()
        .insert(
            "sleep".to_string(),
            HostExecEntry::Simple("/usr/bin/sleep".into()),
        );
    cfg
}

/// Start the loop with an explicitly pinned cwd map.
fn start_cwd(
    cfg: Config,
    cwd_map: CwdMap,
    rec: Arc<Recorder>,
) -> (UnixStream, JoinHandle<anyhow::Result<()>>) {
    let (client, mut server) = UnixStream::pair().unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let handle =
        std::thread::spawn(move || serve_connection(&mut server, &cfg, "dev", &*rec, &cwd_map));
    (client, handle)
}

/// A V2 request with a guest working directory attached.
fn v2_cwd(cmd: &str, args: Vec<String>, cwd: Option<String>) -> GuestMessage {
    GuestMessage::HostExecV2 {
        cmd: cmd.into(),
        args,
        env: vec![],
        cwd,
        tty: false,
    }
}

/// A allowlisted script that prints its working directory.
fn pwd_script(dir: &Path) -> String {
    use std::os::unix::fs::PermissionsExt;
    let script = dir.join("showpwd.sh");
    std::fs::write(&script, "#!/bin/sh\npwd\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    script.to_string_lossy().into_owned()
}

/// Config + cwd map pair used by the cwd tests: the container's staging
/// home, the "host home" the map falls back to, and a container-side cwd
/// under the map's derived container-home prefix.
struct CwdFixture {
    staging: tempfile::TempDir,
    fallback: tempfile::TempDir,
    cfg: Config,
    cwd_map: CwdMap,
}

impl CwdFixture {
    /// A fds-capable config allowlisting a `pwd` script, whose home is the
    /// staging dir, whose cwd policy is `policy`, and whose cwd map is
    /// built against `fallback` as the host home.
    fn new(policy: podbox::config::HostExecCwd) -> Self {
        let staging = tempfile::tempdir().unwrap();
        let fallback = tempfile::tempdir().unwrap();
        let mut cfg = fds_config();
        cfg.integration
            .host_exec
            .allowlist
            .as_mut()
            .unwrap()
            .insert(
                "showpwd".to_string(),
                HostExecEntry::Simple(pwd_script(staging.path())),
            );
        cfg.integration.host_exec.cwd = policy;
        // The map is built from a config whose home is the staging dir;
        // rebuilding through `Config::parse` keeps the fixture closely
        // aligned with a real parsed definition.
        let home = staging.path().to_string_lossy().into_owned();
        let mapped_home = Config::parse(&format!(
            "[image]\nbase = \"fedora:41\"\nname = \"env\"\n[container]\nname = \"env\"\nhome = \"{home}\"\n"
        ))
        .unwrap();
        let cwd_map = CwdMap::build(&mapped_home, None, fallback.path());
        Self {
            staging,
            fallback,
            cfg,
            cwd_map,
        }
    }
}

/// With cwd = "map", the child starts in the host directory the guest cwd
/// is bound from: a guest `…/projects` ends up in the staging home's
/// `projects`, and the exit is clean.
#[test]
fn mapped_guest_cwd_starts_the_child_on_the_host_path() {
    use podbox::config::HostExecCwd;
    let fix = CwdFixture::new(HostExecCwd::Map);
    let container_user = fix
        .fallback
        .path()
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("root");
    let sub = fix.staging.path().join("subdir");
    std::fs::create_dir(&sub).unwrap();

    let (mut c, h) = start_cwd(fix.cfg, fix.cwd_map, Arc::default());
    send(&mut c, &hello_fds());
    let _ = recv(&mut c);

    let (out_r, out_w) = pipe_pair();
    let (_, stderr_w) = pipe_pair();
    send_v2_with_fds(
        &mut c,
        &v2_cwd(
            "showpwd",
            vec![],
            Some(format!("/home/{container_user}/subdir")),
        ),
        &[pipe_pair().0, out_w, stderr_w],
    );
    let mut out = String::new();
    use std::io::Read;
    std::fs::File::from(out_r).read_to_string(&mut out).unwrap();
    assert_eq!(
        out.trim(),
        sub.canonicalize().unwrap().to_string_lossy().to_string(),
        "the child ran in the mapped host directory"
    );
    assert!(matches!(
        recv(&mut c),
        HostMessage::HostExecDone { exit_code: 0, .. }
    ));
    finish(c, h).unwrap();
}

/// With cwd = "map", a guest path with no binding falls back to the host
/// home and the guest gets a stderr note naming the unmapped path before
/// the child's output — through its own descriptor.
#[test]
fn unmapped_guest_cwd_falls_back_to_the_host_home_with_a_warning() {
    use podbox::config::HostExecCwd;
    let fix = CwdFixture::new(HostExecCwd::Map);
    let (mut c, h) = start_cwd(fix.cfg, fix.cwd_map, Arc::default());
    send(&mut c, &hello_fds());
    let _ = recv(&mut c);

    let (out_r, out_w) = pipe_pair();
    let (our_err, err_w) = pipe_pair();
    send_v2_with_fds(
        &mut c,
        &v2_cwd("showpwd", vec![], Some("/srv/unmapped-dir".into())),
        &[pipe_pair().0, out_w, err_w],
    );

    let mut err = String::new();
    use std::io::Read;
    std::fs::File::from(our_err)
        .read_to_string(&mut err)
        .unwrap();
    assert!(
        err.contains("no mapping to the host") && err.contains("/srv/unmapped-dir"),
        "warning should name the guest path: {err:?}"
    );

    let mut out = String::new();
    std::fs::File::from(out_r).read_to_string(&mut out).unwrap();
    assert_eq!(
        out.trim(),
        fix.fallback
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string(),
        "fallback start directory is the host home of the map"
    );
    assert!(matches!(
        recv(&mut c),
        HostMessage::HostExecDone { exit_code: 0, .. }
    ));
    finish(c, h).unwrap();
}

/// With cwd = "refuse", an unmappable guest path is a typed error naming
/// the path, nothing runs, and the session stays usable.
#[test]
fn refuse_policy_rejects_an_unmapped_cwd_without_executing() {
    use podbox::config::HostExecCwd;
    let fix = CwdFixture::new(HostExecCwd::Refuse);
    let (mut c, h) = start_cwd(fix.cfg, fix.cwd_map, Arc::default());
    send(&mut c, &hello_fds());
    let _ = recv(&mut c);

    let guest = "/srv/nowhere".to_string();
    send_v2_with_fds(
        &mut c,
        &v2_cwd("showpwd", vec![], Some(guest.clone())),
        &[pipe_pair().0, pipe_pair().1, pipe_pair().1],
    );
    match recv(&mut c) {
        HostMessage::Error { reason, code } => {
            assert!(reason.contains(&guest), "reason names the path: {reason:?}");
            assert_eq!(code.as_deref(), Some("not_allowed"));
        }
        other => panic!("expected typed Error, got {other:?}"),
    }
    // Nothing was executed and the connection still works for the
    // historical request shape.
    send(
        &mut c,
        &GuestMessage::HostExec {
            cmd: "echo".into(),
            args: vec!["after-refuse".into()],
        },
    );
    match recv(&mut c) {
        HostMessage::HostExecStdout { data } => assert!(data.contains("after-refuse")),
        other => panic!("expected the session to survive, got {other:?}"),
    }
    assert!(matches!(recv(&mut c), HostMessage::HostExecDone { .. }));
    finish(c, h).unwrap();
}

/// With the `home` cwd policy, the guest path is ignored: the child starts
/// in the host home and no warning is emitted.
#[test]
fn home_cwd_policy_ignores_the_guest_path() {
    use podbox::config::HostExecCwd;
    let fix = CwdFixture::new(HostExecCwd::Home);
    let (mut c, h) = start_cwd(fix.cfg, fix.cwd_map, Arc::default());
    send(&mut c, &hello_fds());
    let _ = recv(&mut c);

    let (out_r, out_w) = pipe_pair();
    let (our_err, err_w) = pipe_pair();
    send_v2_with_fds(
        &mut c,
        &v2_cwd("showpwd", vec![], Some("/home/somewhere/projects".into())),
        &[pipe_pair().0, out_w, err_w],
    );

    let mut err = String::new();
    use std::io::Read;
    std::fs::File::from(our_err)
        .read_to_string(&mut err)
        .unwrap();
    assert!(
        err.is_empty(),
        "no warning under the default policy: {err:?}"
    );

    let mut out = String::new();
    std::fs::File::from(out_r).read_to_string(&mut out).unwrap();
    assert_eq!(
        out.trim(),
        fix.fallback
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string(),
        "the guest cwd is ignored: the child starts in the host home"
    );
    assert!(matches!(
        recv(&mut c),
        HostMessage::HostExecDone { exit_code: 0, .. }
    ));
    finish(c, h).unwrap();
}
