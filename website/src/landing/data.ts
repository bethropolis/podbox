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
  { id: 'mise', label: 'mise' },
  { id: 'brew', label: 'Homebrew' },
  { id: 'aur', label: 'Arch (AUR)' },
  { id: 'cargo', label: 'cargo install' },
  { id: 'source', label: 'Source' },
] as const;

export type InstallTabId = (typeof installTabs)[number]['id'];

export const installCommands: Record<string, string> = {
  curl: `# Grab the binary & verify
curl -fsSL https://bethropolis.github.io/podbox/install.sh | sh`,
  mise: `# Install as a mise tool (Linux only)
mise use -g github:bethropolis/podbox`,
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
  { icon: FileCode2, color: 'text-[var(--accent-mauve)]', hover: 'hover:border-[var(--accent-mauve)]/40', title: 'One TOML file', body: 'Image, packages, mounts and runtime in a single file you can commit. No flags to retype.' },
  { icon: Cpu, color: 'text-[var(--accent-blue)]', hover: 'hover:border-[var(--accent-blue)]/40', title: 'systemd runs it', body: 'Autostart, restart and socket activation come from systemd. No podbox daemon runs on your machine.' },
  { icon: Monitor, color: 'text-[var(--accent-teal)]', hover: 'hover:border-[var(--accent-teal)]/40', title: 'Your screen and sound', body: 'GUI apps and games work, with GPU acceleration. Wayland, PipeWire and graphics are handled for you.' },
  { icon: Radio, color: 'text-[var(--accent-green)]', hover: 'hover:border-[var(--accent-green)]/40', title: 'Filtered D-Bus', body: 'The session bus is not handed over whole. Only the interfaces you allow reach the host.' },
  { icon: ArrowLeftRight, color: 'text-[var(--accent-peach)]', hover: 'hover:border-[var(--accent-peach)]/40', title: 'Apps without a shared home', body: 'Export a desktop entry to your launcher or a tool to your PATH, with no home directory shared.' },
  { icon: Boxes, color: 'text-[var(--accent-red)]', hover: 'hover:border-[var(--accent-red)]/40', title: 'Fast starts', body: 'Packages are baked into the image at build time, so containers start in milliseconds.' },
];

export const dayToDay = [
  { title: 'Build the image', code: `# Build current directory's podbox.toml\npodbox build .\n\n# Or build from a specific config\npodbox build ~/configs/fedora.toml` },
  { title: 'Get in, or run one command', code: `# Start container (if stopped) and enter shell\npodbox enter fedora\n\n# Or run a command directly inside without entering\npodbox exec fedora -- cargo check` },
  { title: 'Put an app on your desktop', code: `# Export desktop application (.desktop file)\npodbox export app code\n\n# Export CLI tool to ~/.local/bin on host\npodbox export bin rg` },
  { title: 'See what it is doing', code: `# Check systemd user service status\nsystemctl --user status podbox-fedora.service\n\n# View container logs via journald\njournalctl --user -u podbox-fedora.service -f` },
];
