import React, { useState, useEffect } from 'react';
import { TerminalCodeBlock } from './TerminalCodeBlock';
import { withBase } from '../base';
import {
  Play,
  Pause,
  RotateCcw,
  Sparkles,
  Layers,
  Cpu,
  Shield,
  Radio,
  FileCode,
  Terminal,
  ChevronRight,
  Maximize2,
  ZoomOut,
  Info,
  CheckCircle2,
  Copy,
  ExternalLink,
  Sliders,
  Send,
  Bell,
  Clipboard,
  Globe,
  Flame
} from 'lucide-react';

interface HowItWorksSectionProps {
  // Navigation is plain MPA links; no callback props cross the Astro boundary.
}

type TabType = 'build' | 'runtime' | 'protocol';

export function HowItWorksSection(_props: HowItWorksSectionProps) {
  const [activeTab, setActiveTab] = useState<TabType>('build');
  const [selectedNode, setSelectedNode] = useState<string>('build_codegen');
  const [isSimulating, setIsSimulating] = useState(false);
  const [simStep, setSimStep] = useState(0);
  const [activePacket, setActivePacket] = useState<string | null>(null);
  const [actionLog, setActionLog] = useState<string[]>([
    'System ready. Select a node or trigger an interactive simulation.',
  ]);

  // Handle simulation timer
  useEffect(() => {
    let timer: any;
    if (isSimulating) {
      timer = setInterval(() => {
        setSimStep(prev => {
          const next = (prev + 1) % 4;
          if (activeTab === 'build') {
            const nodes = ['build_toml', 'build_codegen', 'build_image', 'build_systemd'];
            setSelectedNode(nodes[next]);
            const msgs = [
              'Parsing and validating podbox.toml declarative schema...',
              'Running pure Rust codegen: multi-stage Containerfile & Quadlet units...',
              'Building OCI image with baked packages & guest daemon...',
              'systemd --user reloads units and claims container lifecycle supervision.',
            ];
            setActionLog(l => [msgs[next], ...l.slice(0, 5)]);
          } else if (activeTab === 'runtime') {
            const nodes = ['rt_guest', 'rt_handshake', 'rt_proxies', 'rt_host'];
            setSelectedNode(nodes[next]);
            const msgs = [
              'User process inside container issues an intercepted request...',
              'podbox-guest negotiates capabilities across the UNIX domain socket...',
              'Proxies filter D-Bus methods, Wayland globals, and host-exec requests...',
              'Host session bus, GPU device (/dev/dri) & Wayland compositor handle payload.',
            ];
            setActionLog(l => [msgs[next], ...l.slice(0, 5)]);
          }
          return next;
        });
      }, 2200);
    }
    return () => clearInterval(timer);
  }, [isSimulating, activeTab]);

  // Set default selected node when switching tabs
  const handleTabChange = (tab: TabType) => {
    setActiveTab(tab);
    setIsSimulating(false);
    if (tab === 'build') setSelectedNode('build_codegen');
    else if (tab === 'runtime') setSelectedNode('rt_guest');
    else setSelectedNode('proto_handshake');
    setActionLog([`Switched to ${tab.toUpperCase()} architecture view.`]);
  };

  // Interactive triggers for Runtime
  const triggerRuntimeAction = (actionType: 'notification' | 'clipboard' | 'url' | 'exec') => {
    setActivePacket(actionType);
    if (actionType === 'notification') {
      setActionLog(prev => [
        '[EVENT] notify-send "Build completed" inside container',
        '-> Intercepted by podbox-guest daemon',
        '-> Forwarded across UNIX socket to D-Bus proxy',
        '-> xdg-dbus-proxy validates --talk=org.freedesktop.Notifications',
        '-> Dispatched to host notification daemon (mako/dunst)',
        ...prev.slice(0, 5),
      ]);
      setSelectedNode('rt_dbus');
    } else if (actionType === 'clipboard') {
      setActionLog(prev => [
        '[EVENT] wl-copy requested inside container',
        '-> Intercepted by podbox-guest clipboard helper',
        '-> Routed through socket host with capability token',
        '-> Synchronized to host Wayland clipboard selection',
        ...prev.slice(0, 5),
      ]);
      setSelectedNode('rt_socket');
    } else if (actionType === 'url') {
      setActionLog(prev => [
        '[EVENT] xdg-open "https://github.com/bethropolis/podbox"',
        '-> Interceptor catches invocation',
        '-> Handed to host-exec broker',
        '-> Host browser opens link outside container sandbox',
        ...prev.slice(0, 5),
      ]);
      setSelectedNode('rt_host');
    } else if (actionType === 'exec') {
      setActionLog(prev => [
        '[EVENT] podbox-host-exec "podman ps"',
        '-> Validates command whitelist in podbox.toml',
        '-> Socket host spawns target binary in host user session',
        ...prev.slice(0, 5),
      ]);
      setSelectedNode('rt_guest');
    }

    setTimeout(() => {
      setActivePacket(null);
    }, 2500);
  };

  // Node details dictionary
  const nodeDetails: Record<
    string,
    {
      title: string;
      category: string;
      description: string;
      specs: string[];
      code?: { lang: string; filename: string; snippet: string };
      command?: string;
    }
  > = {
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

  const currentNode = nodeDetails[selectedNode] || nodeDetails.build_codegen;

  return (
    <section id="architecture" className="w-full my-12 font-mono">
      {/* Section Header */}
      <div className="mb-8">
        <div className="flex items-center gap-2 text-xs text-[var(--accent-blue)] uppercase tracking-wider font-bold mb-2">
          <Cpu className="w-4 h-4" />
          <span>internals // interactive-architecture-visualizer</span>
        </div>
        <div className="flex flex-col md:flex-row md:items-end justify-between gap-4">
          <div>
            <h2 className="text-2xl sm:text-3xl font-bold tracking-tight text-[var(--text-primary)]">
              How podbox works: Build-time & Runtime
            </h2>
            <p className="mt-2 text-sm text-[var(--text-subtext)] max-w-2xl leading-relaxed">
              Explore the lifecycle stages below. Click any node to inspect its internal mechanics,
              or run the interactive simulation to trace data packets as they cross security boundaries.
            </p>
          </div>

          {/* Action Simulation Controls */}
          <div className="flex items-center gap-2 bg-[var(--bg-mantle)] p-1.5 rounded-[4px] border border-[var(--border)] shrink-0">
            <button
              onClick={() => setIsSimulating(!isSimulating)}
              type="button"
              className={`flex items-center gap-1.5 px-3 py-1.5 text-xs font-mono rounded-[2px] transition-colors cursor-pointer font-bold ${
                isSimulating
                  ? 'bg-[var(--accent-red)] text-[#11111b]'
                  : 'bg-[var(--accent-green)] text-[#11111b]'
              }`}
            >
              {isSimulating ? (
                <>
                  <Pause className="w-3.5 h-3.5" />
                  <span>Pause Simulation</span>
                </>
              ) : (
                <>
                  <Play className="w-3.5 h-3.5" />
                  <span>Trace Pipeline</span>
                </>
              )}
            </button>

            <button
              onClick={() => {
                setIsSimulating(false);
                setSimStep(0);
                setActionLog(['Simulation reset.']);
              }}
              type="button"
              className="p-1.5 text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-surface0)] rounded-[2px] cursor-pointer"
              title="Reset simulation"
            >
              <RotateCcw className="w-3.5 h-3.5" />
            </button>
          </div>
        </div>
      </div>

      {/* Main Visualizer Container */}
      <div className="rounded-[4px] border border-[var(--border)] bg-[var(--bg-mantle)] overflow-hidden shadow-lg">
        {/* Terminal Chrome Bar with View Switchers */}
        <div className="flex flex-wrap items-center justify-between px-4 py-2.5 bg-[var(--bg-crust)] border-b border-[var(--border)] gap-2">
          <div className="flex items-center gap-3">
            <div className="flex items-center gap-1.5">
              <span className="w-2.5 h-2.5 rounded-full bg-[#f38ba8]/80 inline-block" />
              <span className="w-2.5 h-2.5 rounded-full bg-[#f9e2af]/80 inline-block" />
              <span className="w-2.5 h-2.5 rounded-full bg-[#a6e3a1]/80 inline-block" />
            </div>
            <span className="text-xs text-[var(--text-subtext)] font-semibold uppercase tracking-wider">
              arch-pipeline // {activeTab}.svg
            </span>
          </div>

          {/* View Tab Buttons */}
          <div className="inline-flex rounded-[2px] p-0.5 bg-[var(--bg-base)] border border-[var(--border)] text-xs">
            <button
              onClick={() => handleTabChange('build')}
              type="button"
              className={`flex items-center gap-1.5 px-3 py-1 rounded-[2px] transition-colors cursor-pointer ${
                activeTab === 'build'
                  ? 'bg-[var(--accent-mauve)] text-[#11111b] font-bold'
                  : 'text-[var(--text-subtext)] hover:text-[var(--text-primary)]'
              }`}
            >
              <FileCode className="w-3.5 h-3.5" />
              <span>Phase 1: Build-Time</span>
            </button>
            <button
              onClick={() => handleTabChange('runtime')}
              type="button"
              className={`flex items-center gap-1.5 px-3 py-1 rounded-[2px] transition-colors cursor-pointer ${
                activeTab === 'runtime'
                  ? 'bg-[var(--accent-blue)] text-[#11111b] font-bold'
                  : 'text-[var(--text-subtext)] hover:text-[var(--text-primary)]'
              }`}
            >
              <Shield className="w-3.5 h-3.5" />
              <span>Phase 2: Runtime IPC</span>
            </button>
            <button
              onClick={() => handleTabChange('protocol')}
              type="button"
              className={`flex items-center gap-1.5 px-3 py-1 rounded-[2px] transition-colors cursor-pointer ${
                activeTab === 'protocol'
                  ? 'bg-[var(--accent-peach)] text-[#11111b] font-bold'
                  : 'text-[var(--text-subtext)] hover:text-[var(--text-primary)]'
              }`}
            >
              <Radio className="w-3.5 h-3.5" />
              <span>Socket Protocol</span>
            </button>
          </div>
        </div>

        {/* Interactive Event Dispatch Bar for Runtime Mode */}
        {activeTab === 'runtime' && (
          <div className="flex flex-wrap items-center justify-between px-4 py-2 bg-[var(--bg-base)] border-b border-[var(--border)] text-xs gap-2">
            <div className="flex items-center gap-2 text-[var(--accent-mauve)] font-medium">
              <Sparkles className="w-3.5 h-3.5" />
              <span>Trigger In-Container Event:</span>
            </div>
            <div className="flex flex-wrap items-center gap-1.5">
              <button
                onClick={() => triggerRuntimeAction('notification')}
                type="button"
                className="flex items-center gap-1 px-2.5 py-1 rounded-[2px] bg-[var(--bg-mantle)] hover:bg-[var(--bg-surface0)] border border-[var(--border)] text-[var(--accent-yellow)] cursor-pointer"
              >
                <Bell className="w-3 h-3" />
                <span>notify-send</span>
              </button>
              <button
                onClick={() => triggerRuntimeAction('clipboard')}
                type="button"
                className="flex items-center gap-1 px-2.5 py-1 rounded-[2px] bg-[var(--bg-mantle)] hover:bg-[var(--bg-surface0)] border border-[var(--border)] text-[var(--accent-green)] cursor-pointer"
              >
                <Clipboard className="w-3 h-3" />
                <span>wl-copy</span>
              </button>
              <button
                onClick={() => triggerRuntimeAction('url')}
                type="button"
                className="flex items-center gap-1 px-2.5 py-1 rounded-[2px] bg-[var(--bg-mantle)] hover:bg-[var(--bg-surface0)] border border-[var(--border)] text-[var(--accent-blue)] cursor-pointer"
              >
                <Globe className="w-3 h-3" />
                <span>xdg-open</span>
              </button>
              <button
                onClick={() => triggerRuntimeAction('exec')}
                type="button"
                className="flex items-center gap-1 px-2.5 py-1 rounded-[2px] bg-[var(--bg-mantle)] hover:bg-[var(--bg-surface0)] border border-[var(--border)] text-[var(--accent-peach)] cursor-pointer"
              >
                <Terminal className="w-3 h-3" />
                <span>host-exec</span>
              </button>
            </div>
          </div>
        )}

        {/* SVG INTERACTIVE CANVAS */}
        <div className="p-4 sm:p-6 bg-[var(--bg-base)] flex items-center justify-center relative overflow-x-auto min-h-[340px]">
          {/* Subtle dot background */}
          <div className="absolute inset-0 bg-terminal-dots pointer-events-none" />

          {/* TAB 1: BUILD TIME PIPELINE */}
          {activeTab === 'build' && (
            <svg
              viewBox="0 0 850 310"
              className="w-full max-w-[850px] h-auto select-none overflow-visible"
            >
              <defs>
                <linearGradient id="ib-grad1" x1="0%" y1="0%" x2="100%" y2="0%">
                  <stop offset="0%" stopColor="#3b82f6" />
                  <stop offset="100%" stopColor="#6366f1" />
                </linearGradient>
                <linearGradient id="ib-grad2" x1="0%" y1="0%" x2="100%" y2="0%">
                  <stop offset="0%" stopColor="#8b5cf6" />
                  <stop offset="100%" stopColor="#d946ef" />
                </linearGradient>
                <linearGradient id="ib-grad3" x1="0%" y1="0%" x2="100%" y2="0%">
                  <stop offset="0%" stopColor="#6366f1" />
                  <stop offset="100%" stopColor="#a855f7" />
                </linearGradient>
                <marker
                  id="ib-arrow"
                  viewBox="0 0 10 10"
                  refX="6"
                  refY="5"
                  markerWidth="6"
                  markerHeight="6"
                  orient="auto-start-reverse"
                >
                  <path d="M 0 1.5 L 8 5 L 0 8.5 z" fill="var(--text-muted)" />
                </marker>
              </defs>

              {/* Connecting Lines */}
              <path
                d="M 160 135 L 210 135"
                stroke="var(--border)"
                strokeWidth="2.5"
                markerEnd="url(#ib-arrow)"
              />
              <path
                d="M 400 135 C 420 135, 420 55, 440 55"
                stroke="var(--border)"
                strokeWidth="2.5"
                markerEnd="url(#ib-arrow)"
              />
              <path
                d="M 400 135 C 420 135, 420 220, 440 220"
                stroke="var(--border)"
                strokeWidth="2.5"
                markerEnd="url(#ib-arrow)"
              />
              <path
                d="M 600 55 C 630 55, 630 135, 660 135"
                stroke="var(--border)"
                strokeWidth="2.5"
                markerEnd="url(#ib-arrow)"
              />
              <path
                d="M 630 220 C 650 220, 650 135, 660 135"
                stroke="var(--border)"
                strokeWidth="2.5"
                markerEnd="url(#ib-arrow)"
              />

              {/* Animated pulse packet when simulating */}
              {isSimulating && (
                <circle r="5" fill="#cba6f7">
                  <animateMotion
                    path="M 160 135 L 210 135 M 400 135 C 420 135, 420 55, 440 55 M 600 55 C 630 55, 630 135, 660 135"
                    dur="3s"
                    repeatCount="indefinite"
                  />
                </circle>
              )}

              {/* Node 1: definition.toml */}
              <g
                transform="translate(20, 95)"
                onClick={() => setSelectedNode('build_toml')}
                className="cursor-pointer group"
              >
                <rect
                  width="140"
                  height="80"
                  rx="6"
                  fill="var(--bg-mantle)"
                  stroke={selectedNode === 'build_toml' ? 'var(--accent-mauve)' : 'var(--border)'}
                  strokeWidth={selectedNode === 'build_toml' ? '2.5' : '1.5'}
                  className="transition-all group-hover:stroke-[var(--accent-mauve)]"
                />
                <text
                  x="70"
                  y="38"
                  textAnchor="middle"
                  fill="var(--text-primary)"
                  fontSize="13"
                  fontWeight="700"
                  fontFamily="monospace"
                >
                  definition.toml
                </text>
                <text
                  x="70"
                  y="58"
                  textAnchor="middle"
                  fill="var(--text-muted)"
                  fontSize="11"
                  fontFamily="monospace"
                >
                  Declarative config
                </text>
              </g>

              {/* Node 2: podbox build / enable */}
              <g
                transform="translate(210, 95)"
                onClick={() => setSelectedNode('build_codegen')}
                className="cursor-pointer group"
              >
                <rect
                  width="190"
                  height="80"
                  rx="6"
                  fill="url(#ib-grad1)"
                  stroke={selectedNode === 'build_codegen' ? '#ffffff' : '#334155'}
                  strokeWidth={selectedNode === 'build_codegen' ? '2.5' : '1'}
                  className="transition-all group-hover:brightness-110"
                />
                <text
                  x="95"
                  y="38"
                  textAnchor="middle"
                  fill="#ffffff"
                  fontSize="13"
                  fontWeight="700"
                  fontFamily="monospace"
                >
                  podbox build
                </text>
                <text
                  x="95"
                  y="58"
                  textAnchor="middle"
                  fill="#e2e8f0"
                  fontSize="11"
                  fontFamily="monospace"
                >
                  Pure codegen, no daemon
                </text>
              </g>

              {/* Node 3a: OCI Image */}
              <g
                transform="translate(440, 15)"
                onClick={() => setSelectedNode('build_image')}
                className="cursor-pointer group"
              >
                <rect
                  width="160"
                  height="80"
                  rx="6"
                  fill="url(#ib-grad2)"
                  stroke={selectedNode === 'build_image' ? '#ffffff' : '#334155'}
                  strokeWidth={selectedNode === 'build_image' ? '2.5' : '1'}
                  className="transition-all group-hover:brightness-110"
                />
                <text
                  x="80"
                  y="38"
                  textAnchor="middle"
                  fill="#ffffff"
                  fontSize="13"
                  fontWeight="700"
                  fontFamily="monospace"
                >
                  OCI Image
                </text>
                <text
                  x="80"
                  y="58"
                  textAnchor="middle"
                  fill="#e2e8f0"
                  fontSize="11"
                  fontFamily="monospace"
                >
                  Packages baked in
                </text>
              </g>

              {/* Node 3b: Quadlet units */}
              <g
                transform="translate(440, 175)"
                onClick={() => setSelectedNode('build_quadlet')}
                className="cursor-pointer group"
              >
                <rect
                  width="190"
                  height="90"
                  rx="6"
                  fill="url(#ib-grad3)"
                  stroke={selectedNode === 'build_quadlet' ? '#ffffff' : '#334155'}
                  strokeWidth={selectedNode === 'build_quadlet' ? '2.5' : '1'}
                  className="transition-all group-hover:brightness-110"
                />
                <text
                  x="95"
                  y="34"
                  textAnchor="middle"
                  fill="#ffffff"
                  fontSize="13"
                  fontWeight="700"
                  fontFamily="monospace"
                >
                  Quadlet units
                </text>
                <text
                  x="95"
                  y="54"
                  textAnchor="middle"
                  fill="#e2e8f0"
                  fontSize="11"
                  fontFamily="monospace"
                >
                  .container .socket .build
                </text>
                <text
                  x="95"
                  y="70"
                  textAnchor="middle"
                  fill="#cbd5e1"
                  fontSize="10"
                  fontFamily="monospace"
                >
                  + companion services
                </text>
              </g>

              {/* Node 4: systemd --user */}
              <g
                transform="translate(660, 95)"
                onClick={() => setSelectedNode('build_systemd')}
                className="cursor-pointer group"
              >
                <rect
                  width="150"
                  height="80"
                  rx="6"
                  fill="var(--bg-mantle)"
                  stroke={selectedNode === 'build_systemd' ? 'var(--accent-blue)' : 'var(--border)'}
                  strokeWidth={selectedNode === 'build_systemd' ? '2.5' : '1.5'}
                  className="transition-all group-hover:stroke-[var(--accent-blue)]"
                />
                <text
                  x="75"
                  y="38"
                  textAnchor="middle"
                  fill="var(--text-primary)"
                  fontSize="13"
                  fontWeight="700"
                  fontFamily="monospace"
                >
                  systemd --user
                </text>
                <text
                  x="75"
                  y="58"
                  textAnchor="middle"
                  fill="var(--text-muted)"
                  fontSize="11"
                  fontFamily="monospace"
                >
                  Owns the lifecycle
                </text>
              </g>
            </svg>
          )}

          {/* TAB 2: RUNTIME IPC & PROXIES */}
          {activeTab === 'runtime' && (
            <svg
              viewBox="0 0 850 440"
              className="w-full max-w-[850px] h-auto select-none overflow-visible"
            >
              <defs>
                <linearGradient id="rt-grad1" x1="0%" y1="0%" x2="100%" y2="0%">
                  <stop offset="0%" stopColor="#3b82f6" />
                  <stop offset="100%" stopColor="#6366f1" />
                </linearGradient>
                <linearGradient id="rt-grad3" x1="0%" y1="0%" x2="100%" y2="0%">
                  <stop offset="0%" stopColor="#6366f1" />
                  <stop offset="100%" stopColor="#a855f7" />
                </linearGradient>
                <marker
                  id="rt-arrow"
                  viewBox="0 0 10 10"
                  refX="6"
                  refY="5"
                  markerWidth="6"
                  markerHeight="6"
                  orient="auto-start-reverse"
                >
                  <path d="M 0 1.5 L 8 5 L 0 8.5 z" fill="var(--text-muted)" />
                </marker>
              </defs>

              {/* Top: Guest Daemon in Container */}
              <g
                transform="translate(265, 20)"
                onClick={() => setSelectedNode('rt_guest')}
                className="cursor-pointer group"
              >
                <rect
                  width="320"
                  height="78"
                  rx="6"
                  fill="url(#rt-grad1)"
                  stroke={selectedNode === 'rt_guest' ? '#ffffff' : '#334155'}
                  strokeWidth={selectedNode === 'rt_guest' ? '2.5' : '1'}
                  className="transition-all group-hover:brightness-110"
                />
                <text
                  x="160"
                  y="34"
                  textAnchor="middle"
                  fill="#ffffff"
                  fontSize="13"
                  fontWeight="700"
                  fontFamily="monospace"
                >
                  podbox-guest (in container)
                </text>
                <text
                  x="160"
                  y="56"
                  textAnchor="middle"
                  fill="#e2e8f0"
                  fontSize="11"
                  fontFamily="monospace"
                >
                  notify · clipboard · xdg-open · host-exec
                </text>
              </g>

              {/* Handshake line */}
              <path
                d="M 425 98 L 425 198"
                stroke="var(--accent-mauve)"
                strokeWidth="2.5"
                markerEnd="url(#rt-arrow)"
                className="cursor-pointer"
                onClick={() => setSelectedNode('rt_handshake')}
              />
              <text
                x="440"
                y="148"
                fill="var(--accent-mauve)"
                fontSize="10"
                fontFamily="monospace"
                fontWeight="600"
              >
                Hello ↔ capability negotiation
              </text>

              {/* Dashed proxy connections */}
              <path
                d="M 360 98 C 360 150, 220 150, 220 198"
                stroke="var(--border)"
                strokeWidth="2"
                strokeDasharray="4,4"
                markerEnd="url(#rt-arrow)"
              />
              <path
                d="M 490 98 C 490 150, 630 150, 630 198"
                stroke="var(--border)"
                strokeWidth="2"
                strokeDasharray="4,4"
                markerEnd="url(#rt-arrow)"
              />

              {/* Dynamic animated packet */}
              {activePacket && (
                <circle r="6" fill="#a6e3a1">
                  <animateMotion
                    path={
                      activePacket === 'notification'
                        ? 'M 490 98 C 490 150, 630 150, 630 198 L 630 280 C 630 310, 510 310, 510 328'
                        : activePacket === 'clipboard'
                        ? 'M 425 98 L 425 198 L 425 328'
                        : 'M 360 98 C 360 150, 220 150, 220 198 L 220 280 C 220 310, 340 310, 340 328'
                    }
                    dur="1.2s"
                    repeatCount="1"
                  />
                </circle>
              )}

              {/* HOST SECURITY BOUNDARY line */}
              <line
                x1="30"
                y1="150"
                x2="820"
                y2="150"
                stroke="var(--border)"
                strokeWidth="1.5"
                strokeDasharray="6,6"
              />
              <text
                x="40"
                y="142"
                fill="var(--text-muted)"
                fontSize="10"
                fontWeight="700"
                fontFamily="monospace"
                letterSpacing="1"
              >
                HOST SECURITY BOUNDARY (RESTRICTED ACCESS)
              </text>

              {/* Wayland Proxy */}
              <g
                transform="translate(130, 200)"
                onClick={() => setSelectedNode('rt_wayland')}
                className="cursor-pointer group"
              >
                <rect
                  width="180"
                  height="80"
                  rx="6"
                  fill="url(#rt-grad3)"
                  stroke={selectedNode === 'rt_wayland' ? '#ffffff' : '#334155'}
                  strokeWidth={selectedNode === 'rt_wayland' ? '2.5' : '1'}
                  className="transition-all group-hover:brightness-110"
                />
                <text
                  x="90"
                  y="34"
                  textAnchor="middle"
                  fill="#ffffff"
                  fontSize="13"
                  fontWeight="700"
                  fontFamily="monospace"
                >
                  Wayland proxy
                </text>
                <text
                  x="90"
                  y="54"
                  textAnchor="middle"
                  fill="#e2e8f0"
                  fontSize="11"
                  fontFamily="monospace"
                >
                  Filters registry globals
                </text>
              </g>

              {/* Socket Host */}
              <g
                transform="translate(335, 200)"
                onClick={() => setSelectedNode('rt_socket')}
                className="cursor-pointer group"
              >
                <rect
                  width="180"
                  height="80"
                  rx="6"
                  fill="url(#rt-grad3)"
                  stroke={selectedNode === 'rt_socket' ? '#ffffff' : '#334155'}
                  strokeWidth={selectedNode === 'rt_socket' ? '2.5' : '1'}
                  className="transition-all group-hover:brightness-110"
                />
                <text
                  x="90"
                  y="34"
                  textAnchor="middle"
                  fill="#ffffff"
                  fontSize="13"
                  fontWeight="700"
                  fontFamily="monospace"
                >
                  Socket host
                </text>
                <text
                  x="90"
                  y="54"
                  textAnchor="middle"
                  fill="#e2e8f0"
                  fontSize="11"
                  fontFamily="monospace"
                >
                  Capability-gated handshake
                </text>
              </g>

              {/* D-Bus Proxy */}
              <g
                transform="translate(540, 200)"
                onClick={() => setSelectedNode('rt_dbus')}
                className="cursor-pointer group"
              >
                <rect
                  width="180"
                  height="80"
                  rx="6"
                  fill="url(#rt-grad3)"
                  stroke={selectedNode === 'rt_dbus' ? '#ffffff' : '#334155'}
                  strokeWidth={selectedNode === 'rt_dbus' ? '2.5' : '1'}
                  className="transition-all group-hover:brightness-110"
                />
                <text
                  x="90"
                  y="34"
                  textAnchor="middle"
                  fill="#ffffff"
                  fontSize="13"
                  fontWeight="700"
                  fontFamily="monospace"
                >
                  D-Bus proxy
                </text>
                <text
                  x="90"
                  y="54"
                  textAnchor="middle"
                  fill="#e2e8f0"
                  fontSize="11"
                  fontFamily="monospace"
                >
                  Scoped portal call rules
                </text>
              </g>

              {/* Fan-in to Host resources */}
              <path
                d="M 220 280 C 220 310, 340 310, 340 328"
                stroke="var(--border)"
                strokeWidth="2"
                strokeDasharray="4,4"
                markerEnd="url(#rt-arrow)"
              />
              <path
                d="M 425 280 L 425 328"
                stroke="var(--border)"
                strokeWidth="2"
                strokeDasharray="4,4"
                markerEnd="url(#rt-arrow)"
              />
              <path
                d="M 630 280 C 630 310, 510 310, 510 328"
                stroke="var(--border)"
                strokeWidth="2"
                strokeDasharray="4,4"
                markerEnd="url(#rt-arrow)"
              />

              {/* Host Resources Bottom Box */}
              <g
                transform="translate(265, 330)"
                onClick={() => setSelectedNode('rt_host')}
                className="cursor-pointer group"
              >
                <rect
                  width="320"
                  height="80"
                  rx="6"
                  fill="var(--bg-mantle)"
                  stroke={selectedNode === 'rt_host' ? 'var(--accent-teal)' : 'var(--border)'}
                  strokeWidth={selectedNode === 'rt_host' ? '2.5' : '1.5'}
                  className="transition-all group-hover:stroke-[var(--accent-teal)]"
                />
                <text
                  x="160"
                  y="34"
                  textAnchor="middle"
                  fill="var(--text-primary)"
                  fontSize="13"
                  fontWeight="700"
                  fontFamily="monospace"
                >
                  Host resources
                </text>
                <text
                  x="160"
                  y="54"
                  textAnchor="middle"
                  fill="var(--text-muted)"
                  fontSize="11"
                  fontFamily="monospace"
                >
                  Compositor · session bus · GPU · PipeWire · XDG
                </text>
              </g>
            </svg>
          )}

          {/* TAB 3: SOCKET PROTOCOL SPEC */}
          {activeTab === 'protocol' && (
            <svg
              viewBox="0 0 850 320"
              className="w-full max-w-[850px] h-auto select-none overflow-visible"
            >
              <defs>
                <linearGradient id="pr-grad" x1="0%" y1="0%" x2="100%" y2="0%">
                  <stop offset="0%" stopColor="#fab387" />
                  <stop offset="100%" stopColor="#f38ba8" />
                </linearGradient>
              </defs>

              {/* Client Box */}
              <g
                transform="translate(40, 60)"
                onClick={() => setSelectedNode('proto_handshake')}
                className="cursor-pointer group"
              >
                <rect
                  width="220"
                  height="200"
                  rx="6"
                  fill="var(--bg-mantle)"
                  stroke="var(--accent-mauve)"
                  strokeWidth="2"
                />
                <text
                  x="110"
                  y="35"
                  textAnchor="middle"
                  fill="var(--accent-mauve)"
                  fontSize="14"
                  fontWeight="700"
                  fontFamily="monospace"
                >
                  GUEST CLIENT
                </text>
                <text
                  x="110"
                  y="60"
                  textAnchor="middle"
                  fill="var(--text-muted)"
                  fontSize="11"
                  fontFamily="monospace"
                >
                  /usr/local/bin/podbox-guest
                </text>

                <rect
                  x="20"
                  y="85"
                  width="180"
                  height="35"
                  rx="3"
                  fill="var(--bg-base)"
                  stroke="var(--border)"
                />
                <text
                  x="110"
                  y="107"
                  textAnchor="middle"
                  fill="var(--text-primary)"
                  fontSize="11"
                  fontFamily="monospace"
                >
                  1. Client Hello (JSON)
                </text>

                <rect
                  x="20"
                  y="135"
                  width="180"
                  height="35"
                  rx="3"
                  fill="var(--bg-base)"
                  stroke="var(--border)"
                />
                <text
                  x="110"
                  y="157"
                  textAnchor="middle"
                  fill="var(--text-primary)"
                  fontSize="11"
                  fontFamily="monospace"
                >
                  3. Stream Request Frames
                </text>
              </g>

              {/* Host Box */}
              <g
                transform="translate(590, 60)"
                onClick={() => setSelectedNode('proto_channels')}
                className="cursor-pointer group"
              >
                <rect
                  width="220"
                  height="200"
                  rx="6"
                  fill="var(--bg-mantle)"
                  stroke="var(--accent-blue)"
                  strokeWidth="2"
                />
                <text
                  x="110"
                  y="35"
                  textAnchor="middle"
                  fill="var(--accent-blue)"
                  fontSize="14"
                  fontWeight="700"
                  fontFamily="monospace"
                >
                  HOST BROKER
                </text>
                <text
                  x="110"
                  y="60"
                  textAnchor="middle"
                  fill="var(--text-muted)"
                  fontSize="11"
                  fontFamily="monospace"
                >
                  /run/user/1000/podbox.sock
                </text>

                <rect
                  x="20"
                  y="85"
                  width="180"
                  height="35"
                  rx="3"
                  fill="var(--bg-base)"
                  stroke="var(--border)"
                />
                <text
                  x="110"
                  y="107"
                  textAnchor="middle"
                  fill="var(--text-primary)"
                  fontSize="11"
                  fontFamily="monospace"
                >
                  2. Verify &amp; HelloAck
                </text>

                <rect
                  x="20"
                  y="135"
                  width="180"
                  height="35"
                  rx="3"
                  fill="var(--bg-base)"
                  stroke="var(--border)"
                />
                <text
                  x="110"
                  y="157"
                  textAnchor="middle"
                  fill="var(--text-primary)"
                  fontSize="11"
                  fontFamily="monospace"
                >
                  4. Dispatch to Session
                </text>
              </g>

              {/* Middle Protocol channel */}
              <g transform="translate(280, 100)">
                <path
                  d="M 0 50 L 290 50"
                  stroke="var(--accent-green)"
                  strokeWidth="3"
                  strokeDasharray="6,4"
                />
                <rect
                  x="55"
                  y="30"
                  width="180"
                  height="40"
                  rx="4"
                  fill="var(--bg-crust)"
                  stroke="var(--border)"
                />
                <text
                  x="145"
                  y="55"
                  textAnchor="middle"
                  fill="var(--accent-green)"
                  fontSize="11"
                  fontWeight="700"
                  fontFamily="monospace"
                >
                  UNIX DOMAIN SOCKET
                </text>
              </g>
            </svg>
          )}
        </div>

        {/* INTERACTIVE INSPECTOR PANEL */}
        <div className="grid grid-cols-1 lg:grid-cols-12 border-t border-[var(--border)] divide-y lg:divide-y-0 lg:divide-x divide-[var(--border)] bg-[var(--bg-mantle)]">
          {/* Left Column: Node Details & Capabilities */}
          <div className="lg:col-span-6 p-4 sm:p-5 space-y-4">
            <div className="flex items-center justify-between">
              <span className="text-[10px] uppercase font-bold px-2 py-0.5 rounded-[2px] bg-[var(--bg-surface0)] text-[var(--accent-mauve)] border border-[var(--border)]">
                {currentNode.category}
              </span>
              <span className="text-xs text-[var(--text-muted)] flex items-center gap-1">
                <CheckCircle2 className="w-3.5 h-3.5 text-[var(--accent-green)]" />
                <span>Verified Spec</span>
              </span>
            </div>

            <div>
              <h3 className="text-base font-bold text-[var(--text-primary)]">
                {currentNode.title}
              </h3>
              <p className="mt-1.5 text-xs text-[var(--text-subtext)] leading-relaxed">
                {currentNode.description}
              </p>
            </div>

            <div className="space-y-1.5 pt-1">
              <div className="text-[11px] font-bold text-[var(--text-muted)] uppercase tracking-wider">
                Key Architecture Guarantees
              </div>
              <ul className="space-y-1 text-xs text-[var(--text-subtext)]">
                {currentNode.specs.map((spec, sIdx) => (
                  <li key={sIdx} className="flex items-start gap-2">
                    <span className="text-[var(--accent-mauve)] font-bold">›</span>
                    <span>{spec}</span>
                  </li>
                ))}
              </ul>
            </div>

            {/* Quick CLI trigger or doc jump */}
            <div className="pt-2">
              <a
                href={withBase('/docs/architecture')}
                className="text-xs text-[var(--accent-blue)] hover:underline inline-flex items-center gap-1 cursor-pointer font-medium"
              >
                <span>Explore full architecture documentation</span>
                <ChevronRight className="w-3.5 h-3.5" />
              </a>
            </div>
          </div>

          {/* Right Column: Code Preview & Live Event Stream */}
          <div className="lg:col-span-6 flex flex-col bg-[var(--bg-base)]">
            <div className="flex items-center justify-between px-3 py-2 border-b border-[var(--border)] bg-[var(--bg-crust)]/60 text-xs">
              <span className="text-[var(--text-muted)] font-medium">
                {currentNode.code?.filename || 'Live Event Stream'}
              </span>
              <span className="text-[10px] text-[var(--accent-green)] flex items-center gap-1">
                <span className="w-2 h-2 rounded-full bg-[var(--accent-green)] animate-pulse" />
                <span>active</span>
              </span>
            </div>

            <div className="p-3 flex-1 overflow-y-auto">
              {currentNode.code ? (
                <TerminalCodeBlock
                  code={currentNode.code.snippet}
                  language={currentNode.code.lang}
                  filename={currentNode.code.filename}
                />
              ) : null}

              {/* Event / Action Log Output */}
              <div className="mt-2 p-2.5 rounded-[2px] bg-[var(--bg-mantle)] border border-[var(--border)] text-[11px] space-y-1 font-mono">
                <div className="text-[10px] text-[var(--text-muted)] uppercase tracking-widest font-bold pb-1 border-b border-[var(--border)]/50">
                  Event Stream &amp; Activity Log
                </div>
                {actionLog.map((log, lIdx) => (
                  <div
                    key={lIdx}
                    className={`leading-relaxed truncate ${
                      lIdx === 0
                        ? 'text-[var(--accent-mauve)] font-semibold'
                        : 'text-[var(--text-muted)]'
                    }`}
                  >
                    {log}
                  </div>
                ))}
              </div>
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}
