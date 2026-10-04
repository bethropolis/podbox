// Shared podbox schema: every Studio field declared exactly once.
//
// When the Rust schema (`crates/podbox/src/config`) gains or changes a
// field, update it HERE and everything follows: state defaults
// (STUDIO_DEFAULTS), TOML emission (emitToml), TOML import (parseTomlDoc),
// tab metadata (CATEGORIES), and select options (OPTIONS).
//
// Panels stay bespoke JSX, but they read defaults, options, and constants
// from this module instead of hardcoding them. `bun run check:studio`
// guards the data layer against drift.

import type { EnvVarItem, HostExecItem, MountItem, ServiceItem } from './types';

// ── scalar field kinds ────────────────────────────────────────────────────

export type FieldKind = 'string' | 'number' | 'boolean' | 'select' | 'string-list';

/** When a scalar field is written to TOML. */
export type EmitMode =
  | 'always' // e.g. integration scalars, required keys
  | 'non-empty' // strings/lists: emit iff truthy / non-empty
  | 'non-default' // emit iff !== def (numbers, selects, sentineled strings)
  | 'is-true' // emit `= true` iff true
  | 'is-false'; // emit `= false` iff false

export interface FieldDef {
  /** Key in StudioValues, e.g. 'containerMemory'. */
  stateKey: string;
  /** Dotted TOML path, e.g. 'container.memory'. */
  tomlPath: string;
  kind: FieldKind;
  /** Studio default; emission compares against it for 'non-default'. */
  def: unknown;
  emit?: EmitMode;
  options?: SelectOption[];
}

export interface SelectOption {
  value: string;
  label: string;
  sublabel?: string;
}

// ── select options (mirrors Rust enums) ───────────────────────────────────

export const IMAGE_PRESET_OPTIONS: SelectOption[] = [
  { value: 'fedora:44', label: 'Fedora 44 Rawhide', sublabel: 'Recommended' },
  { value: 'fedora:43', label: 'Fedora 43' },
  { value: 'fedora:42', label: 'Fedora 42' },
  { value: 'archlinux:latest', label: 'Arch Linux', sublabel: 'Rolling' },
  { value: 'ubuntu:24.04', label: 'Ubuntu 24.04 LTS (Noble)' },
  { value: 'debian:bookworm', label: 'Debian 12 (Bookworm)' },
  { value: 'alpine:3.20', label: 'Alpine 3.20', sublabel: 'musl/minimal' },
];

export const SHELL_OPTIONS: SelectOption[] = [
  { value: 'bash', label: '/bin/bash' },
  { value: 'fish', label: '/usr/bin/fish' },
  { value: 'zsh', label: '/bin/zsh' },
  { value: 'sh', label: '/bin/sh' },
];

export const PACKAGE_MANAGER_OPTIONS: SelectOption[] = [
  { value: 'auto', label: 'Auto Detect (Recommended)' },
  { value: 'dnf', label: 'DNF (Fedora / RHEL)' },
  { value: 'apt', label: 'APT (Ubuntu / Debian)' },
  { value: 'pacman', label: 'Pacman (Arch Linux)' },
  { value: 'apk', label: 'APK (Alpine)' },
  { value: 'zypper', label: 'Zypper (openSUSE)' },
];

export const NETWORK_MODE_OPTIONS: SelectOption[] = [
  { value: 'private', label: 'private (Isolated loopback namespace)' },
  { value: 'pasta', label: 'pasta (Podman userspace networking)' },
  { value: 'host', label: 'host (Direct host network stack)' },
  { value: 'bridge', label: 'bridge (Podman default CNI bridge)' },
  { value: 'none', label: 'none (Completely offline / airgapped)' },
];

export const USERNS_OPTIONS: SelectOption[] = [
  { value: 'keep-id', label: 'keep-id (Host UID = Container UID)' },
  { value: 'nomap', label: 'nomap (Rootless subordinate IDs)' },
  { value: 'private', label: 'private (Standard user namespace)' },
];

export const CAP_PRESET_OPTIONS: SelectOption[] = [
  { value: 'default', label: 'Default (Standard rootless dev)' },
  { value: 'none', label: 'None (Drop all capabilities)' },
  { value: 'monitoring', label: 'Monitoring (Add SYS_PTRACE)' },
  { value: 'admin', label: 'Admin (CAP_NET_ADMIN / SYS_ADMIN)' },
];

