use crate::protocol::{CAP_HOST_EXEC, GuestMessage, HostMessage, read_frame, write_frame};
use crate::socket::connect_negotiated;

pub fn run(args: &[String]) {
    if args.len() < 2 {
        eprintln!("host-exec: usage: host-exec <command> [args...]");
        std::process::exit(1);
    }
    run_as_command(&args[1], &args[2..]);
}

pub fn run_as_command(cmd: &str, args: &[String]) {
    let msg = GuestMessage::HostExec {
        cmd: cmd.to_string(),
        args: args.to_vec(),
    };

    // Negotiation happens before the request is written; a refused capability
    // is reported here rather than by the host dropping the connection.
    let mut stream = connect_negotiated(&[CAP_HOST_EXEC]).unwrap_or_else(|e| {
        eprintln!("host-exec: {e}");
        std::process::exit(1);
    });
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
