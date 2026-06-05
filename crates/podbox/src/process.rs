use std::ffi::OsString;
use std::os::unix::process::CommandExt;
use std::process::{Command, ExitStatus, Output};

/// Build a `Vec<OsString>` from a slice of `&str`/`&String` literals.
///
/// Use this to construct argument lists for `podman` (or any other)
/// invocations. Slightly more readable than `.into()` on every element:
///
/// ```
/// let name = "myenv";
/// let args = podbox::process::args(&["exec", "-it", name]);
/// ```
pub fn args<S: AsRef<str>>(items: &[S]) -> Vec<OsString> {
    items.iter().map(|s| OsString::from(s.as_ref())).collect()
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_builds_osstring_vec() {
        let v = args(&["foo", "bar", "baz"]);
        assert_eq!(v.len(), 3);
        assert_eq!(v[0], "foo");
        assert_eq!(v[1], "bar");
        assert_eq!(v[2], "baz");
    }

    #[test]
    fn args_accepts_mixed_types() {
        let s = String::from("hello");
        let v = args(&["a", &s, "c"]);
        assert_eq!(v[1], "hello");
    }
}
