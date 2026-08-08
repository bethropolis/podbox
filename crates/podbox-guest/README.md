# podbox-guest

Guest-side daemon for [podbox](https://crates.io/crates/podbox-cli). This is the binary that runs *inside* a podbox container. It handles the host socket protocol, tracks processes for idle-timeout shutdown, and provides the notify / xdg-open / clipboard / host-exec interceptors that route requests back to the host.

It is built as a **static musl** binary, so it runs unchanged inside arbitrary container distros without libc version skew.

Most users never touch this crate directly: the guest is embedded into podbox at compile time when building from the full workspace, and the published podbox container images ship with it pre-baked.

See [podbox-cli](https://crates.io/crates/podbox-cli) for the host tool, or the [source repository](https://github.com/bethropolis/podbox) for the full project.
