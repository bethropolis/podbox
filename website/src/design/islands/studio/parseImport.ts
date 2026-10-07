// Reverse of generateStudioToml: parsed podbox.toml -> studio state patch.
//
// Scalar fields come straight from schema FIELDS (one line per field);
// only genuinely custom shapes and legacy-key back-compat have hand-written
// readers below. Unknown keys are ignored; missing keys keep current state.

import type { StudioValues } from './useStudioState';
import type { MountItem, EnvVarItem, HostExecItem, SecretItem, ServiceItem } from './types';
import { FIELDS, CACHE_ORDER, getPath, coerceScalar } from './schema';

type Patch = Partial<StudioValues>;

const str = (v: unknown): string | undefined => (typeof v === 'string' ? v : undefined);
const bool = (v: unknown): boolean | undefined => (typeof v === 'boolean' ? v : undefined);
const strArr = (v: unknown): string[] | undefined =>
  Array.isArray(v) ? v.filter((e): e is string => typeof e === 'string') : undefined;

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
  return Object.entries(rec as Record<string, unknown>)
    // `forward` is a list in the same table, not a KEY=value pair.
    .filter(([key, value]) => key !== 'forward' && typeof value === 'string')
    .map(([key, value]) => ({ key, value: value as string }));
}

/** `secrets = ["a"]` or `[[security.secrets]]` — both land in the same list. */
function parseSecrets(rec: unknown): SecretItem[] | undefined {
  if (!Array.isArray(rec)) return undefined;
  const out: SecretItem[] = [];
  for (const entry of rec) {
    if (typeof entry === 'string') {
      if (entry.trim()) out.push({ name: entry, secretType: 'env', target: '', mode: '', source: 'podman' });
      continue;
    }
    if (entry && typeof entry === 'object') {
      const t = entry as Record<string, unknown>;
      const name = str(t.name);
      if (!name) continue;
      out.push({
        name,
        secretType: t.type === 'mount' ? 'mount' : 'env',
        target: str(t.target) ?? '',
        mode: str(t.mode) ?? '',
        source: t.source === 'systemd' ? 'systemd' : 'podman',
      });
    }
  }
  return out.length > 0 ? out : undefined;
}

function parseAllowlist(rec: unknown): HostExecItem[] | undefined {
  if (!rec || typeof rec !== 'object') return undefined;
  return Object.entries(rec as Record<string, unknown>)
    .filter(([, v]) => typeof v === 'string')
    .map(([alias, path]) => ({ alias, path: path as string }));
}

function parseServices(rec: unknown): ServiceItem[] | undefined {
  if (!rec || typeof rec !== 'object') return undefined;
  const out: ServiceItem[] = [];
  for (const [name, svc] of Object.entries(rec as Record<string, unknown>)) {
    if (typeof svc === 'string') {
      if (name.trim() && svc.trim()) out.push({ name, command: svc, restart: 'on-failure' });
    } else if (svc && typeof svc === 'object') {
      const detail = svc as Record<string, unknown>;
      if (typeof detail.command === 'string' && detail.command.trim()) {
        out.push({
          name,
          command: detail.command,
          restart: typeof detail.restart === 'string' ? detail.restart : 'on-failure',
        });
      }
    }
  }
  return out;
}

// `[storage.shared_caches]` / `[storage.host_caches]` are flat name->bool
// tables; canonical order first, unknown names preserved after.
function parseCacheFlags(rec: unknown): string[] | undefined {
  if (!rec || typeof rec !== 'object') return undefined;
  const entries = rec as Record<string, unknown>;
  return CACHE_ORDER.filter((c) => entries[c] === true).concat(
    Object.keys(entries).filter((k) => !CACHE_ORDER.includes(k) && entries[k] === true),
  );
}

