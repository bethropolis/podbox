// Shared podbox schema: every Studio field declared exactly once.
//
// The Rust schema (`crates/podbox/src/config`) is the source of truth: its
// defaults and enum variants arrive through the wasm engine (engine.ts),
// and this module wires them to the state the panels edit — the field
// list (FIELDS), the state shape (StudioDefaults / initialStudioValues),
// and the UI metadata the engine does not carry (labels, descriptions).
// `bun run check:studio` guards the wiring against drift.

import { engineDefault, engineSchema, enumVariants } from './engine';
import type { EnvVarItem, HostExecItem, MountItem, ProvisionItem, SecretItem, ServiceItem } from './types';

export type { ProvisionItem };

// ── scalar field kinds ────────────────────────────────────────────────────

export type FieldKind = 'string' | 'number' | 'boolean' | 'select' | 'string-list' | 'xdg-dir';

/**
 * `[integration.xdg_dirs]` is three-state, not boolean: the engine accepts
 * either a plain `true` (read-only mount) or the detailed table form, and only
 * the latter can ask for a writable bind.
 */
export type XdgDirMode = 'off' | 'ro' | 'rw';

export interface FieldDef {
  /** Key in StudioValues, e.g. 'containerMemory'. */
  stateKey: string;
  /** Dotted TOML path, e.g. 'container.memory'. */
  tomlPath: string;
  kind: FieldKind;
}

export interface SelectOption {
  value: string;
  label: string;
  sublabel?: string;
  /** Renders under an `<optgroup>`; options without one stay flat. */
  group?: string;
}

/**
 * UI label metadata for one enum variant. Variants the table does not
 * name fall back to their raw token, so a Rust-side addition stays
 * selectable and the parity check fails until it gets a real label.
 */
export type SelectLabelMeta = Record<string, { label?: string; sublabel?: string; group?: string }>;

// ── options from the schema data ──────────────────────────────────────────

/**
 * Build the select options for a config enum: variant order from the
 * engine, display metadata from `labels`. A variant without a label entry
 * renders with its raw token rather than being silently dropped.
 */
export function enumOptions(path: string, labels: SelectLabelMeta = {}): SelectOption[] {
  return enumVariants(path).map((value) => {
    const meta = labels[value] ?? {};
    return { value, label: meta.label ?? value, ...meta };
  });
}

/**
 * UI labels per config enum, keyed by config path. Every served variant
 * must have an entry here — `check:studio` fails until a new Rust variant
 * is labeled, instead of the option rendering as a raw token.
 */
