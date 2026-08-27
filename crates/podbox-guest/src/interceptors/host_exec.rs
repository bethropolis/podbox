use crate::protocol::{GuestMessage, HostMessage, read_frame, write_frame};
use crate::socket::{connect_to_host, container_name, handshake, host_socket_path};

pub fn run(args: &[String]) {
    if args.len() < 2 {
        eprintln!("host-exec: usage: host-exec <command> [args...]");
        std::process::exit(1);
    }

    let msg = GuestMessage::HostExec {
        cmd: args[1].clone(),
        args: args[2..].to_vec(),
    };

    let path = host_socket_path().unwrap_or_else(|e| {
        eprintln!("host-exec: {e}");
        std::process::exit(1);
    });
    let mut stream = connect_to_host(&path).unwrap_or_else(|e| {
        eprintln!("host-exec: connect failed: {e}");
        std::process::exit(1);
    });

    // The host gates each connection on a `Hello` negotiation before any
    // privileged message is honored. Advertise host-exec (matching what the
    // guest daemon itself requests) so the host grants the capability here.
    let container = container_name().unwrap_or_default();
    let caps: Vec<String> = crate::protocol::ALL_CAPABILITIES
        .iter()
        .map(|&s| s.to_string())
        .collect();
    let (accepted, _idle_timeout) = handshake(&mut stream, &container, &caps).unwrap_or_else(
        |e| {
            eprintln!("host-exec: negotiation failed: {e}");
            std::process::exit(1);
        },
    );
    if !accepted
        .iter()
        .any(|c| c == crate::protocol::CAP_HOST_EXEC)
    {
        eprintln!("host-exec: capability 'host_exec' not accepted by host");
        std::process::exit(1);
    }
    if write_frame(&mut stream, &msg).is_err() {
        std::process::exit(1);
    }

    let mut exit_code = 0i32;
    while let Ok(Some(bytes)) = read_frame(&mut stream) {
        match serde_json::from_slice::<HostMessage>(&bytes) {
            Ok(HostMessage::HostExecStdout { data }) => print!("{data}"),
            Ok(HostMessage::HostExecStderr { data }) => eprint!("{data}"),
            Ok(HostMessage::HostExecDone { exit_code: code }) => {
                exit_code = code;
                break;
            }
            _ => break,
        }
    }
    std::process::exit(exit_code);
}
