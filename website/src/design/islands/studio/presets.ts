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
    blurb: 'CachyOS with a mise-managed toolchain, mr-boxington, and LazyVim',
    icon: Flame,
    accent: 'text-[var(--accent-peach)]',
    overrides: {
      selectedPresetDistro: 'cachyos/cachyos:latest',
      ...named('rust-dev'),
      // mise and fish come from the CachyOS repos — no curl bootstrap, no
      // RUN layer. The toolchain itself installs at first boot via
      // [[provision]] steps, so it lives in the home volume, not the image.
      // `git` is explicit because the base set has curl but *not* git, and
      // the git_identity bridge is a silent no-op in a container without it.
      packagesInstallList: ['git', 'neovim', 'ripgrep', 'mold', 'mise', 'fish'],
      packageManager: 'pacman',
      containerShell: 'fish',
      provisionSteps: [
        {
          name: 'mise-activate',
          run: "mkdir -p ~/.config/fish/conf.d\necho 'mise activate fish | source' > ~/.config/fish/conf.d/mise.fish",
          timeout: '5m',
          onFailure: 'warn',
          envList: [],
          root: false,
        },
        {
          name: 'rust-toolchain',
          run: 'mise use -g rust',
          timeout: '5m',
          onFailure: 'warn',
          envList: [],
          root: false,
        },
        {
          name: 'mr-boxington',
          run: 'mise use --global --tool-option mr_boxington=true rust mr-boxington',
          timeout: '5m',
          onFailure: 'warn',
          envList: [],
          root: false,
        },
        {
          name: 'expose-shims',
          run: 'ln -sf $PODBOX_HOME/.local/share/mise/shims/* /usr/local/bin/',
          timeout: '5m',
          onFailure: 'warn',
          envList: [],
          root: true,
        },
        {
          name: 'lazyvim',
          run: '[ -d ~/.config/nvim ] || git clone https://github.com/LazyVim/starter ~/.config/nvim',
          timeout: '5m',
          onFailure: 'warn',
          envList: [],
          root: false,
        },
      ],
      // Host caches reuse the host's own cargo registry and mbx store;
      // `rustup` stays out — a toolchain is libc-bound and must not cross
      // machines via a shared volume.
      sharedCaches: [],
      hostCaches: ['cargo', 'mbx'],
      // Everything below is already the Studio default except the opt-ins,
      // which are the only lines that reach the TOML.
      intGitIdentity: true,
      intXdgOpen: true,
      intSshAgent: true,
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