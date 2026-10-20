// Studio state -> Config-shaped JSON.
//
// TOML emission itself lives in Rust (engine.ts `configToToml`); this module
// only translates the Studio state into the object shape the `Config`
// deserializer expects, mirroring the decisions the TypeScript emitter used
// to make: which sections appear (a `[table]` with nothing inside is dead
// config), which values are convenience tokens the engine does not accept.
// Rust's own `skip_serializing_if` rules drop engine defaults, so unset
// values can ride along harmlessly.

import { engineDefault, configToToml } from './engine';
import type { StudioValues } from './useStudioState';
import type { SecretItem } from './types';

type V = Record<string, any>;

/** `xdg_open` allows these schemes when the config names none. */
const DEFAULT_XDG_OPEN_SCHEMES = ['http', 'https', 'mailto'];

/**
 * The Studio's no-tokens: values that mean "leave the engine key unset"
 * (`seccomp = "default"` is not a valid podbox value — seccomp profiles are
 * paths; `keep-id` is what rootless podman does anyway; `auto` GPU is the
 * engine default; `portal` is the effective D-Bus preset once the proxy is
 * on with no rules).
 */
const LEAVE_UNSET = {
  seccomp: 'default',
  userns: 'keep-id',
  gpu: 'auto',
  dbusPreset: 'portal',
} as const;

/** `xdg_dirs` entries: `off` drops the key, the rest become bool/table. */
function xdgDirValue(mode: string): unknown {
  return mode === 'rw' ? { enabled: true, read_write: true } : mode === 'ro';
}

function splitList(raw: string): string[] {
  return raw.split(',').map((s) => s.trim()).filter(Boolean);
}

/** One `[security].secrets` entry; shorthand strings only for bare entries. */
function secretToConfig(secret: SecretItem, allBare: boolean): unknown {
  if (allBare) return secret.name.trim();
  const out: V = { name: secret.name.trim() };
  // The defaults (`type = env`, `source = podman`) stay out of the object:
  // the Rust serializer writes nothing around them and neither do we.
  if (secret.secretType !== 'env') out.type = secret.secretType;
  if (secret.source !== 'podman') out.source = secret.source;
  if (secret.target.trim()) out.target = secret.target.trim();
  if (secret.mode.trim()) out.mode = secret.mode.trim();
  return out;
}