export const ENUM_LABELS: Record<string, SelectLabelMeta> = {
  'image.packages.manager': {
    dnf: { label: 'DNF (Fedora / RHEL)' },
    apt: { label: 'APT (Ubuntu / Debian)' },
    pacman: { label: 'Pacman (Arch Linux)' },
    apk: { label: 'APK (Alpine)' },
    zypper: { label: 'Zypper (openSUSE)' },
  },
  'dotfiles.clone_on': {
    host: { label: 'host', sublabel: 'clones with host git' },
    container: { label: 'container', sublabel: 'clones inside the image' },
  },
  'container.services.restart': {
    never: { label: 'never' },
    'on-failure': { label: 'on-failure' },
    always: { label: 'always' },
  },
  'security.userns': {
    'keep-id': { label: 'keep-id (Host UID = Container UID)' },
    nomap: { label: 'nomap (Rootless subordinate IDs)' },
    private: { label: 'private (Standard user namespace)' },
  },
  'security.cap_preset': {
    default: { label: 'Default (Standard rootless dev)' },
    none: { label: 'None (Drop all capabilities)' },
    monitoring: { label: 'Monitoring (Add SYS_PTRACE)' },
    admin: { label: 'Admin (CAP_NET_ADMIN / SYS_ADMIN)' },
  },
  'security.secrets.type': {
    env: { label: 'env', sublabel: 'environment variable' },
    mount: { label: 'mount', sublabel: 'file in the container' },
  },
  'security.secrets.source': {
    podman: { label: 'podman secret', sublabel: 'podman secret show' },
    systemd: { label: 'systemd', sublabel: 'LoadCredential' },
  },
  'network.mode': {
    private: { label: 'private (Isolated loopback namespace)' },
    pasta: { label: 'pasta (Podman userspace networking)' },
    host: { label: 'host (Direct host network stack)' },
    bridge: { label: 'bridge (Podman default CNI bridge)' },
    none: { label: 'none (Completely offline / airgapped)' },
    slirp4netns: { label: 'slirp4netns (Legacy user-mode networking)' },
  },
  'lifecycle.on_stop': {
    keep: { label: 'keep (Preserve container on stop)' },
    remove: { label: 'remove (Destroy container on stop)' },
  },
  'dbus.preset': {
    portal: { label: 'portal (Desktop Portals & Notifications)' },
    gnome: { label: 'gnome (GNOME Shell integration)' },
    kde: { label: 'kde (KDE Plasma integration)' },
    flatpak: { label: 'flatpak (Strict Flatpak compatibility)' },
    none: { label: 'none (Custom rules only)' },
  },
  'integration.gpu': {
    auto: { label: 'auto (Pass /dev/dri if present on host)' },
    nvidia: { label: 'nvidia (NVIDIA Container Toolkit CDI)' },
    true: { label: 'true (Require /dev/dri passthrough)' },
    false: { label: 'false (Software rendering only)' },
  },
  'provision.on_failure': {
    warn: { label: 'warn', sublabel: 'warn and continue' },
    abort: { label: 'abort', sublabel: 'stop on first failure' },
  },
  'integration.host_exec.cwd': {
    home: { label: 'home', sublabel: 'ignore the guest cwd (default)' },
    map: {
      label: 'map',
      sublabel: 'translate through the container bind mappings',
    },
    refuse: { label: 'refuse', sublabel: 'error on an unusable cwd' },
  },
};

// ── engine-served data, typed for the Studio ──────────────────────────────

/** The builtin cache tick-list, in engine declaration order. */
export function cacheOrder(): string[] {
  return enumVariants('storage.shared_caches');
}

/** Engine default for a `[[provision]]` step's timeout (`5m`). */
export function provisionTimeout(): string {
  const t = engineDefault('provision.timeout');
  if (typeof t !== 'string') throw new Error('provision.timeout default must be a string');
  return t;
}

/** The allowlist `xdg_open = true` opens when the config names no schemes. */
export const DEFAULT_XDG_OPEN_SCHEMES = ['http', 'https', 'mailto'];

/**
 * TOML paths the engine serves schema data for beyond the one-field-each
 * list: shapes with a custom Studio emitter (state->config in
 * configShape.ts) rather than a scalar line. Kept next to FIELDS so the
 * parity check can pin both halves.
 */
export const CUSTOM_SCHEMA_PATHS = {
  defaults: ['image.run.commands', 'integration.gpu', 'provision.timeout', 'wayland.firewall'],
  enums: [
    'integration.gpu',
    'provision.on_failure',
    'security.secrets.type',
    'security.secrets.source',
    'container.services.restart',
    'storage.shared_caches',
    'storage.host_caches',
  ],
} as const;

// Suggestions for [integration.host_exec.allowlist]. The browser cannot ask
// the host what is installed, so these are the commands worth reaching for
// often enough to be one click away; any absolute path still works.
export const HOST_EXEC_SUGGESTIONS: { alias: string; path: string }[] = [
  { alias: 'podbox', path: '/usr/local/bin/podbox' },
  { alias: 'git', path: '/usr/bin/git' },
  { alias: 'gh', path: '/usr/bin/gh' },
  { alias: 'podman', path: '/usr/bin/podman' },
  { alias: 'docker', path: '/usr/bin/docker' },
  { alias: 'rsync', path: '/usr/bin/rsync' },
];

