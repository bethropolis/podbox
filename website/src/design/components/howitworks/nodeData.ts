export type TabType = 'build' | 'runtime' | 'protocol';
export interface NodeDetail {
  title: string;
  /** Short mono label above the title: where this piece lives. */
  category: string;
  description: string;
  /** Concrete facts, one line each. Keep them checkable against docs/. */
  specs: string[];
}

// Copy rule for this file: say what the piece does, in the words the docs
// use. No adjectives the code can't back up.
export const nodeDetails: Record<string, NodeDetail> = {
  // Build time
  build_toml: {
    title: 'podbox.toml',
    category: 'you write',
    description:
      'One file describes the environment: base image, packages, what the container may touch on the host, and how it should run. It is the only thing you edit; everything below is generated from it.',
    specs: [
      'Sections: [image], [container], [integration], [network], [security], [lifecycle]',
      'Can extend a bundled profile (extends = "profile:fedora")',
      'Lives wherever you like; commit it next to your project',
    ],
  },
  build_codegen: {
    title: 'podbox build',
    category: 'runs once, then exits',
    description:
      'Reads the TOML and writes two kinds of output: a Containerfile for the image and Quadlet unit files for systemd. Generation is a pure function of the config, so the same file always produces the same output.',
    specs: [
      'TOML → Containerfile + .container / .socket / .build units',
      'The guest binary is copied into the build context',
      'podbox build --dry-run prints everything without writing',
    ],
  },
  build_image: {
    title: 'OCI image',
    category: 'podman',
    description:
      'Built rootless by Podman from the generated Containerfile. Packages are installed at build time, so starting the container does not reinstall anything.',
    specs: [
      'Tagged locally as localhost/podbox-<name>:latest',
      'Or a prebuilt image from ghcr.io, used as-is',
      'podbox-guest is added as the entrypoint',
    ],
  },
  build_quadlet: {
    title: 'Quadlet units',
    category: '~/.config/containers/systemd',
    description:
      'Plain systemd unit files. Podman\'s Quadlet generator turns them into services at daemon-reload. Companion units cover the host socket, the D-Bus proxy and the Wayland filter when those are enabled.',
    specs: [
      '<name>.container: the container itself',
      '<name>.socket: host side of the guest socket, created before start',
      '<name>.build: rebuilds the image when the Containerfile changes',
    ],
  },
  build_systemd: {
    title: 'systemd --user',
    category: 'owns the lifecycle',
    description:
      'From here on it is an ordinary user service. Autostart, restart on failure and logs are systemd\'s job; podbox is not running in the background.',
    specs: [
      'systemctl --user status <name>.service',
      'journalctl --user -u <name>.service -f',
      'Restart=on-failure, WantedBy=default.target when autostart is on',
    ],
  },

  // While it runs
  rt_guest: {
    title: 'podbox-guest',
    category: 'inside the container',
    description:
      'The container\'s entrypoint. It sets up your user, then puts small stand-ins for notify-send, xdg-open, the clipboard and host-exec on PATH. Each one forwards its request to the host instead of needing a shared bus.',
    specs: [
      'Static binary at /usr/local/bin/podbox-guest',
      'Only installs stand-ins for capabilities the host accepted',
      'Talks to the host over one Unix socket',
    ],
  },
  rt_handshake: {
    title: 'Hello / HelloAck',
    category: 'socket handshake',
    description:
      'Every connection starts by naming the capabilities it wants. The host answers with what your config allows; anything else on that connection is refused.',
    specs: [
      'Capabilities: notify, xdg_open, clipboard, host_exec',
      'Rejected capabilities get no stand-in and no retries',
    ],
  },
  rt_wayland: {
    title: 'Wayland filter',
    category: 'host, per container',
    description:
      'GUI apps reach your compositor through a small proxy instead of the raw socket. It hides protocols a container has no business using, such as screen capture and virtual keyboards, and passes the rest through.',
    specs: [
      'Blocked list is configurable per container',
      'Normal windows, GPU rendering and input work as usual',
    ],
  },
  rt_socket: {
    title: 'Host socket service',
    category: 'host, socket-activated',
    description:
      'Started by systemd when the guest first connects. It shows notifications, sets and reads the clipboard, opens links, and runs host commands that your config allowlists. It stops again when idle.',
    specs: [
      'Socket: $XDG_RUNTIME_DIR/podbox/<name>.sock',
      'host-exec runs only commands listed in the config',
    ],
  },
  rt_dbus: {
    title: 'D-Bus proxy',
    category: 'host, optional',
    description:
      'When the container needs the session bus, it gets an xdg-dbus-proxy in front of it rather than the bus itself. Only the names you list are reachable.',
    specs: [
      'Rules come from [dbus] talk and own, or a preset',
      'Needs xdg-dbus-proxy installed on the host',
    ],
  },
  rt_host: {
    title: 'Your desktop',
    category: 'host',
    description:
      'Compositor, PipeWire and GPU are shared directly when you turn them on. Your files are not: the container gets its own home directory, plus whatever folders you list.',
    specs: [
      'Home: ~/containers/<name>, not your $HOME',
      'GPU via /dev/dri, or NVIDIA device nodes',
    ],
  },

  // Messages
  proto_handshake: {
    title: 'Handshake',
    category: 'first frame on every connection',
    description:
      'The guest sends hello with its version, container name and the capabilities it wants. The host replies hello_ack with accepted and rejected lists and an idle timeout.',
    specs: [
      'Frames are length-prefixed JSON over a Unix stream socket',
      'Version mismatches are reported, not silently ignored',
    ],
  },
  proto_channels: {
    title: 'Requests',
    category: 'after the handshake',
    description:
      'Each request is one typed message: notify, xdg_open, clipboard_set, clipboard_get or host_exec. Replies carry clipboard data, command output and exit codes, or which notification action was clicked.',
    specs: [
      'Full message list: docs/protocol.md',
      'The host can ping, ask about idleness, or send shutdown',
    ],
  },
};
