use std::ffi::OsString;
use std::os::unix::process::CommandExt;
use std::process::{Command, ExitStatus, Output};

/// Replace the current process with the given binary and arguments.
///
/// Uses `CommandExt::exec()` so the shell gets a real TTY.
/// On success this function never returns; on failure it returns an error.
pub fn exec_replace(bin: &str, args: &[OsString]) -> anyhow::Error {
    let mut cmd = Command::new(bin);
    cmd.args(args);
    let err = cmd.exec();
    anyhow::Error::from(err).context(format!("failed to exec {}", bin))
}

/// Run a command, capturing stdout and stderr.
pub fn run_piped(bin: &str, args: &[OsString]) -> anyhow::Result<Output> {
    let output = Command::new(bin)
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?
        .wait_with_output()?;
    Ok(output)
}

/// Spawn a command attached to the current terminal.
pub fn spawn_interactive(bin: &str, args: &[OsString]) -> anyhow::Result<ExitStatus> {
    let status = Command::new(bin).args(args).status()?;
    Ok(status)
}
