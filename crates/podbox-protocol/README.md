# podbox-protocol

Internal wire-protocol crate for [podbox](https://crates.io/crates/podbox-cli): the length-prefixed JSON message types (`GuestMessage` / `HostMessage`) and framing shared between the podbox host CLI and its `podbox-guest` daemon.

This is an internal protocol crate, not intended for standalone use outside the podbox project. Version compatibility between the protocol and the host/guest binaries is pinned by the podbox workspace, and is not guaranteed across podbox releases beyond that.

If you're building podbox from source, you don't need to depend on this crate directly. See the [podbox-cli](https://crates.io/crates/podbox-cli) package or the [source repository](https://github.com/bethropolis/podbox) for the full project.
