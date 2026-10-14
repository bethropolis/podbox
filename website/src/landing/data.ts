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
  aur: `# Arch Linux, via AUR (binary, fast)
paru -S podbox-bin

# ...or build from source
paru -S podbox`,
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

// Each feature ends in the config line or command that makes it true.
export const features = [
  {
    title: 'One TOML file',
    body: 'Image, packages, mounts and runtime in a single file you can commit. No flags to retype.',
    detail: 'base = "fedora:44"',
  },
  {
    title: 'systemd runs it',
    body: 'Autostart, restarts and socket activation come from systemd. No podbox process stays behind.',
    detail: 'systemctl --user status dev.service',
  },
  {
    title: 'Your screen and sound',
    body: 'GUI apps and games work, GPU included. Wayland, PipeWire and graphics are wired up for you.',
    detail: 'wayland = true · gpu = "auto"',
  },
  {
    title: 'Filtered D-Bus',
    body: 'The session bus is not handed over whole. Only the names you allow reach the host.',
    detail: 'talk = ["org.freedesktop.Notifications"]',
  },
  {
    title: 'Apps without a shared home',
    body: 'Put a desktop entry in your launcher or a tool on your PATH, with no home directory shared.',
    detail: 'podbox export app code',
  },
  {
    title: 'Fast starts',
    body: 'Packages are baked into the image at build time, so starting a container installs nothing.',
    detail: 'install = ["neovim", "ripgrep", "gcc"]',
  },
];

export const dayToDay = [
  {
    title: 'Build the image',
    code: `# Build, or rebuild after editing the config
podbox build dev

# Point at a config file anywhere
podbox --config ./podbox.toml build`,
  },
  {
    title: 'Get in, or run one command',
    code: `# Start it if needed and open a shell
podbox enter dev

# Run a single command without a shell
podbox -C dev exec cargo check`,
  },
  {
    title: 'Put an app on your desktop',
    code: `# Add an app to your host launcher
podbox -C dev export app code

# Put a CLI tool on your host PATH
podbox -C dev export bin rg`,
  },
  {
    title: 'See what it is doing',
    code: `podbox status dev
podbox logs dev --follow

# It is an ordinary systemd user service
systemctl --user status dev.service`,
  },
];
