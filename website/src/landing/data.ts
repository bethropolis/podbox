import {
  ArrowLeftRight,
  Boxes,
  Cpu,
  FileCode2,
  Monitor,
  Radio,
} from 'lucide-react';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

// Single source of truth: workspace version in ../Cargo.toml, read at
// build time (this module is only imported by .astro files, never client
// bundles). Anchored at the process working directory (website/) because
// import.meta.url shifts when Vite bundles server code for production.
// Falls back to 'dev' when unreadable.
function loadVersion(): string {
  try {
    const cargo = readFileSync(join(process.cwd(), '..', 'Cargo.toml'), 'utf8');
    return cargo.match(/^version\s*=\s*"([^"]+)"/m)?.[1] ?? 'dev';
  } catch {
    return 'dev';
  }
}

export const VERSION = loadVersion();

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
curl -fsSL https://bethropolis.github.io/podbox/install.sh | sh`,
  brew: `# Homebrew (Linux only)
brew install bethropolis/homebrew-tap/podbox`,
  aur: `# Arch Linux, via AUR
paru -S podbox-bin`,
  cargo: `# From crates.io (supports prebuilt images only)
cargo install podbox-cli`,
  source: `# Source install (builds CLI & guest daemon)
git clone https://github.com/bethropolis/podbox
cd podbox && scripts/install.sh   # installs to ~/.local/bin`,
};

export const firstRunCommand = `# Spin up a prebuilt Fedora container and hop in
podbox create fedora
podbox enter fedora`;

export const cargoNote = `Note: the crates.io build supports prebuilt images only (image_ref in your config, or podbox create ghcr.io/bethropolis/podbox:<tag>). Custom image builds need a full source build from the workspace.`;

export const features = [
  { icon: FileCode2, color: 'text-[var(--accent-mauve)]', hover: 'hover:border-[var(--accent-mauve)]/50', title: 'Declarative TOML Specification', body: 'One TOML file per environment: image, packages, mounts, runtime. Commit it to git, never retype flags.' },
  { icon: Cpu, color: 'text-[var(--accent-blue)]', hover: 'hover:border-[var(--accent-blue)]/50', title: 'systemd Quadlet Lifecycle', body: 'systemd owns the lifecycle: autostart, restarts, socket activation, journald logs. No podbox daemon ever runs.' },
  { icon: Monitor, color: 'text-[var(--accent-teal)]', hover: 'hover:border-[var(--accent-teal)]/50', title: 'Wayland, Audio & GPU Passthrough', body: 'Run GUI apps and games with GPU power. Wayland, sound, and graphics work out of the box.' },
  { icon: Radio, color: 'text-[var(--accent-green)]', hover: 'hover:border-[var(--accent-green)]/50', title: 'Filtered D-Bus via xdg-dbus-proxy', body: 'No open session bus. Only the D-Bus interfaces you allow reach the host, via xdg-dbus-proxy.' },
  { icon: ArrowLeftRight, color: 'text-[var(--accent-peach)]', hover: 'hover:border-[var(--accent-peach)]/50', title: 'Guest Interceptor', body: 'Ship GUI apps to your launcher and CLI tools to PATH via podbox export — no shared home.' },
  { icon: Boxes, color: 'text-[var(--accent-red)]', hover: 'hover:border-[var(--accent-red)]/50', title: 'Baked-in Image Caching', body: 'Packages bake into the OCI image at build time, not at startup. Containers start in milliseconds.' },
];

export const dayToDay = [
  { dot: 'bg-[var(--accent-mauve)]', title: "1. Build image & install Quadlet units", code: `# Build current directory's podbox.toml\npodbox build .\n\n# Or build from a specific config\npodbox build ~/configs/fedora.toml` },
  { dot: 'bg-[var(--accent-blue)]', title: '2. Enter the interactive container shell', code: `# Start container (if stopped) and enter shell\npodbox enter fedora\n\n# Or run a command directly inside without entering\npodbox exec fedora -- cargo check` },
  { dot: 'bg-[var(--accent-green)]', title: '3. Export GUI app to host launcher', code: `# Export desktop application (.desktop file)\npodbox export app code\n\n# Export CLI tool to ~/.local/bin on host\npodbox export bin rg` },
  { dot: 'bg-[var(--accent-peach)]', title: '4. Inspect systemd lifecycle directly', code: `# Check systemd user service status\nsystemctl --user status podbox-fedora.service\n\n# View container logs via journald\njournalctl --user -u podbox-fedora.service -f` },
];