// A few known container schemes to offer one click away; anything can still
// be typed by hand. `file` is deliberately absent: it opens local files on the
// host for whatever a container page picks as its path.
export const XDG_OPEN_SCHEME_SUGGESTIONS = ['nfs', 'smb', 'vscode', 'steam'];

// ── caches (tick-list from the engine; blurbs are pure UI) ────────────────

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

// The cache tick-lists are engine-served variant lists whose names land in
// the page with a pure-UI blurb underneath. Both double as the label table
// the parity check pins: a cache added in Rust fails check:studio until it
// gets a description here.
const CACHE_LABELS: SelectLabelMeta = Object.fromEntries(
  Object.entries(CACHE_DESCRIPTIONS).map(([name, description]) => [name, { label: name, sublabel: description }]),
);
ENUM_LABELS['storage.shared_caches'] = CACHE_LABELS;
ENUM_LABELS['storage.host_caches'] = CACHE_LABELS;

// ── image presets ─────────────────────────────────────────────────────────

// Tier-1 upstream images, grouped by family. Kept current on purpose:
// Fedora 44 is stable, 45 is still in beta, and the rolling/testing entries
// are opt-in for people who want them.
export const IMAGE_PRESET_OPTIONS: SelectOption[] = [
  { value: 'fedora:44', label: 'Fedora 44', sublabel: 'stable · recommended', group: 'Fedora' },
  { value: 'fedora:45', label: 'Fedora 45', sublabel: 'beta', group: 'Fedora' },
  { value: 'fedora:rawhide', label: 'Fedora Rawhide', sublabel: 'unstable', group: 'Fedora' },
  { value: 'archlinux:latest', label: 'Arch Linux', sublabel: 'rolling', group: 'Arch' },
  { value: 'cachyos/cachyos:latest', label: 'CachyOS', sublabel: 'rolling · perf', group: 'Arch' },
  { value: 'ubuntu:26.04', label: 'Ubuntu 26.04 LTS', group: 'Ubuntu' },
  { value: 'ubuntu:24.04', label: 'Ubuntu 24.04 LTS', group: 'Ubuntu' },
  { value: 'debian:trixie', label: 'Debian 13 (trixie)', sublabel: 'stable', group: 'Debian' },
  { value: 'debian:testing', label: 'Debian testing', sublabel: 'unstable', group: 'Debian' },
  { value: 'alpine:3.24', label: 'Alpine 3.24', sublabel: 'musl · minimal', group: 'Alpine' },
  { value: 'registry.opensuse.org/opensuse/tumbleweed:latest', label: 'openSUSE Tumbleweed', sublabel: 'rolling', group: 'openSUSE' },
];

// Shell names are not config enums: the engine maps these well-known
// binaries in `shell_binary`, anything with a slash passes through verbatim.
export const SHELL_OPTIONS: SelectOption[] = [
  { value: 'bash', label: '/bin/bash' },
  { value: 'fish', label: '/usr/bin/fish' },
  { value: 'zsh', label: '/bin/zsh' },
  { value: 'sh', label: '/bin/sh' },
];

// `manager = "auto"` is the Studio's "no override" token (the engine stores
// `manager` as an unset option); the variants come from the schema data.
export function packageManagerOptions(): SelectOption[] {
  return [
    { value: 'auto', label: 'Auto Detect (Recommended)' },
    ...enumOptions('image.packages.manager', ENUM_LABELS['image.packages.manager']),
  ];
}

export function dotfilesCloneOnOptions(): SelectOption[] {
  return enumOptions('dotfiles.clone_on', ENUM_LABELS['dotfiles.clone_on']);
}

export function serviceRestartOptions(): SelectOption[] {
  return enumOptions('container.services.restart', ENUM_LABELS['container.services.restart']);
}

export function usernsOptions(): SelectOption[] {
  return enumOptions('security.userns', ENUM_LABELS['security.userns']);
}

