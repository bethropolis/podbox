import type { StudioValues } from './useStudioState';
import type { MountItem, EnvVarItem, HostExecItem } from './types';

type Patch = Partial<StudioValues>;

const str = (v: unknown): string | undefined => (typeof v === 'string' ? v : undefined);
const bool = (v: unknown): boolean | undefined => (typeof v === 'boolean' ? v : undefined);
const strArr = (v: unknown): string[] | undefined =>
  Array.isArray(v) ? v.filter((e): e is string => typeof e === 'string') : undefined;
const num = (v: unknown): number | undefined => (typeof v === 'number' ? v : undefined);

function parseMounts(list: string[] | undefined): MountItem[] | undefined {
  if (!list) return undefined;
  return list.flatMap((m) => {
    const parts = m.split(':');
    if (parts.length < 2) return [];
    const mode = parts.length > 2 ? parts.pop()! : 'z';
    const guest = parts.pop()!;
    const host = parts.join(':');
    if (!host.trim() || !guest.trim()) return [];
    return [{ host: host.trim(), guest: guest.trim(), mode: mode.trim() || 'z' }];
  });
}

function parseEnv(rec: unknown): EnvVarItem[] | undefined {
  if (!rec || typeof rec !== 'object') return undefined;
  return Object.entries(rec as Record<string, unknown>).map(([key, value]) => ({
    key,
    value: typeof value === 'string' ? value : String(value ?? ''),
  }));
}

function parseAllowlist(rec: unknown): HostExecItem[] | undefined {
  if (!rec || typeof rec !== 'object') return undefined;
  return Object.entries(rec as Record<string, unknown>)
    .filter(([, v]) => typeof v === 'string')
    .map(([alias, path]) => ({ alias, path: path as string }));
}

