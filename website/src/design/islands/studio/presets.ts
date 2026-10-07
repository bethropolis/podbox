import { Flame, Globe, Lock, Monitor, type LucideIcon } from 'lucide-react';
import { STUDIO_DEFAULTS, UI_ONLY_KEYS } from './schema';
import type { StudioValues } from './useStudioState';

export type PresetId = 'rust' | 'arch-gui' | 'fullstack' | 'minimal';

export interface PresetDef {
  id: PresetId;
  label: string;
  /** One line on what this preset is for, shown in the menu. */
  blurb: string;
  icon: LucideIcon;
  /** Tailwind text colour for the icon. */
  accent: string;
  /** Only what makes this preset different. */
  overrides: Partial<StudioValues>;
}

/** Container name drives the image tag and the home path; keep them in sync. */
const named = (name: string) =>
  ({ containerName: name, imageName: name, containerHome: `~/containers/${name}` }) satisfies Partial<StudioValues>;

const desktopIntegration = {
  intWayland: true,
  intAudio: true,
  intGpu: 'auto',
} satisfies Partial<StudioValues>;

const headlessIntegration = {
  intWayland: false,
  intAudio: false,
  intGpu: 'false',
} satisfies Partial<StudioValues>;

export const PRESETS: readonly PresetDef[] = [
  {
    id: 'rust',
    label: 'Rust Dev',
    blurb: 'Fedora with a mise-managed toolchain and mr-boxington',
    icon: Flame,
    accent: 'text-[var(--accent-peach)]',
    overrides: {
      selectedPresetDistro: 'fedora:44',
      ...named('rust-dev'),
      // The toolchain comes from mise, not dnf: installing distro `cargo` and
      // `rustc` alongside it would shadow the mise shims. `git` is listed
      // explicitly because podbox's base set has curl but *not* git, and the
      // git_identity bridge is a silent no-op in a container without it.
      packagesInstallList: ['git', 'neovim', 'ripgrep', 'mold'],
      runCommands: [
        'curl -fsSL https://mise.run | MISE_INSTALL_PATH=/usr/local/bin/mise sh',
        // Three locations have to be pinned, or the toolchain builds fine and
        // then fails at runtime:
        //   MISE_DATA_DIR          tool installs (default ~/.local/share/mise)
        //   MISE_GLOBAL_CONFIG_FILE  `mise use --global` config — without it the
        //     build writes root's ~/.config/mise and no shim finds a version
        //   RUSTUP_HOME            mise's `rust` tool is only a rustup proxy, so
        //     the real toolchain follows RUSTUP_HOME (default ~/.rustup) and
        //     lands in root's home, invisible to the container user
        // CARGO_HOME is deliberately left alone: it stays per-user, which is
        // where the shared cargo cache volume is mounted (~/.cargo/registry).
        'MISE_DATA_DIR=/usr/local/share/mise MISE_GLOBAL_CONFIG_FILE=/usr/local/share/mise/config.toml RUSTUP_HOME=/usr/local/share/rustup mise use --global --tool-option mr_boxington=true rust mr-boxington && ln -sf /usr/local/share/mise/shims/* /usr/local/bin/',
      ],
      envVars: [
        { key: 'MISE_DATA_DIR', value: '/usr/local/share/mise' },
        { key: 'MISE_GLOBAL_CONFIG_FILE', value: '/usr/local/share/mise/config.toml' },
        { key: 'RUSTUP_HOME', value: '/usr/local/share/rustup' },
      ],
      // mr-boxington runs in the container, so its ~/.cache/mbx belongs with
      // the cargo registry: podbox-managed volumes, shared between containers.
      // `rustup` is deliberately absent — a toolchain is libc-bound and must
      // not cross distro boundaries.
      sharedCaches: ['cargo', 'mbx'],
      // Deviates from podbox's `audio = true`; everything else below is
      // already the Studio default and therefore never written to the TOML.
      intAudio: false,
      intNotify: true,
      intXdgOpen: true,
      intClipboard: true,
      intSshAgent: true,
      xdgProjects: 'rw',
    },
  },
  {
    id: 'arch-gui',
    label: 'Arch GUI',
    blurb: 'Rolling Arch with a Wayland desktop and clipboard sharing',
    icon: Monitor,
    accent: 'text-[var(--accent-blue)]',
    overrides: {
      selectedPresetDistro: 'archlinux:latest',
      ...named('arch-desktop'),
      packagesInstallList: ['firefox', 'alacritty', 'neovim', 'mesa', 'pipewire'],
      runCommands: ['pacman -Scc --noconfirm'],
      ...desktopIntegration,
      intClipboard: true,
      dbusPreset: 'portal',
      lifeQuadlet: true,
    },
  },
  {
    id: 'fullstack',
    label: 'Full-Stack',
    blurb: 'Headless web stack with dev ports published and autostart on',
    icon: Globe,
    accent: 'text-[var(--accent-green)]',
    overrides: {
      selectedPresetDistro: 'ubuntu:24.04',
      ...named('fullstack-web'),
      packagesInstallList: ['nodejs', 'npm', 'pnpm', 'git', 'curl', 'python3'],
      runCommands: ['apt-get clean', 'rm -rf /var/lib/apt/lists/*'],
      portMappingsList: ['3000:3000', '5173:5173', '8080:8080'],
      netMode: 'pasta',
      ...headlessIntegration,
      lifeAutostart: true,
    },
  },
  {
    id: 'minimal',
    label: 'Hardened',
    blurb: 'Offline, read-only rootfs, no desktop integration at all',
    icon: Lock,
    accent: 'text-[var(--accent-red)]',
    overrides: {
      selectedPresetDistro: 'alpine:3.24',
      ...named('micro-box'),
      packagesInstallList: ['busybox-extras', 'curl', 'ca-certificates'],
      runCommands: ['apk cache clean'],
      ...headlessIntegration,
      intDbus: false,
      readOnlyRootfs: true,
      noNewPrivileges: true,
      // Ports are rejected when the network is none/offline. The defaults are
      // merged underneath, so this preset can no longer inherit them from the
      // session it was applied to.
      netMode: 'none',
      portMappingsList: [],
    },
  },
];

/**
 * Reset to a blank slate.
 *
 * Built from `STUDIO_DEFAULTS` so it cannot drift from what a fresh page load
 * produces, minus the view state — resetting should not close a dropdown or
 * throw the user out of fullscreen.
 */
export function defaultPatch(): Partial<StudioValues> {
  const base = { ...STUDIO_DEFAULTS } as Record<string, unknown>;
  for (const key of UI_ONLY_KEYS) delete base[key];
  return { ...base, activePreset: 'custom' } as Partial<StudioValues>;
}

/**
 * A preset is a complete configuration, not a delta: defaults first, then the
 * preset's own overrides. Applying one therefore always lands on the same
 * result, whatever the session looked like beforehand.
 */
export function presetPatch(id: PresetId): Partial<StudioValues> {
  const def = PRESETS.find((p) => p.id === id);
  if (!def) return defaultPatch();
  return { ...defaultPatch(), ...def.overrides, activePreset: id };
}