export const ON_STOP_OPTIONS: SelectOption[] = [
  { value: 'keep', label: 'keep (Preserve container on stop)' },
  { value: 'remove', label: 'remove (Destroy container on stop)' },
];

export const DBUS_PRESET_OPTIONS: SelectOption[] = [
  { value: 'portal', label: 'portal (Desktop Portals & Notifications)' },
  { value: 'gnome', label: 'gnome (GNOME Shell integration)' },
  { value: 'kde', label: 'kde (KDE Plasma integration)' },
  { value: 'flatpak', label: 'flatpak (Strict Flatpak compatibility)' },
  { value: 'none', label: 'none (Custom rules only)' },
];

export const GPU_OPTIONS: SelectOption[] = [
  { value: 'auto', label: 'auto (Pass /dev/dri if present on host)' },
  { value: 'nvidia', label: 'nvidia (NVIDIA Container Toolkit CDI)' },
  { value: 'true', label: 'true (Require /dev/dri passthrough)' },
  { value: 'false', label: 'false (Software rendering only)' },
];

export const CLONE_ON_OPTIONS: SelectOption[] = [
  { value: 'host', label: 'host (shared, cached)' },
  { value: 'container', label: 'container (isolated)' },
];

export const SERVICE_RESTART_OPTIONS: SelectOption[] = [
  { value: 'on-failure', label: 'on-failure' },
  { value: 'always', label: 'always' },
  { value: 'never', label: 'never' },
];

// ── caches (mirrors BUILTIN_CACHES in crates/podbox/src/config/types/cache.rs)

export const CACHE_ORDER = [
  'cargo',
  'npm',
  'pnpm',
  'pip',
  'uv',
  'yarn',
  'bun',
  'composer',
  'maven',
  'gradle',
  'ccache',
  'go',
  'rustup',
  'mbx',
];

export const CACHE_DESCRIPTIONS: Record<string, string> = {
  cargo: 'Rust crate registry + git checkouts',
  npm: 'Node package cache',
  pnpm: 'pnpm content-addressable store',
  pip: 'Python wheel cache',
  uv: 'uv package cache',
  yarn: 'Yarn Classic + Berry caches',
  bun: 'Bun install cache',
  composer: 'PHP Composer cache',
  maven: 'Maven local repository',
  gradle: 'Gradle build caches',
  ccache: 'C/C++ compiler cache',
  go: 'Go module cache',
  rustup: 'Rust toolchain installs',
  mbx: 'Mr Boxington shared cache',
};

// ── categories (tab metadata; icons stay in CategoryTabs.tsx) ─────────────

export interface CategoryDef {
  id: string;
  label: string;
  blurb: string;
}

export const CATEGORIES: CategoryDef[] = [
  { id: 'image', label: '[image]', blurb: 'Distro & Packages' },
  { id: 'container', label: '[container]', blurb: 'Resources & Shell' },
  { id: 'dotfiles', label: '[dotfiles]', blurb: 'Repo & Install' },
  { id: 'storage', label: '[storage]', blurb: 'Caches & Mounts' },
  { id: 'security', label: '[security]', blurb: 'UserNS & Caps' },
  { id: 'network', label: '[network]', blurb: 'Pasta & Ports' },
  { id: 'integration', label: '[integration]', blurb: 'Wayland & GPU' },
  { id: 'lifecycle', label: '[lifecycle]', blurb: 'Quadlet & Boot' },
  { id: 'dbus', label: '[dbus]', blurb: 'Proxy Rules' },
  { id: 'wayland', label: '[wayland]', blurb: 'Filter Protocol' },
];

// ── defaults (mirrors the useState initializers in useStudioState.ts) ──────
// useState reads from here, so defaults live in exactly one place.
//
// The explicit annotation (rather than `as StudioValues`) is load-bearing:
// StudioValues is derived from useState's return type, so referencing it
// here would be circular and collapse all inference to `any`.