// Reverse of generateStudioToml: plain podbox.toml -> studio state patch.
// Unknown keys are ignored; missing keys keep current state.
export function tomlToPatch(doc: Record<string, any>): Patch {
  const patch: Patch = { activePreset: 'custom' };
  const sec = (name: string): Record<string, any> =>
    doc?.[name] && typeof doc[name] === 'object' ? doc[name] : {};

  const image = sec('image');
  if (str(image.preset) !== undefined) {
    patch.imageType = 'preset';
    patch.selectedPresetDistro = image.preset!;
  } else if (str(image.base) !== undefined) {
    patch.imageType = 'custom';
    patch.customImageBase = image.base!;
  }
  if (str(image.name) !== undefined) patch.imageName = image.name;
  if (str(image.prebuilt) !== undefined) patch.imagePrebuiltRef = image.prebuilt;
  if (num(image.pull_retry) !== undefined) patch.pullRetry = image.pull_retry;
  if (str(image.pull_retry_delay) !== undefined) patch.pullRetryDelay = image.pull_retry_delay;
  if (str(image.package_manager) !== undefined) patch.packageManager = image.package_manager;
  const pkgs = strArr(image.packages);
  if (pkgs) patch.packagesInstallList = pkgs;
  const rm = strArr(image.remove_packages);
  if (rm) patch.packagesRemoveList = rm;
  const run = strArr(image.run);
  if (run) patch.runCommands = run.join('\n');

  const container = sec('container');
  const cname = str(container.name);
  if (cname !== undefined) {
    const clean = cname.toLowerCase().replace(/[^a-z0-9_-]/g, '');
    patch.containerName = clean;
    if (str(image.name) === undefined) patch.imageName = clean;
    patch.containerHome = `~/containers/${clean}`;
  }
  if (str(container.home) !== undefined) patch.containerHome = container.home;
  if (str(container.shell) !== undefined) patch.containerShell = container.shell;
  if (str(container.memory) !== undefined) patch.containerMemory = container.memory;
  if (str(container.cpus) !== undefined) patch.containerCpus = container.cpus;
  if (str(container.reload_cmd) !== undefined) patch.containerReloadCmd = container.reload_cmd;
  const mounts = parseMounts(strArr(container.mounts));
  if (mounts) patch.extraMounts = mounts;
  const env = parseEnv(container.env);
  if (env) patch.envVars = env;

  const security = sec('security');
  const apparmor = str(security.apparmor) ?? str((security.s as any)?.apparmor);
  if (apparmor !== undefined) patch.apparmor = apparmor;
  const seccomp = str(security.seccomp) ?? str((security.s as any)?.seccomp);
  if (seccomp !== undefined) patch.seccomp = seccomp;
  if (bool(security.security_label_disable) !== undefined)
    patch.secLabelDisable = security.security_label_disable;
  if (bool(security.no_new_privileges) !== undefined)
    patch.noNewPrivileges = security.no_new_privileges;
  if (bool(security.read_only_rootfs) !== undefined)
    patch.readOnlyRootfs = security.read_only_rootfs;
  if (str(security.userns) !== undefined) patch.usernsMode = security.userns;
  if (str(security.cap_preset) !== undefined) patch.capPreset = security.cap_preset;
  const capAdd = strArr(security.cap_add);
  if (capAdd) patch.extraCapAddList = capAdd;

  const network = sec('network');
  if (str(network.mode) !== undefined) patch.netMode = network.mode;
  const ports = strArr(network.ports);
  if (ports) patch.portMappingsList = ports;

  const integration = sec('integration');
  if (bool(integration.wayland) !== undefined) patch.intWayland = integration.wayland;
  if (bool(integration.audio) !== undefined) patch.intAudio = integration.audio;
  if (
    typeof integration.gpu === 'boolean' ||
    typeof integration.gpu === 'string'
  )
    patch.intGpu = String(integration.gpu);
  if (bool(integration.dbus) !== undefined) patch.intDbus = integration.dbus;
  if (bool(integration.notify) !== undefined) patch.intNotify = integration.notify;
  if (bool(integration.xdg_open) !== undefined) patch.intXdgOpen = integration.xdg_open;
  if (bool(integration.clipboard) !== undefined) patch.intClipboard = integration.clipboard;
  if (bool(integration.sync_fonts) !== undefined) patch.intSyncFonts = integration.sync_fonts;
  if (bool(integration.sync_icons) !== undefined) patch.intSyncIcons = integration.sync_icons;
  if (bool(integration.sync_themes) !== undefined) patch.intSyncThemes = integration.sync_themes;
  if (bool(integration.ssh_agent) !== undefined) patch.intSshAgent = integration.ssh_agent;
  if (bool(integration.gpg_agent) !== undefined) patch.intGpgAgent = integration.gpg_agent;

  const hostExec = sec('integration').host_exec as any;
  if (hostExec && typeof hostExec === 'object') {
    if (bool(hostExec.enabled) !== undefined) patch.hostExecEnabled = hostExec.enabled;
    const allow = parseAllowlist(hostExec.allowlist);
    if (allow) patch.hostExecList = allow;
  }
  const xdg = (sec('integration').xdg_dirs as any) ?? {};
  for (const [tomlKey, stateKey] of [
    ['documents', 'xdgDocuments'],
    ['downloads', 'xdgDownloads'],
    ['pictures', 'xdgPictures'],
    ['music', 'xdgMusic'],
    ['videos', 'xdgVideos'],
    ['desktop', 'xdgDesktop'],
    ['projects', 'xdgProjects'],
  ] as const) {
    if (bool(xdg[tomlKey]) !== undefined) (patch as any)[stateKey] = xdg[tomlKey];
  }
  const exp = (sec('integration').export as any) ?? {};
  const apps = strArr(exp.apps);
  if (apps) patch.exportAppsList = apps;
  const bins = strArr(exp.bins);
  if (bins) patch.exportBinsList = bins;

  const lifecycle = sec('lifecycle');
  if (bool(lifecycle.quadlet) !== undefined) patch.lifeQuadlet = lifecycle.quadlet;
  if (bool(lifecycle.autostart) !== undefined) patch.lifeAutostart = lifecycle.autostart;
  if (str(lifecycle.on_stop) !== undefined) patch.lifeOnStop = lifecycle.on_stop;
  if (bool(lifecycle.auto_update) !== undefined) patch.lifeAutoUpdate = lifecycle.auto_update;
  if (str(lifecycle.idle_timeout) !== undefined) patch.lifeIdleTimeout = lifecycle.idle_timeout;

  const systemd = sec('systemd');
  const requires = strArr(systemd.requires);
  if (requires) patch.sysRequires = requires.join(', ');
  const after = strArr(systemd.after);
  if (after) patch.sysAfter = after.join(', ');

  const dbus = sec('dbus');
  if (str(dbus.preset) !== undefined) {
    patch.dbusPreset = dbus.preset;
    if (dbus.preset === 'none') patch.intDbus = false;
  }
  const talk = strArr(dbus.talk);
  if (talk) patch.dbusTalkList = talk;
  const own = strArr(dbus.own);
  if (own) patch.dbusOwnList = own;

  const wayland = sec('wayland');
  if (bool(wayland.firewall) !== undefined) patch.waylandFirewall = wayland.firewall;
  const blocked = strArr(wayland.blocked_interfaces);
  if (blocked) patch.waylandBlockedList = blocked;

  return patch;
}