export function capPresetOptions(): SelectOption[] {
  return enumOptions('security.cap_preset', ENUM_LABELS['security.cap_preset']);
}

export function secretTypeOptions(): SelectOption[] {
  return enumOptions('security.secrets.type', ENUM_LABELS['security.secrets.type']);
}

export function secretSourceOptions(): SelectOption[] {
  return enumOptions('security.secrets.source', ENUM_LABELS['security.secrets.source']);
}

export function networkModeOptions(): SelectOption[] {
  return enumOptions('network.mode', ENUM_LABELS['network.mode']);
}

export function onStopOptions(): SelectOption[] {
  return enumOptions('lifecycle.on_stop', ENUM_LABELS['lifecycle.on_stop']);
}

export function dbusPresetOptions(): SelectOption[] {
  return enumOptions('dbus.preset', ENUM_LABELS['dbus.preset']);
}

export function gpuOptions(): SelectOption[] {
  return enumOptions('integration.gpu', ENUM_LABELS['integration.gpu']);
}

export function provisionOnFailureOptions(): SelectOption[] {
  return enumOptions('provision.on_failure', ENUM_LABELS['provision.on_failure']);
}

// ── categories (tab metadata; icons stay in CategoryTabs.tsx) ─────────────

export interface CategoryDef {
  id: string;
  /** TOML section(s) the panel writes, shown as a small code tag. */
  label: string;
  /** Human name for the tab. */
  name: string;
  blurb: string;
}

export const CATEGORIES: CategoryDef[] = [
  { id: 'image', label: '[image]', name: 'Image', blurb: 'Distro & packages' },
  { id: 'container', label: '[container]', name: 'Container', blurb: 'Resources & services' },
  { id: 'dotfiles', label: '[dotfiles]', name: 'Dotfiles', blurb: 'Repo & setup steps' },
  { id: 'storage', label: '[storage]', name: 'Storage', blurb: 'Caches & mounts' },
  { id: 'security', label: '[security]', name: 'Security', blurb: 'User namespace & caps' },
  { id: 'network', label: '[network]', name: 'Network', blurb: 'Mode & ports' },
  { id: 'integration', label: '[integration]', name: 'Desktop', blurb: 'Display, audio & GPU' },
  { id: 'lifecycle', label: '[lifecycle]', name: 'Lifecycle', blurb: 'Quadlet & autostart' },
  { id: 'proxies', label: '[dbus] [wayland]', name: 'Filters', blurb: 'D-Bus & Wayland' },
];

// ── state shape ───────────────────────────────────────────────────────────

export interface StudioDefaults {
  isFullscreen: boolean;
  activeCategory:
    | 'image'
    | 'container'
    | 'security'
    | 'network'
    | 'integration'
    | 'lifecycle'
    | 'proxies'
    | 'dotfiles'
    | 'storage';
  activeView: 'toml' | 'quadlet' | 'containerfile' | 'host' | 'proxy' | 'compositor';
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
  runCommands: string[];
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
  /** `[container.env].forward` — host vars injected at enter/exec/run. */
  envForward: string[];
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
  /** Schemes beyond the engine default; empty = default (`http`, `https`, `mailto`). */
  intXdgOpenSchemes: string[];
  intClipboard: boolean;
  intSyncFonts: boolean;