export interface StudioDefaults {
  isFullscreen: boolean;
  activeCategory:
    | 'image'
    | 'container'
    | 'security'
    | 'network'
    | 'integration'
    | 'lifecycle'
    | 'dbus'
    | 'wayland'
    | 'dotfiles'
    | 'storage';
  activeView: 'toml' | 'quadlet' | 'containerfile';
  copied: boolean;
  showExportMenu: boolean;
  activePreset: string;
  imageType: 'preset' | 'custom';
  selectedPresetDistro: string;
  customImageBase: string;
  imageName: string;
  imagePrebuiltRef: string;
  pullRetry: number;
  pullRetryDelay: string;
  packagesInstallList: string[];
  packagesRemoveList: string[];
  packageManager: string;
  runCommands: string;
  dotfilesSource: string;
  dotfilesTarget: string;
  dotfilesCloneOn: string;
  dotfilesInstall: string;
  sharedCaches: string[];
  hostCaches: string[];
  containerName: string;
  containerHome: string;
  containerShell: string;
  containerMemory: string;
  containerCpus: string;
  containerSlice: string;
  containerCpuWeight: number;
  containerReloadCmd: string;
  extraMounts: MountItem[];
  envVars: EnvVarItem[];
  services: ServiceItem[];
  apparmor: string;
  seccomp: string;
  secLabelDisable: boolean;
  noNewPrivileges: boolean;
  readOnlyRootfs: boolean;
  usernsMode: string;
  capPreset: string;
  extraCapAddList: string[];
  netMode: string;
  netOffline: boolean;
  portMappingsList: string[];
  intGitIdentity: boolean;
  intWayland: boolean;
  intAudio: boolean;
  intGpu: string;
  intDbus: boolean;
  intNotify: boolean;
  intXdgOpen: boolean;
  intClipboard: boolean;
  intSyncFonts: boolean;
  intSyncIcons: boolean;
  intSyncThemes: boolean;
  intSshAgent: boolean;
  intGpgAgent: boolean;
  hostExecEnabled: boolean;
  hostExecList: HostExecItem[];
  xdgDocuments: boolean;
  xdgDownloads: boolean;
  xdgPictures: boolean;
  xdgMusic: boolean;
  xdgVideos: boolean;
  xdgDesktop: boolean;
  xdgProjects: boolean;
  exportAppsList: string[];
  exportBinsList: string[];
  lifeQuadlet: boolean;
  lifeAutostart: boolean;
  lifeOnStop: string;
  lifeAutoUpdate: boolean;
  lifeAutoCheckpoint: boolean;
  lifeIdleTimeout: string;
  sysRequires: string;
  sysAfter: string;
  dbusPreset: string;
  dbusTalkList: string[];
  dbusOwnList: string[];
  waylandFirewall: boolean;
  waylandBlockedList: string[];
}

export const STUDIO_DEFAULTS: StudioDefaults = {
  // view state
  isFullscreen: false,
  activeCategory: 'image',
  activeView: 'toml',
  copied: false,
  showExportMenu: false,
  activePreset: 'custom',
  // [image]
  imageType: 'preset',
  selectedPresetDistro: 'fedora:44',
  customImageBase: 'ghcr.io/username/custom-env:latest',
  imageName: 'dev-box',
  imagePrebuiltRef: '',
  pullRetry: 3,
  pullRetryDelay: '5s',
  packagesInstallList: ['git'],
  packagesRemoveList: [],
  packageManager: 'auto',
  runCommands: 'dnf clean all',
  // [dotfiles]
  dotfilesSource: '',
  dotfilesTarget: '',
  dotfilesCloneOn: 'host',
  dotfilesInstall: '',
  // [storage]
  sharedCaches: [],
  hostCaches: [],
  // [container]
  containerName: 'dev-box',
  containerHome: '~/containers/dev-box',
  containerShell: 'bash',
  containerMemory: '4G',
  containerCpus: '2.0',
  containerSlice: 'podbox.slice',
  containerCpuWeight: 200,
  containerReloadCmd: '',
  extraMounts: [],
  envVars: [],
  services: [],
  // [security]
  apparmor: '',
  seccomp: 'default',
  secLabelDisable: true,
  noNewPrivileges: true,
  readOnlyRootfs: false,
  usernsMode: 'keep-id',
  capPreset: 'default',
  extraCapAddList: [],
  // [network]
  netMode: 'private',
  netOffline: false,
  portMappingsList: [],
  // [integration]
  intGitIdentity: true,
  intWayland: true,
  intAudio: true,
  intGpu: 'auto',
  intDbus: false,
  intNotify: false,
  intXdgOpen: false,
  intClipboard: false,
  intSyncFonts: false,
  intSyncIcons: false,
  intSyncThemes: false,
  intSshAgent: false,
  intGpgAgent: false,
  hostExecEnabled: false,
  hostExecList: [],
  xdgDocuments: false,
  xdgDownloads: false,
  xdgPictures: false,
  xdgMusic: false,
  xdgVideos: false,
  xdgDesktop: false,
  xdgProjects: false,
  exportAppsList: [],
  exportBinsList: [],
  // [lifecycle]
  lifeQuadlet: true,
  lifeAutostart: false,
  lifeOnStop: 'keep',
  lifeAutoUpdate: false,
  lifeAutoCheckpoint: false,
  lifeIdleTimeout: 'off',
  // [systemd]
  sysRequires: '',
  sysAfter: 'network-online.target',
  // [dbus]
  dbusPreset: 'portal',
  dbusTalkList: [],
  dbusOwnList: [],
  // [wayland]
  waylandFirewall: false,
  waylandBlockedList: [],
};

