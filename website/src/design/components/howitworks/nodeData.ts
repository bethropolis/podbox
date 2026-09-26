export type TabType = 'build' | 'runtime' | 'protocol';
export interface NodeDetail {
  title: string;
  category: string;
  description: string;
  specs: string[];
  code?: { lang: string; filename: string; snippet: string };
  command?: string;
}
export const nodeDetails: Record<string, NodeDetail> = {
    // Build-time nodes
    build_toml: {
      title: 'definition.toml (Declarative Config)',
      category: 'Input Specification',
      description:
        'The single source of truth for the container environment. Declares the base image, packages to bake in, GPU acceleration, GUI/audio passthrough, host mounts, and strict D-Bus proxy access rules.',
      specs: [
        'Version-controllable in git repositories',
        'Defines [container], [image], [gui], [mounts], and [dbus]',
        'Zero host-system contamination by default',
      ],
      code: {
        lang: 'toml',
        filename: 'podbox.toml',
        snippet: `[container]
name = "dev-rust"

[image]
base = "registry.fedoraproject.org/fedora:41"
packages = ["neovim", "cargo", "ripgrep", "wayland-devel"]

[gui]
wayland = true
audio = true
gpu = "auto"

[dbus]
proxy = true
talk = ["org.freedesktop.Notifications"]`,
      },
      command: 'podbox build .',
    },
    build_codegen: {
      title: 'podbox build / Pure Codegen Engine',
      category: 'Compiler & Synthesis',
      description:
        'A purely declarative compiler with no persistent background daemon. Parses the TOML, embeds the guest daemon binary into the container build context, writes a multi-stage Containerfile, and generates standard systemd Quadlet unit files.',
      specs: [
        'Pure function: TOML -> Containerfile + Quadlet units',
        'Zero memory overhead when idle; terminates upon build completion',
        'Supports prebuilt registry tags or custom multi-package baking',
      ],
      code: {
        lang: 'bash',
        filename: 'cli execution',
        snippet: `# Generate OCI image and ~/.config/containers/systemd/dev-rust.container
podbox build .

# Reload systemd generator
systemctl --user daemon-reload`,
      },
      command: 'podbox build --dry-run',
    },
    build_image: {
      title: 'OCI Image (localhost/podbox-*)',
      category: 'Immutable Build Artifact',
      description:
        'The OCI image built via rootless Podman. Base distribution packages are baked in at build time rather than reinstalled on container launch. The podbox guest agent is injected at /usr/local/bin/podbox-guest.',
      specs: [
        'Instant spin-up (sub-50ms) with pre-baked packages',
        'Independent of host package manager (Fedora, Arch, Ubuntu, Alpine)',
        'Tagged locally as localhost/podbox-<name>:latest',
      ],
      code: {
        lang: 'bash',
        filename: 'generated Containerfile snippet',
        snippet: `FROM registry.fedoraproject.org/fedora:41
RUN dnf install -y neovim cargo ripgrep wayland-devel && dnf clean all
COPY --from=podbox-builder /bin/podbox-guest /usr/local/bin/podbox-guest
USER user
WORKDIR /home/user`,
      },
      command: 'podman images | grep podbox',
    },
    build_quadlet: {
      title: 'systemd Quadlet Units (.container, .socket, .build)',
      category: 'systemd Integration',
      description:
        'Standard Quadlet files placed into ~/.config/containers/systemd/. systemd transforms these declarative unit files into first-class system services. Includes companion services for proxy lifecycle and socket activation.',
      specs: [
        'Automatic container start on user login',
        'Restart on failure policies managed natively by systemd',
        'Companion .service for xdg-dbus-proxy and Wayland forwarder',
      ],
      code: {
        lang: 'systemd',
        filename: '~/.config/containers/systemd/dev-rust.container',
        snippet: `[Unit]
Description=podbox dev-rust container
After=network-online.target

[Container]
Image=localhost/podbox-dev-rust:latest
ContainerName=podbox-dev-rust
Volume=podbox-dev-rust-home:/home/user:Z
Environment=WAYLAND_DISPLAY=wayland-0
Volume=/run/user/%U/wayland-0:/run/user/1000/wayland-0:ro
Device=/dev/dri

[Service]
Restart=on-failure
TimeoutStopSec=30`,
      },
      command: 'systemctl --user status podbox-dev-rust.service',
    },
    build_systemd: {
      title: 'systemd --user (Lifecycle Supervisor)',
      category: 'Supervisor & Process Manager',
      description:
        'The host systemd user manager supervises the entire container lifecycle. There is no podbox daemon running in background. Standard systemctl commands and journald logging apply out of the box.',
      specs: [
        'systemctl --user {start, stop, restart, status} dev-rust',
        'Streaming logs via journalctl --user -u podbox-dev-rust -f',
        'Socket activation triggers container spin-up on demand',
      ],
      code: {
        lang: 'bash',
        filename: 'systemd commands',
        snippet: `# View service state
systemctl --user status podbox-dev-rust.service

# View live structured container logs
journalctl --user -u podbox-dev-rust.service -f`,
      },
      command: 'systemctl --user status',
    },

    // Runtime nodes
    rt_guest: {
      title: 'podbox-guest (In Container Agent)',
      category: 'In-Container Multiplexer',
      description:
        'A lightweight Rust interceptor binary running inside the container. It captures desktop notifications (notify-send), clipboard requests (wl-copy/wl-paste), URL opening (xdg-open), and approved host executions.',
      specs: [
        'Minimal footprint (~2MB memory, asynchronous epoll/tokio)',
        'Translates internal D-Bus/XDG calls into JSON-RPC messages',
        'Communicates with host over an isolated UNIX domain socket',
      ],
      code: {
        lang: 'bash',
        filename: 'guest environment',
        snippet: `# Interceptor shims inside container
export WAYLAND_DISPLAY=wayland-0
export DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus
export PODBOX_SOCKET=/run/user/1000/podbox.sock`,
      },
      command: 'podbox enter dev-rust',
    },
    rt_handshake: {
      title: 'Hello Handshake & Capability Negotiation',
      category: 'IPC Protocol Boundary',
      description:
        'When the container starts, the guest daemon and host socket broker execute a structured handshake over the UNIX socket. The host verifies container identity and negotiates permitted capabilities.',
      specs: [
        'Strict capability negotiation: notifications, clipboard, host-exec',
        'Cryptographic UID/GID check on the UNIX socket peer credentials',
        'Rejects unconfigured or unauthorized capability requests',
      ],
      code: {
        lang: 'json',
        filename: 'Hello frame payload',
        snippet: `{
  "type": "hello",
  "client_version": "0.1.2",
  "capabilities": ["notifications", "clipboard", "xdg_open"]
}`,
      },
    },
    rt_wayland: {
      title: 'Wayland Proxy',
      category: 'Display Security Filter',
      description:
        'Proxies the Wayland compositor socket ($WAYLAND_DISPLAY) to the guest container. Filters dangerous registry globals (such as keyloggers, screen capturers, or input injection) while passing standard window rendering.',
      specs: [
        'Allows: wl_compositor, wl_shm, xdg_wm_base',
        'Blocks: wlr_export_dmabuf, zwp_input_method, privileged capture',
        'Native 120Hz/Vulkan/OpenGL GUI window performance',
      ],
      code: {
        lang: 'toml',
        filename: 'podbox.toml [gui]',
        snippet: `[gui]
wayland = true
audio = true
gpu = "auto"`,
      },
      command: 'podbox exec dev-rust -- gedit',
    },
    rt_socket: {
      title: 'Socket Host (Capability-gated Broker)',
      category: 'Host-side IPC Host',
      description:
        'The host-side broker listening on /run/user/<uid>/podbox-<name>.sock. Dispatches capability-gated requests like clipboard synchronization, file transfers, or whitelisted host commands.',
      specs: [
        'Runs in the host user session context',
        'Gated strictly by the permissions specified in podbox.toml',
        'Terminates cleanly when the container unit stops',
      ],
      code: {
        lang: 'rust',
        filename: 'protocol handler',
        snippet: `// Dispatches validated guest command
match request {
  GuestRequest::ClipboardCopy { data } => host_clipboard::set(data),
  GuestRequest::XdgOpen { url } => host_open::open_validated(url),
  GuestRequest::HostExec { cmd } => host_exec::run_whitelisted(cmd),
}`,
      },
    },
    rt_dbus: {
      title: 'D-Bus Proxy (xdg-dbus-proxy)',
      category: 'D-Bus Filtering Sandbox',
      description:
        'Instead of mounting your unfiltered host session bus, podbox spawns an xdg-dbus-proxy instance that only allows specific bus names and interfaces explicitly granted in your TOML config.',
      specs: [
        'Strict policy: default deny all bus access',
        'Granular rules: --talk=org.freedesktop.Notifications',
        'Prevents container escape through privileged D-Bus services',
      ],
      code: {
        lang: 'bash',
        filename: 'xdg-dbus-proxy invocation',
        snippet: `xdg-dbus-proxy /run/user/1000/bus /run/user/1000/podbox-dbus/bus \\
  --filter \\
  --talk=org.freedesktop.Notifications \\
  --call=org.freedesktop.portal.Desktop=org.freedesktop.portal.OpenURI.*`,
      },
    },
    rt_host: {
      title: 'Host Resources & Peripherals',
      category: 'Host Infrastructure',
      description:
        'Your host machine hardware and desktop environment: Wayland Compositor (GNOME/KDE/Sway), PipeWire audio engine, host GPU device nodes (/dev/dri/renderD128), and user session services.',
      specs: [
        'Direct GPU rendering with zero hypervisor virtualization overhead',
        'PipeWire low-latency native audio stream routing',
        'Host files remain invisible except for explicitly declared mounts',
      ],
      code: {
        lang: 'bash',
        filename: 'host peripherals',
        snippet: `# Verified device passthrough
ls -l /dev/dri/card* /dev/dri/renderD*
pactl info  # PipeWire PulseAudio compatibility`,
      },
    },

    // Protocol nodes
    proto_handshake: {
      title: '1. Handshake Phase (Hello <-> HelloAck)',
      category: 'Protocol Lifecycle',
      description:
        'Initial synchronous connection over the UNIX domain socket. Client announces identity, protocol version, and desired capabilities. Host validates against TOML constraints and responds with negotiated flags.',
      specs: [
        'Framed JSON-RPC over length-prefixed bytes',
        'Fails immediately if protocol versions or required capabilities mismatch',
      ],
      code: {
        lang: 'json',
        filename: 'Handshake Frames',
        snippet: `// Client -> Host
{ "type": "hello", "version": 1, "capabilities": ["notify", "clipboard"] }

// Host -> Client
{ "type": "hello_ack", "granted": ["notify", "clipboard"], "session_id": "px-88" }`,
      },
    },
    proto_channels: {
      title: '2. Multiplexed Channels',
      category: 'Data Transmission',
      description:
        'Multiple asynchronous streams share the single UNIX domain socket: Channel 0 (Control/Heartbeat), Channel 1 (XDG Portals), Channel 2 (Clipboard Sync), Channel 3 (Host Command Exec).',
      specs: [
        'Channel ID encoded in packet header',
        'Backpressure support for large clipboard transfers',
      ],
      code: {
        lang: 'json',
        filename: 'Event Frame',
        snippet: `{
  "channel": 1,
  "action": "notify",
  "summary": "Build Succeeded",
  "body": "Cargo finished in 4.2s"
}`,
      },
    },
};