export function tomlToPatch(doc: Record<string, any>): Patch {
  const patch: Record<string, any> = { activePreset: 'custom' };
  const sec = (name: string): Record<string, any> =>
    doc?.[name] && typeof doc[name] === 'object' ? doc[name] : {};

  // Generic scalars: every FIELDS entry reads its TOML path.
  for (const f of FIELDS) {
    const coerced = coerceScalar(f.kind, getPath(doc, f.tomlPath));
    if (coerced !== undefined) patch[f.stateKey] = coerced;
  }

  const image = sec('image');
  // `base` selects the custom-URI mode; legacy `preset` selects a preset.
  // (The generic pass above ignores both: they share one state switch.)
  if (str(image.preset) !== undefined) {
    patch.imageType = 'preset';
    patch.selectedPresetDistro = image.preset!;
  } else if (str(image.base) !== undefined) {
    patch.imageType = 'custom';
    patch.customImageBase = image.base!;
  }
  // Legacy flat keys from older Studio exports.
  if (str(image.prebuilt) !== undefined && str(image.image) === undefined) {
    patch.imagePrebuiltRef = image.prebuilt;
  }
  if (str(image.package_manager) !== undefined) {
    const pkgs = sec('image').packages;
    if (!pkgs || typeof pkgs !== 'object' || (pkgs as Record<string, unknown>).manager === undefined) {
      patch.packageManager = image.package_manager;
    }
  }
  const pkgsRaw = image.packages;
  if (Array.isArray(pkgsRaw)) {
    const pkgs = strArr(pkgsRaw);
    if (pkgs) patch.packagesInstallList = pkgs;
  }
  if (Array.isArray(image.remove_packages)) {
    const rm = strArr(image.remove_packages);
    if (rm) patch.packagesRemoveList = rm;
  }
  const runRaw = image.run;
  if (Array.isArray(runRaw)) {
    const run = strArr(runRaw);
    if (run) patch.runCommands = run;
  } else if (runRaw && typeof runRaw === 'object' && !Array.isArray(runRaw)) {
    const run = strArr((runRaw as Record<string, unknown>).commands);
    if (run) patch.runCommands = run;
  }

  const container = sec('container');
  const cname = str(container.name);
  if (cname !== undefined) {
    const clean = cname.toLowerCase().replace(/[^a-z0-9_-]/g, '');
    patch.containerName = clean;
    if (str(image.name) === undefined) patch.imageName = clean;
    patch.containerHome = `~/containers/${clean}`;
  }
  if (str(container.home) !== undefined) patch.containerHome = container.home;
  const mountsObj =
    container.mounts && typeof container.mounts === 'object' && !Array.isArray(container.mounts)
      ? (container.mounts as Record<string, unknown>)
      : undefined;
  // Legacy flat `mounts` list.
  const mounts = parseMounts(strArr(mountsObj?.extra) ?? strArr(container.mounts));
  if (mounts) patch.extraMounts = mounts;
  const secrets = parseSecrets(sec('security').secrets);
  if (secrets) patch.secrets = secrets;

  const env = parseEnv(container.env);
  if (env) patch.envVars = env;
  const envRec = container.env as Record<string, unknown> | undefined;
  const forward = strArr(envRec?.forward);
  if (forward) patch.envForward = forward;
  const services = parseServices(container.services);
  if (services) patch.services = services;

  const storage = sec('storage');
  const sharedCaches = parseCacheFlags(storage.shared_caches);
  if (sharedCaches) patch.sharedCaches = sharedCaches;
  const hostCaches = parseCacheFlags(storage.host_caches);
  if (hostCaches) patch.hostCaches = hostCaches;

  const integration = sec('integration');
  // gpu accepts bare bools or "auto"/"nvidia"/"true"/"false" strings.
  if (typeof integration.gpu === 'boolean' || typeof integration.gpu === 'string') {
    patch.intGpu = String(integration.gpu);
  }
  const hostExec = integration.host_exec as any;
  if (hostExec && typeof hostExec === 'object') {
    if (bool(hostExec.enabled) !== undefined) patch.hostExecEnabled = hostExec.enabled;
    const allow = parseAllowlist(hostExec.allowlist);
    if (allow) patch.hostExecList = allow;
  }

  const dbus = sec('dbus');
  if (str(dbus.preset) !== undefined) {
    patch.dbusPreset = dbus.preset;
    if (dbus.preset === 'none') patch.intDbus = false;
  }

  // Comma-joined systemd lists (custom state shape).
  const systemd = sec('systemd');
  const requires = strArr(systemd.requires);
  if (requires) patch.sysRequires = requires.join(', ');
  const after = strArr(systemd.after);
  if (after) patch.sysAfter = after.join(', ');

  return patch as Patch;
}