// ── scalar fields (generic emit/parse) ─────────────────────────────────────
// Custom shapes (mount lists, env maps, services, cache tables, section
// gating) live in generateToml.ts / parseImport.ts next to the engine.

export const FIELDS: FieldDef[] = [
  // [image]
  { stateKey: 'imageName', tomlPath: 'image.name', kind: 'string', def: '', emit: 'always' },
  { stateKey: 'imagePrebuiltRef', tomlPath: 'image.image', kind: 'string', def: '', emit: 'non-empty' },
  { stateKey: 'pullRetry', tomlPath: 'image.pull_retry', kind: 'number', def: 3, emit: 'non-default' },
  { stateKey: 'pullRetryDelay', tomlPath: 'image.pull_retry_delay', kind: 'string', def: '5s', emit: 'non-default' },
  // [image.packages]
  { stateKey: 'packagesInstallList', tomlPath: 'image.packages.install', kind: 'string-list', def: [], emit: 'non-empty' },
  { stateKey: 'packagesRemoveList', tomlPath: 'image.packages.remove', kind: 'string-list', def: [], emit: 'non-empty' },
  { stateKey: 'packageManager', tomlPath: 'image.packages.manager', kind: 'select', def: 'auto', emit: 'non-default', options: PACKAGE_MANAGER_OPTIONS },
  // [container]
  { stateKey: 'containerName', tomlPath: 'container.name', kind: 'string', def: '', emit: 'always' },
  { stateKey: 'containerHome', tomlPath: 'container.home', kind: 'string', def: '', emit: 'non-empty' },
  { stateKey: 'containerShell', tomlPath: 'container.shell', kind: 'string', def: '', emit: 'non-empty' },
  { stateKey: 'containerMemory', tomlPath: 'container.memory', kind: 'string', def: '', emit: 'non-empty' },
  { stateKey: 'containerCpus', tomlPath: 'container.cpus', kind: 'string', def: '', emit: 'non-empty' },
  { stateKey: 'containerSlice', tomlPath: 'container.slice', kind: 'string', def: 'podbox.slice', emit: 'non-default' },
  { stateKey: 'containerCpuWeight', tomlPath: 'container.cpu_weight', kind: 'number', def: 200, emit: 'non-default' },
  { stateKey: 'containerReloadCmd', tomlPath: 'container.reload_cmd', kind: 'string', def: '', emit: 'non-empty' },
  // [dotfiles]
  { stateKey: 'dotfilesSource', tomlPath: 'dotfiles.source', kind: 'string', def: '', emit: 'non-empty' },
  { stateKey: 'dotfilesTarget', tomlPath: 'dotfiles.target', kind: 'string', def: '', emit: 'non-empty' },
  { stateKey: 'dotfilesCloneOn', tomlPath: 'dotfiles.clone_on', kind: 'select', def: 'host', emit: 'non-default', options: CLONE_ON_OPTIONS },
  { stateKey: 'dotfilesInstall', tomlPath: 'dotfiles.install', kind: 'string', def: '', emit: 'non-empty' },
  // [security]
  { stateKey: 'apparmor', tomlPath: 'security.apparmor', kind: 'string', def: '', emit: 'non-empty' },
  { stateKey: 'seccomp', tomlPath: 'security.seccomp', kind: 'string', def: 'default', emit: 'non-default' },
  { stateKey: 'secLabelDisable', tomlPath: 'security.security_label_disable', kind: 'boolean', def: true, emit: 'is-false' },
  { stateKey: 'noNewPrivileges', tomlPath: 'security.no_new_privileges', kind: 'boolean', def: true, emit: 'is-false' },
  { stateKey: 'readOnlyRootfs', tomlPath: 'security.read_only_rootfs', kind: 'boolean', def: false, emit: 'is-true' },
  { stateKey: 'usernsMode', tomlPath: 'security.userns', kind: 'select', def: 'keep-id', emit: 'non-default', options: USERNS_OPTIONS },
  { stateKey: 'capPreset', tomlPath: 'security.cap_preset', kind: 'select', def: 'default', emit: 'non-default', options: CAP_PRESET_OPTIONS },
  { stateKey: 'extraCapAddList', tomlPath: 'security.cap_add', kind: 'string-list', def: [], emit: 'non-empty' },
  // [network]
  { stateKey: 'netMode', tomlPath: 'network.mode', kind: 'select', def: 'private', emit: 'non-default', options: NETWORK_MODE_OPTIONS },
  { stateKey: 'netOffline', tomlPath: 'network.offline', kind: 'boolean', def: false, emit: 'is-true' },
  { stateKey: 'portMappingsList', tomlPath: 'network.ports', kind: 'string-list', def: [], emit: 'non-empty' },
  // [integration]
  { stateKey: 'intGitIdentity', tomlPath: 'integration.git_identity', kind: 'boolean', def: true, emit: 'is-false' },
  { stateKey: 'intWayland', tomlPath: 'integration.wayland', kind: 'boolean', def: true, emit: 'always' },
  { stateKey: 'intAudio', tomlPath: 'integration.audio', kind: 'boolean', def: true, emit: 'always' },
  { stateKey: 'intDbus', tomlPath: 'integration.dbus', kind: 'boolean', def: false, emit: 'always' },
  { stateKey: 'intNotify', tomlPath: 'integration.notify', kind: 'boolean', def: false, emit: 'is-false' },
  { stateKey: 'intXdgOpen', tomlPath: 'integration.xdg_open', kind: 'boolean', def: false, emit: 'is-false' },
  { stateKey: 'intClipboard', tomlPath: 'integration.clipboard', kind: 'boolean', def: false, emit: 'is-false' },
  { stateKey: 'intSyncFonts', tomlPath: 'integration.sync_fonts', kind: 'boolean', def: false, emit: 'is-false' },
  { stateKey: 'intSyncIcons', tomlPath: 'integration.sync_icons', kind: 'boolean', def: false, emit: 'is-false' },
  { stateKey: 'intSyncThemes', tomlPath: 'integration.sync_themes', kind: 'boolean', def: false, emit: 'is-false' },
  { stateKey: 'intSshAgent', tomlPath: 'integration.ssh_agent', kind: 'boolean', def: false, emit: 'is-true' },
  { stateKey: 'intGpgAgent', tomlPath: 'integration.gpg_agent', kind: 'boolean', def: false, emit: 'is-true' },
  // [integration.xdg_dirs]
  { stateKey: 'xdgDocuments', tomlPath: 'integration.xdg_dirs.documents', kind: 'boolean', def: false, emit: 'is-true' },
  { stateKey: 'xdgDownloads', tomlPath: 'integration.xdg_dirs.downloads', kind: 'boolean', def: false, emit: 'is-true' },
  { stateKey: 'xdgPictures', tomlPath: 'integration.xdg_dirs.pictures', kind: 'boolean', def: false, emit: 'is-true' },
  { stateKey: 'xdgMusic', tomlPath: 'integration.xdg_dirs.music', kind: 'boolean', def: false, emit: 'is-true' },
  { stateKey: 'xdgVideos', tomlPath: 'integration.xdg_dirs.videos', kind: 'boolean', def: false, emit: 'is-true' },
  { stateKey: 'xdgDesktop', tomlPath: 'integration.xdg_dirs.desktop', kind: 'boolean', def: false, emit: 'is-true' },
  { stateKey: 'xdgProjects', tomlPath: 'integration.xdg_dirs.projects', kind: 'boolean', def: false, emit: 'is-true' },
  // [integration.export]
  { stateKey: 'exportAppsList', tomlPath: 'integration.export.apps', kind: 'string-list', def: [], emit: 'non-empty' },
  { stateKey: 'exportBinsList', tomlPath: 'integration.export.bins', kind: 'string-list', def: [], emit: 'non-empty' },
  // [lifecycle]
  { stateKey: 'lifeQuadlet', tomlPath: 'lifecycle.quadlet', kind: 'boolean', def: false, emit: 'non-default' },
  { stateKey: 'lifeAutostart', tomlPath: 'lifecycle.autostart', kind: 'boolean', def: false, emit: 'is-true' },
  { stateKey: 'lifeOnStop', tomlPath: 'lifecycle.on_stop', kind: 'select', def: 'keep', emit: 'non-default', options: ON_STOP_OPTIONS },
  { stateKey: 'lifeAutoUpdate', tomlPath: 'lifecycle.auto_update', kind: 'boolean', def: false, emit: 'is-true' },
  { stateKey: 'lifeAutoCheckpoint', tomlPath: 'lifecycle.auto_checkpoint', kind: 'boolean', def: false, emit: 'is-true' },
  { stateKey: 'lifeIdleTimeout', tomlPath: 'lifecycle.idle_timeout', kind: 'string', def: 'off', emit: 'non-default' },
  // [dbus]
  { stateKey: 'dbusPreset', tomlPath: 'dbus.preset', kind: 'select', def: 'portal', emit: 'non-default', options: DBUS_PRESET_OPTIONS },
  { stateKey: 'dbusTalkList', tomlPath: 'dbus.talk', kind: 'string-list', def: [], emit: 'non-empty' },
  { stateKey: 'dbusOwnList', tomlPath: 'dbus.own', kind: 'string-list', def: [], emit: 'non-empty' },
  // [wayland]
  { stateKey: 'waylandBlockedList', tomlPath: 'wayland.blocked_interfaces', kind: 'string-list', def: [], emit: 'non-empty' },
];