  intSyncIcons: boolean;
  intSyncThemes: boolean;
  intSshAgent: boolean;
  intGpgAgent: boolean;
  /** `[[provision]]` steps (top-level, ordered). */
  provisionSteps: ProvisionItem[];
  hostExecEnabled: boolean;
  /** What the fd-passing session does with the guest cwd (`map`/`home`/`refuse`). */
  hostExecCwd: string;
  hostExecList: HostExecItem[];
  // [integration.hardware] — host device passthrough
  hwKvm: boolean;
  hwJoystick: boolean;
  hwWebcam: boolean;
  hwSerial: boolean;
  hwUsb: boolean;
  hwYubikey: boolean;
  /** [security].secrets */
  secrets: SecretItem[];
  xdgDocuments: XdgDirMode;
  xdgDownloads: XdgDirMode;
  xdgPictures: XdgDirMode;
  xdgMusic: XdgDirMode;
  xdgVideos: XdgDirMode;
  xdgDesktop: XdgDirMode;
  xdgProjects: XdgDirMode;
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

/**
 * Keys that describe the view rather than the configuration. Excluded from
 * the saved session and from a preset/reset, so neither persists "fullscreen"
 * or reopens a dropdown.
 */
export const UI_ONLY_KEYS = [
  'isFullscreen',
  'activeCategory',
  'activeView',
  'copied',
  'showExportMenu',
] as const;

/**
 * State defaults that mirror an engine default, by dotted schema path.
 * Built from the schema data at values-construction time instead of being
 * copied, so a default that changes in Rust changes here with it.
 */
const FROM_ENGINE: Partial<Record<keyof StudioDefaults, string>> = {
  pullRetry: 'image.pull_retry',
  pullRetryDelay: 'image.pull_retry_delay',
  packagesRemoveList: 'image.packages.remove',
  dotfilesTarget: 'dotfiles.target',
  dotfilesCloneOn: 'dotfiles.clone_on',
  containerShell: 'container.shell',
  containerSlice: 'container.slice',
  containerCpuWeight: 'container.cpu_weight',
  secLabelDisable: 'security.security_label_disable',
  noNewPrivileges: 'security.no_new_privileges',
  capPreset: 'security.cap_preset',
  netMode: 'network.mode',
  intGpu: 'integration.gpu',
  hostExecCwd: 'integration.host_exec.cwd',
  lifeIdleTimeout: 'lifecycle.idle_timeout',
  lifeOnStop: 'lifecycle.on_stop',
  waylandFirewall: 'wayland.firewall',
  dbusPreset: 'dbus.preset',
};

// Memoized once: the value set is consulted on every render for key sets,
// and rebuilding it (with its engine lookups) would be wasted work. Fresh
// copies for resets/presets are made by spreading `studioDefaults()`.
let cachedValues: StudioDefaults | null = null;

export function studioDefaults(): StudioDefaults {
  if (!cachedValues) cachedValues = initialStudioValues();
  return cachedValues;
}

/**
 * Fresh Studio values: UI-owned prefills (placeholder rows, the sample
 * package list, pseudo tokens like `seccomp = "default"`) plus the engine
 * defaults served by the schema data. A missing engine default is schema
 * drift, so the whole Studio fails the check instead of running on a
 * silently stale value.
 */
export function initialStudioValues(): StudioDefaults {
  const base: StudioDefaults = {
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
    imageName: 'dev',
    imagePrebuiltRef: '',
    pullRetry: 0,
    pullRetryDelay: '',
    packagesInstallList: ['git', 'fish', 'fastfetch'],
    packagesRemoveList: [],
    packageManager: 'auto',
    // Empty on purpose: the engine appends the right per-distro cleanup
    // (dnf/apt/pacman/…) to the install layer itself. A hardcoded clean
    // command here would be redundant on Fedora and break other distros.
    runCommands: [],
    // [dotfiles]
    dotfilesSource: '',
    dotfilesTarget: '',
    dotfilesCloneOn: 'host',
    dotfilesInstall: '',
    // [storage]
    sharedCaches: [],
    hostCaches: [],
    // [container]
    containerName: 'dev',
    containerHome: '~/containers/dev',
    containerShell: 'bash',
    // Empty = unlimited. Limits are opt-in per container, not a base default.
    containerMemory: '',
    containerCpus: '',
    containerSlice: 'podbox.slice',
    containerCpuWeight: 200,
    containerReloadCmd: '',
    // One empty row so the host/guest pairing is discoverable; the emitter
    // drops rows with a blank side, so the placeholder never becomes a real
    // mount.
    extraMounts: [{ host: '', guest: '', mode: 'z' }],
    envVars: [],
    envForward: [],
    services: [],
    // [security]
    apparmor: '',
    // "default" is the Studio's no-token (the engine leaves `seccomp` unset).
    seccomp: 'default',
    secLabelDisable: true,
    noNewPrivileges: true,
    readOnlyRootfs: false,
    // "keep-id" is the Studio's no-token (rootless podman's own default).
    usernsMode: 'keep-id',
    capPreset: 'default',
    extraCapAddList: [],
    // [network]
    netMode: 'pasta',
    netOffline: false,
    portMappingsList: [],
    // [integration]
    // Everything here is opt-in: the engine default is off for all of these
    // except gpu (auto-detect), so Studio only writes what you turn on.
    intGitIdentity: false,
    intWayland: false,
    intAudio: false,
    intGpu: 'auto',
    intDbus: false,
    intNotify: false,
    intXdgOpen: false,
    intXdgOpenSchemes: [],
    intClipboard: false,
    intSyncFonts: false,
    intSyncIcons: false,
    intSyncThemes: false,
    intSshAgent: false,
    intGpgAgent: false,
    hostExecEnabled: false,
    hostExecCwd: 'home',
    hostExecList: [],
    provisionSteps: [],
    hwKvm: false,
    hwJoystick: false,
    hwWebcam: false,
    hwSerial: false,
    hwUsb: false,
    hwYubikey: false,
    secrets: [],
    xdgDocuments: 'off',
    xdgDownloads: 'ro',
    xdgPictures: 'off',
    xdgMusic: 'off',
    xdgVideos: 'off',
    xdgDesktop: 'off',
    xdgProjects: 'rw',
    exportAppsList: [],
    exportBinsList: [],
    // [lifecycle]
    lifeQuadlet: true,
    lifeAutostart: false,
    lifeOnStop: 'keep',
    lifeAutoUpdate: true,
    lifeAutoCheckpoint: false,
    lifeIdleTimeout: 'off',
    // [systemd]
    // Empty: unit ordering is a deliberate edit, not something every box
    // needs. The panel still offers keys and values for those who do.
    sysRequires: '',
    sysAfter: '',
    // [dbus]
    dbusPreset: 'portal',
    dbusTalkList: [],
    dbusOwnList: [],
    // [wayland]
    waylandFirewall: true,
    waylandBlockedList: [],
  };
  const defaults = engineSchema().defaults;
  for (const [key, path] of Object.entries(FROM_ENGINE) as [keyof StudioDefaults, string][]) {
    const engineValue = defaults[path];
    if (engineValue === undefined || engineValue === null) {
      throw new Error(`schema defaults are missing ${path} (drift between Studio and engine)`);
    }
    const current = base[key];
    if (typeof engineValue !== typeof current) {
      throw new Error(`engine default ${path} has type ${typeof engineValue}, expected ${typeof current}`);
    }
    (base as unknown as Record<string, unknown>)[key] = engineValue;
  }
  return base;
}

// ── scalar fields (generic parse) ──────────────────────────────────────────
// TOML emission now happens in Rust (configShape.ts builds the Config JSON);
// FIELDS drives the SettingsPalette index and the scalar import pass that
// reads a parsed config back into state.

export const FIELDS: FieldDef[] = [
  // [image]
  { stateKey: 'imageName', tomlPath: 'image.name', kind: 'string' },
  { stateKey: 'imagePrebuiltRef', tomlPath: 'image.image', kind: 'string' },
  { stateKey: 'pullRetry', tomlPath: 'image.pull_retry', kind: 'number' },
  { stateKey: 'pullRetryDelay', tomlPath: 'image.pull_retry_delay', kind: 'string' },
  // [image.packages]
  { stateKey: 'packagesInstallList', tomlPath: 'image.packages.install', kind: 'string-list' },
  { stateKey: 'packagesRemoveList', tomlPath: 'image.packages.remove', kind: 'string-list' },
  { stateKey: 'packageManager', tomlPath: 'image.packages.manager', kind: 'select' },
  // [container]
  { stateKey: 'containerName', tomlPath: 'container.name', kind: 'string' },
  { stateKey: 'containerHome', tomlPath: 'container.home', kind: 'string' },
  // 'bash' is podbox's own default, so it is never written out; a different
  // shell is an intentional override and gets emitted.
  { stateKey: 'containerShell', tomlPath: 'container.shell', kind: 'string' },
  { stateKey: 'containerMemory', tomlPath: 'container.memory', kind: 'string' },
  { stateKey: 'containerCpus', tomlPath: 'container.cpus', kind: 'string' },
  { stateKey: 'containerSlice', tomlPath: 'container.slice', kind: 'string' },
  { stateKey: 'containerCpuWeight', tomlPath: 'container.cpu_weight', kind: 'number' },
  { stateKey: 'containerReloadCmd', tomlPath: 'container.reload_cmd', kind: 'string' },
  // [dotfiles]
  { stateKey: 'dotfilesSource', tomlPath: 'dotfiles.source', kind: 'string' },
  { stateKey: 'dotfilesTarget', tomlPath: 'dotfiles.target', kind: 'string' },
  { stateKey: 'dotfilesCloneOn', tomlPath: 'dotfiles.clone_on', kind: 'select' },
  { stateKey: 'dotfilesInstall', tomlPath: 'dotfiles.install', kind: 'string' },
  // [security]
  { stateKey: 'apparmor', tomlPath: 'security.apparmor', kind: 'string' },
  { stateKey: 'seccomp', tomlPath: 'security.seccomp', kind: 'string' },
  { stateKey: 'secLabelDisable', tomlPath: 'security.security_label_disable', kind: 'boolean' },
  { stateKey: 'noNewPrivileges', tomlPath: 'security.no_new_privileges', kind: 'boolean' },
  { stateKey: 'readOnlyRootfs', tomlPath: 'security.read_only_rootfs', kind: 'boolean' },
  { stateKey: 'usernsMode', tomlPath: 'security.userns', kind: 'select' },
  { stateKey: 'capPreset', tomlPath: 'security.cap_preset', kind: 'select' },
  { stateKey: 'extraCapAddList', tomlPath: 'security.cap_add', kind: 'string-list' },
  // [network]
  { stateKey: 'netMode', tomlPath: 'network.mode', kind: 'select' },
  { stateKey: 'netOffline', tomlPath: 'network.offline', kind: 'boolean' },
  { stateKey: 'portMappingsList', tomlPath: 'network.ports', kind: 'string-list' },
  // [integration]
  { stateKey: 'intGitIdentity', tomlPath: 'integration.git_identity', kind: 'boolean' },
  { stateKey: 'intWayland', tomlPath: 'integration.wayland', kind: 'boolean' },
  { stateKey: 'intAudio', tomlPath: 'integration.audio', kind: 'boolean' },
  { stateKey: 'intDbus', tomlPath: 'integration.dbus', kind: 'boolean' },
  { stateKey: 'intNotify', tomlPath: 'integration.notify', kind: 'boolean' },
  { stateKey: 'intXdgOpen', tomlPath: 'integration.xdg_open', kind: 'boolean' },
  { stateKey: 'intClipboard', tomlPath: 'integration.clipboard', kind: 'boolean' },
  { stateKey: 'hostExecCwd', tomlPath: 'integration.host_exec.cwd', kind: 'select' },
  { stateKey: 'intSyncFonts', tomlPath: 'integration.sync_fonts', kind: 'boolean' },
  { stateKey: 'intSyncIcons', tomlPath: 'integration.sync_icons', kind: 'boolean' },
  { stateKey: 'intSyncThemes', tomlPath: 'integration.sync_themes', kind: 'boolean' },
  { stateKey: 'intSshAgent', tomlPath: 'integration.ssh_agent', kind: 'boolean' },
  { stateKey: 'intGpgAgent', tomlPath: 'integration.gpg_agent', kind: 'boolean' },
  // [integration.hardware]
  { stateKey: 'hwKvm', tomlPath: 'integration.hardware.kvm', kind: 'boolean' },
  { stateKey: 'hwJoystick', tomlPath: 'integration.hardware.joystick', kind: 'boolean' },
  { stateKey: 'hwWebcam', tomlPath: 'integration.hardware.webcam', kind: 'boolean' },
  { stateKey: 'hwSerial', tomlPath: 'integration.hardware.serial', kind: 'boolean' },
  { stateKey: 'hwUsb', tomlPath: 'integration.hardware.usb', kind: 'boolean' },
  { stateKey: 'hwYubikey', tomlPath: 'integration.hardware.yubikey', kind: 'boolean' },
  // [integration.xdg_dirs]
  { stateKey: 'xdgDocuments', tomlPath: 'integration.xdg_dirs.documents', kind: 'xdg-dir' },
  { stateKey: 'xdgDownloads', tomlPath: 'integration.xdg_dirs.downloads', kind: 'xdg-dir' },
  { stateKey: 'xdgPictures', tomlPath: 'integration.xdg_dirs.pictures', kind: 'xdg-dir' },
  { stateKey: 'xdgMusic', tomlPath: 'integration.xdg_dirs.music', kind: 'xdg-dir' },
  { stateKey: 'xdgVideos', tomlPath: 'integration.xdg_dirs.videos', kind: 'xdg-dir' },
  { stateKey: 'xdgDesktop', tomlPath: 'integration.xdg_dirs.desktop', kind: 'xdg-dir' },
  { stateKey: 'xdgProjects', tomlPath: 'integration.xdg_dirs.projects', kind: 'xdg-dir' },
  // [integration.export]
  { stateKey: 'exportAppsList', tomlPath: 'integration.export.apps', kind: 'string-list' },
  { stateKey: 'exportBinsList', tomlPath: 'integration.export.bins', kind: 'string-list' },
  // [lifecycle]
  { stateKey: 'lifeQuadlet', tomlPath: 'lifecycle.quadlet', kind: 'boolean' },
  { stateKey: 'lifeAutostart', tomlPath: 'lifecycle.autostart', kind: 'boolean' },
  { stateKey: 'lifeOnStop', tomlPath: 'lifecycle.on_stop', kind: 'select' },
  { stateKey: 'lifeAutoUpdate', tomlPath: 'lifecycle.auto_update', kind: 'boolean' },
  { stateKey: 'lifeAutoCheckpoint', tomlPath: 'lifecycle.auto_checkpoint', kind: 'boolean' },
  { stateKey: 'lifeIdleTimeout', tomlPath: 'lifecycle.idle_timeout', kind: 'string' },
  // [dbus]
  { stateKey: 'dbusPreset', tomlPath: 'dbus.preset', kind: 'select' },
  { stateKey: 'dbusTalkList', tomlPath: 'dbus.talk', kind: 'string-list' },
  { stateKey: 'dbusOwnList', tomlPath: 'dbus.own', kind: 'string-list' },
  // [wayland]
  { stateKey: 'waylandBlockedList', tomlPath: 'wayland.blocked_interfaces', kind: 'string-list' },
];

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
/** Accepts `true`, `false`, and the detailed `{ enabled, read_write }`. */
const xdgMode = (v: unknown): XdgDirMode | undefined => {
  if (typeof v === 'boolean') return v ? 'ro' : 'off';
  if (v && typeof v === 'object') {
    const t = v as { enabled?: unknown; read_write?: unknown };
    if (t.enabled !== true) return 'off';
    return t.read_write === true ? 'rw' : 'ro';
  }
  return undefined;
};

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
    case 'xdg-dir':
      return xdgMode(raw);
    case 'string-list':
      return strArr(raw);
  }
}
