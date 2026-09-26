import {
  ArrowLeftRight,
  Boxes,
  Cpu,
  FileCode2,
  Monitor,
  Radio,
} from 'lucide-react';

export const VERSION = '0.7.2';

export const installTabs = [
  { id: 'curl', label: 'curl script' },
  { id: 'brew', label: 'Homebrew' },
  { id: 'aur', label: 'Arch (AUR)' },
  { id: 'cargo', label: 'cargo install' },
  { id: 'source', label: 'Source' },
] as const;

export type InstallTabId = (typeof installTabs)[number]['id'];

export const installCommands: Record<string, string> = {
  curl: `# Grab the binary & verify
curl -fsSL https://bethropolis.github.io/podbox/install.sh | sh

# Spin up a prebuilt Fedora container and hop in
podbox create fedora
podbox enter fedora`,
  brew: `# Homebrew (Linux only)
brew install bethropolis/homebrew-tap/podbox

# Spin up an environment
podbox create fedora
podbox enter fedora`,
  aur: `# Arch Linux, via AUR
paru -S podbox-bin

# Spin up an environment
podbox create fedora
podbox enter fedora`,
  cargo: `# From crates.io (supports prebuilt images only)
cargo install podbox-cli

# Spin up using a prebuilt GHCR image
podbox create ghcr.io/bethropolis/podbox:fedora
podbox enter fedora`,
  source: `# Source install (builds CLI & guest daemon)
git clone https://github.com/bethropolis/podbox
cd podbox && scripts/install.sh   # installs to ~/.local/bin

podbox create fedora
podbox enter fedora`,
};

export const features = [
  { icon: FileCode2, color: 'text-[var(--accent-mauve)]', hover: 'hover:border-[var(--accent-mauve)]/50', title: 'Declarative TOML Specification', body: 'Environment configuration as code you can commit to git. No long sequences of CLI flags you ran once and forgot.' },
  { icon: Cpu, color: 'text-[var(--accent-blue)]', hover: 'hover:border-[var(--accent-blue)]/50', title: 'systemd Quadlet Lifecycle', body: 'Autostart on login, automatic restart on failure, socket activation, and logging via `journalctl --user`. Zero podbox daemon.' },
  { icon: Monitor, color: 'text-[var(--accent-teal)]', hover: 'hover:border-[var(--accent-teal)]/50', title: 'Wayland, Audio & GPU Passthrough', body: 'Run GUI tools and games with hardware acceleration. Wayland socket, PipeWire audio, and `/dev/dri` or NVIDIA GPUs configured automatically.' },
  { icon: Radio, color: 'text-[var(--accent-green)]', hover: 'hover:border-[var(--accent-green)]/50', title: 'Filtered D-Bus via xdg-dbus-proxy', body: 'Containers never get unrestricted access to your host session bus. Specific interfaces like Notifications or MPRIS are selectively proxied.' },
  { icon: ArrowLeftRight, color: 'text-[var(--accent-peach)]', hover: 'hover:border-[var(--accent-peach)]/50', title: 'Guest Interceptor & Desktop Export', body: 'Export GUI applications (`podbox export app`) and CLI binaries (`podbox export bin`) directly into your host application launcher and PATH.' },
  { icon: Boxes, color: 'text-[var(--accent-red)]', hover: 'hover:border-[var(--accent-red)]/50', title: 'Baked-in Image Caching', body: 'Packages are installed during `podbox build` into the OCI layer, not on container start. Spin-up takes milliseconds, not minutes.' },
];

export const dayToDay = [
  { dot: 'bg-[var(--accent-mauve)]', title: "1. Build image & install Quadlet units", code: `# Build current directory's podbox.toml\npodbox build .\n\n# Or build from a specific config\npodbox build ~/configs/rust-dev.toml` },
  { dot: 'bg-[var(--accent-blue)]', title: '2. Enter the interactive container shell', code: `# Start container (if stopped) and enter shell\npodbox enter rust-dev\n\n# Or run a command directly inside without entering\npodbox exec rust-dev -- cargo check` },
  { dot: 'bg-[var(--accent-green)]', title: '3. Export GUI app to host launcher', code: `# Export desktop application (.desktop file)\npodbox export app rust-dev code\n\n# Export CLI tool to ~/.local/bin on host\npodbox export bin rust-dev rg` },
  { dot: 'bg-[var(--accent-peach)]', title: '4. Inspect systemd lifecycle directly', code: `# Check systemd user service status\nsystemctl --user status podbox-rust-dev.service\n\n# View container logs via journald\njournalctl --user -u podbox-rust-dev.service -f` },
];