// ── generic engine primitives ─────────────────────────────────────────────

export function tableOf(tomlPath: string): string {
  return tomlPath.split('.').slice(0, -1).join('.');
}

export function keyOf(tomlPath: string): string {
  return tomlPath.split('.').slice(-1)[0];
}

export function shouldEmit(field: FieldDef, value: unknown): boolean {
  switch (field.emit ?? 'non-default') {
    case 'always':
      return true;
    case 'non-empty':
      return Array.isArray(value) ? value.length > 0 : value !== '' && value !== undefined && value !== null;
    case 'non-default':
      return Array.isArray(value) || Array.isArray(field.def)
        ? JSON.stringify(value) !== JSON.stringify(field.def)
        : value !== field.def;
    case 'is-true':
      return value === true;
    case 'is-false':
      return value === false;
  }
}

export function formatScalar(kind: FieldKind, value: unknown): string {
  if (kind === 'string-list') {
    return `[${((value as string[]) ?? []).map((e) => `"${e}"`).join(', ')}]`;
  }
  if (kind === 'boolean' || kind === 'number') return String(value);
  return `"${value}"`;
}

/** Read a dotted path from a parsed TOML document. */
export function getPath(doc: Record<string, any>, tomlPath: string): unknown {
  let cur: any = doc;
  for (const part of tomlPath.split('.')) {
    if (!cur || typeof cur !== 'object') return undefined;
    cur = cur[part];
  }
  return cur;
}

const str = (v: unknown): string | undefined => (typeof v === 'string' ? v : undefined);
const num = (v: unknown): number | undefined => (typeof v === 'number' ? v : undefined);
const bool = (v: unknown): boolean | undefined => (typeof v === 'boolean' ? v : undefined);
const strArr = (v: unknown): string[] | undefined =>
  Array.isArray(v) ? v.filter((e): e is string => typeof e === 'string') : undefined;

/** Coerce a TOML value into state shape for a scalar field. */
export function coerceScalar(kind: FieldKind, raw: unknown): unknown {
  switch (kind) {
    case 'string':
    case 'select':
      return str(raw);
    case 'number':
      return num(raw);
    case 'boolean':
      return bool(raw);
    case 'string-list':
      return strArr(raw);
  }
}