export function stateToConfig(s: StudioValues): V {
  // [image] — preset distros are plain base refs; both feed `base`.
  const image: V = {
    base: s.imageType === 'preset' ? s.selectedPresetDistro : s.customImageBase,
    name: s.imageName,
  };
  if (s.imagePrebuiltRef) image.image = s.imagePrebuiltRef;
  // Engine defaults ride along (pull_retry 3, "5s") and are skipped by the
  // serializer; only real values show up.
  image.pull_retry = s.pullRetry;
  image.pull_retry_delay = s.pullRetryDelay;
  const packages: V = {};
  const install = s.packagesInstallList.filter((p) => p.trim());
  const remove = s.packagesRemoveList.filter((p) => p.trim());
  if (install.length > 0) packages.install = install;
  if (remove.length > 0) packages.remove = remove;
  if (s.packageManager !== 'auto') packages.manager = s.packageManager;
  if (Object.keys(packages).length > 0) image.packages = packages;
  const runCommands = s.runCommands.filter((r) => r.trim());
  if (runCommands.length > 0) image.run = { commands: runCommands };

  // [container]
  const container: V = {
    name: s.containerName,
    home: s.containerHome,
    shell: s.containerShell,
    slice: s.containerSlice,
    cpu_weight: s.containerCpuWeight,
  };
  if (s.containerMemory) container.memory = s.containerMemory;
  if (s.containerCpus) container.cpus = s.containerCpus;
  if (s.containerReloadCmd) container.reload_cmd = s.containerReloadCmd;
  const mounts = s.extraMounts.filter((m) => m.host.trim() && m.guest.trim());
  if (mounts.length > 0) {
    container.mounts = {
      extra: mounts.map((m) => `${m.host}:${m.guest}:${m.mode || 'z'}`),
    };
  }
  const env: V = {};
  for (const e of s.envVars) {
    if (e.key.trim()) env[e.key] = e.value;
  }
  const forward = s.envForward.map((f) => f.trim()).filter(Boolean);
  if (forward.length > 0) env.forward = forward;
  if (Object.keys(env).length > 0) container.env = env;
  const services: V = {};
  for (const svc of s.services.filter((x) => x.name.trim() && x.command.trim())) {
    const restart = svc.restart || 'on-failure';
    // The short form is the engine default policy; anything else needs
    // the detailed table.
    services[svc.name.trim()] = restart === 'on-failure' ? svc.command.trim() : { command: svc.command.trim(), restart };
  }
  if (Object.keys(services).length > 0) container.services = services;

  const config: V = {
    image,
    container,
    integration: {},
    lifecycle: {},
    systemd: {},
    network: {},
    security: {},
  };

  // [dotfiles] — no table while only defaults are filled in.
  const dotfiles: V = { source: s.dotfilesSource, target: s.dotfilesTarget, clone_on: s.dotfilesCloneOn };
  if (s.dotfilesInstall) dotfiles.install = s.dotfilesInstall;
  if (
    s.dotfilesSource ||
    s.dotfilesInstall ||
    s.dotfilesTarget !== engineDefault('dotfiles.target') ||
    s.dotfilesCloneOn !== 'host'
  ) {
    config.dotfiles = dotfiles;
  }

  // [[provision]] — blank rows cannot parse, so only usable steps land.
  const steps = s.provisionSteps
    .filter((p) => p.name.trim() && p.run.trim())
    .map((p) => {
      const step: V = { name: p.name.trim(), run: p.run.trim(), timeout: p.timeout, on_failure: p.onFailure };
      const envPairs: V = {};
      for (const e of p.envList) {
        if (e.key.trim()) envPairs[e.key.trim()] = e.value;
      }
      if (Object.keys(envPairs).length > 0) step.env = envPairs;
      if (p.root) step.root = true;
      return step;
    });
  if (steps.length > 0) config.provision = steps;

  // [storage] — flat builtin toggles, engine declaration order first.
  const encache = (enabled: string[]): V | undefined => {
    if (enabled.length === 0) return undefined;
    const out: V = {};
    for (const c of enabled) out[c] = true;
    return out;
  };
  const storage: V = {};
  const shared = encache(s.sharedCaches);
  const host = encache(s.hostCaches);
  if (shared) storage.shared_caches = shared;
  if (host) storage.host_caches = host;
  if (Object.keys(storage).length > 0) config.storage = storage;

  // [security] — the "default" tokens mean "unset" and are dropped here.
  const security: V = {
    security_label_disable: s.secLabelDisable,
    no_new_privileges: s.noNewPrivileges,
    read_only_rootfs: s.readOnlyRootfs,
    cap_preset: s.capPreset,
  };
  if (s.apparmor) security.apparmor = s.apparmor;
  if (s.seccomp !== LEAVE_UNSET.seccomp) security.seccomp = s.seccomp;
  if (s.usernsMode !== LEAVE_UNSET.userns) security.userns = s.usernsMode;
  if (s.extraCapAddList.length > 0) security.cap_add = s.extraCapAddList;
  const secrets = s.secrets.filter((sec) => sec.name.trim());
  if (secrets.length > 0) {
    const allBare = secrets.every(isBareSecret);
    security.secrets = secrets.map((sec) => secretToConfig(sec, allBare));
  }
  config.security = security;

  // [network]
  config.network = {
    mode: s.netMode,
    offline: s.netOffline,
    ports: s.portMappingsList,
  };

  // [integration] — every flag is opt-in, so false entries are skipped by
  // the serializer and can ride along.
  const integration: V = {
    git_identity: s.intGitIdentity,
    wayland: s.intWayland,
    audio: s.intAudio,
    dbus: s.intDbus,
    notify: s.intNotify,
    clipboard: s.intClipboard,
    sync_fonts: s.intSyncFonts,
    sync_icons: s.intSyncIcons,
    sync_themes: s.intSyncThemes,
    ssh_agent: s.intSshAgent,
    gpg_agent: s.intGpgAgent,
    hardware: {
      kvm: s.hwKvm,
      joystick: s.hwJoystick,
      webcam: s.hwWebcam,
      serial: s.hwSerial,
      usb: s.hwUsb,
      yubikey: s.hwYubikey,
    },
    export: {},
  };
  // gpu accepts bare bools or "auto"/"nvidia"; "auto" is the engine default.
  integration.gpu = s.intGpu === 'true' ? true : s.intGpu === 'false' ? false : s.intGpu;
  // `xdg_open = true` is the common shape; a custom scheme allowlist —
  // anything that is not exactly the engine default — needs the table form.
  // An empty list means the engine default.
  const isDefaultSchemes = (list: string[]): boolean =>
    list.length === 0 || (list.length === DEFAULT_XDG_OPEN_SCHEMES.length && DEFAULT_XDG_OPEN_SCHEMES.every((c, i) => list[i] === c));
  const schemes = s.intXdgOpenSchemes.filter((c) => c.trim()).map((c) => c.trim());
  if (schemes.length === 0) {
    if (s.intXdgOpen) integration.xdg_open = true;
  } else if (!isDefaultSchemes(schemes)) {
    integration.xdg_open = { enabled: s.intXdgOpen, allowed_schemes: schemes };
  }
  if (s.hostExecEnabled) {
    const hostExec: V = { enabled: true };
    const allowList: V = {};
    for (const e of s.hostExecList) {
      if (e.alias.trim() && e.path.trim()) allowList[e.alias.trim()] = e.path.trim();
    }
    if (Object.keys(allowList).length > 0) hostExec.allowlist = allowList;
    integration.host_exec = hostExec;
  }
  const xdgDirs: V = {};
  for (const [key, state] of [
    ['documents', s.xdgDocuments],
    ['downloads', s.xdgDownloads],
    ['pictures', s.xdgPictures],
    ['music', s.xdgMusic],
    ['videos', s.xdgVideos],
    ['desktop', s.xdgDesktop],
    ['projects', s.xdgProjects],
  ] as const) {
    xdgDirs[key] = xdgDirValue(state);
  }
  integration.xdg_dirs = xdgDirs;
  const appList = s.exportAppsList.filter((p) => p.trim());
  const binList = s.exportBinsList.filter((p) => p.trim());
  if (appList.length > 0) integration.export.apps = appList;
  if (binList.length > 0) integration.export.bins = binList;
  config.integration = integration;

  // [lifecycle]
  config.lifecycle = {
    quadlet: s.lifeQuadlet,
    autostart: s.lifeAutostart,
    on_stop: s.lifeOnStop,
    auto_update: s.lifeAutoUpdate,
    auto_checkpoint: s.lifeAutoCheckpoint,
    idle_timeout: s.lifeIdleTimeout,
  };

  // [systemd] — empty lists are skipped by the serializer.
  config.systemd = {
    requires: splitList(s.sysRequires),
    after: splitList(s.sysAfter),
  };

  // [dbus] — proxies with the proxy integration off is dead config.
  if (s.intDbus) {
    const dbus: V = {};
    if (s.dbusPreset !== LEAVE_UNSET.dbusPreset) dbus.preset = s.dbusPreset;
    if (s.dbusTalkList.length > 0) dbus.talk = s.dbusTalkList;
    if (s.dbusOwnList.length > 0) dbus.own = s.dbusOwnList;
    config.dbus = dbus;
  }

  // [wayland] — same gating; the firewall switch defaults to on.
  if (s.intWayland) {
    const wayland: V = { firewall: s.waylandFirewall };
    if (s.waylandBlockedList.length > 0) wayland.blocked_interfaces = s.waylandBlockedList;
    config.wayland = wayland;
  }

  return config;
}

/**
 * state -> TOML in one step: `configToToml` covers unknown/unbuildable
 * shapes by logging and returning null, which callers treat as an empty
 * draft rather than a crash.
 */
export function emittedToml(values: StudioValues): string {
  return configToToml(stateToConfig(values)) ?? '';
}

/** True when the entry is exactly what `secrets = ["name"]` expands to. */
function isBareSecret(s: SecretItem): boolean {
  return s.secretType === 'env' && !s.target.trim() && !s.mode.trim() && s.source === 'podman';
}
