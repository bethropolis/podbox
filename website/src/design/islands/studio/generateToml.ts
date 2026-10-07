// Studio TOML emission, driven by studio/schema.ts.
//
// Scalar fields come straight from FIELDS (one line per field, no per-key
// code); only genuinely custom shapes have hand-written emitters below.
// Tables render in TABLE_ORDER and are omitted when empty, so sections
// appear exactly when the user configures them.

import type { StudioValues } from './useStudioState';
import { isBareSecret } from './types';
import {
  FIELDS,
  CACHE_ORDER,
  tableOf,
  keyOf,
  shouldEmit,
  formatScalar,
} from './schema';

type V = Record<string, any>;

const TABLE_ORDER = [
  'image',
  'image.packages',
  'image.run',
  'container',
  'container.mounts',
  'container.services',
  'dotfiles',
  'storage.shared_caches',
  'storage.host_caches',
  'security',
  'network',
  'integration',
  'integration.hardware',
  'integration.host_exec',
  'integration.xdg_dirs',
  'integration.export',
  'lifecycle',
  'systemd',
  'dbus',
  'wayland',
];

// Tables that only render when their feature toggle is on (talk/own lists
// without dbus, or firewall rules without wayland, would be dead config).
const TABLE_GATES: Record<string, (s: StudioValues) => boolean> = {
  dbus: (s) => s.intDbus,
  wayland: (s) => s.intWayland,
};

function scalarLines(s: StudioValues, table: string): string[] {
  const vals = s as unknown as V;
  const lines: string[] = [];
  for (const f of FIELDS) {
    if (tableOf(f.tomlPath) !== table) continue;
    const v = vals[f.stateKey];
    if (!shouldEmit(f, v)) continue;
    lines.push(`${keyOf(f.tomlPath)} = ${formatScalar(f.kind, v)}`);
  }
  return lines;
}

// ── custom shapes ─────────────────────────────────────────────────────────

function customLines(s: StudioValues, table: string): string[] {
  switch (table) {
    case 'image': {
      // Preset distros are plain base refs; `preset` is the legacy Studio key.
      return [`base = "${s.imageType === 'preset' ? s.selectedPresetDistro : s.customImageBase}"`];
    }
    case 'image.run': {
      const cmds = s.runCommands.filter((r) => r.trim());
      if (cmds.length === 0) return [];
      return ['commands = [', ...cmds.map((r) => `  "${r.trim()}",`), ']'];
    }
    case 'container': {
      const valid = s.envVars.filter((e) => e.key.trim());
      if (valid.length === 0) return [];
      // Inline tables must stay on one line: a multi-line `env = {` is not
      // valid TOML and the engine rejects the whole file. `forward` lives in
      // the same table — the engine flattens the string entries and names
      // this one explicitly.
      const pairs = valid.map((e) => `${e.key} = "${e.value}"`);
      const forward = s.envForward.map((f) => f.trim()).filter(Boolean);
      if (forward.length > 0) {
        pairs.push(`forward = [${forward.map((f) => `"${f}"`).join(', ')}]`);
      }
      return pairs.length === 0 ? [] : [`env = { ${pairs.join(', ')} }`];
    }
    case 'container.mounts': {
      const valid = s.extraMounts.filter((m) => m.host.trim() && m.guest.trim());
      if (valid.length === 0) return [];
      return ['extra = [', ...valid.map((m) => `  "${m.host}:${m.guest}:${m.mode || 'z'}",`), ']'];
    }
    case 'container.services': {
      const valid = s.services.filter((svc) => svc.name.trim() && svc.command.trim());
      if (valid.length === 0) return [];
      const lines: string[] = [];
      for (const svc of valid) {
        const restart = svc.restart || 'on-failure';
        if (restart === 'on-failure') {
          lines.push(`${svc.name.trim()} = "${svc.command.trim()}"`);
        } else {
          lines.push(`[${table}.${svc.name.trim()}]`);
          lines.push(`command = "${svc.command.trim()}"`);
          lines.push(`restart = "${restart}"`);
        }
      }
      return lines;
    }
    case 'storage.shared_caches':
      return cacheLines(s.sharedCaches);
    case 'storage.host_caches':
      return cacheLines(s.hostCaches);
    case 'integration': {
      // gpu accepts bare bools or "auto"/"nvidia" strings in the schema.
      const g = s.intGpu;
      return [`gpu = ${g === 'true' || g === 'false' ? g : `"${g}"`}`];
    }
    case 'integration.host_exec': {
      if (!s.hostExecEnabled) return [];
      const lines = ['enabled = true'];
      const valid = s.hostExecList.filter((e) => e.alias.trim() && e.path.trim());
      if (valid.length > 0) {
        lines.push(`allowlist = { ${valid.map((e) => `${e.alias} = "${e.path}"`).join(', ')} }`);
      }
      return lines;
    }
    case 'security': {
      // `secrets` is a list of strings or a list of tables, never both, so the
      // shorthand form is only usable when every entry is bare.
      const valid = s.secrets.filter((sec) => sec.name.trim());
      if (valid.length === 0) return [];
      if (valid.every(isBareSecret)) {
        return [`secrets = [${valid.map((sec) => `"${sec.name.trim()}"`).join(', ')}]`];
      }
      const lines: string[] = [];
      for (const sec of valid) {
        lines.push(`[[${table}.secrets]]`);
        lines.push(`name = "${sec.name.trim()}"`);
        if (sec.secretType !== 'env') lines.push(`type = "${sec.secretType}"`);
        if (sec.source !== 'podman') lines.push(`source = "${sec.source}"`);
        if (sec.target.trim()) lines.push(`target = "${sec.target.trim()}"`);
        if (sec.mode.trim()) lines.push(`mode = "${sec.mode.trim()}"`);
      }
      return lines;
    }
    case 'systemd': {
      const reqList = s.sysRequires.split(',').map((r) => r.trim()).filter(Boolean);
      const afterList = s.sysAfter.split(',').map((a) => a.trim()).filter(Boolean);
      const lines: string[] = [];
      if (reqList.length > 0) lines.push(`requires = [${reqList.map((r) => `"${r}"`).join(', ')}]`);
      // sysAfter defaults to network-online.target; a custom value (or an
      // explicit empty, meaning "no ordering") is emitted.
      if (reqList.length > 0 || s.sysAfter !== 'network-online.target') {
        if (afterList.length > 0) lines.push(`after = [${afterList.map((a) => `"${a}"`).join(', ')}]`);
      }
      return lines;
    }
    case 'wayland': {
      if (!s.intWayland) return [];
      return s.waylandFirewall ? [] : ['firewall = false'];
    }
    default:
      return [];
  }
}

function cacheLines(enabled: string[]): string[] {
  return CACHE_ORDER.filter((c) => enabled.includes(c)).map((c) => `${c} = true`);
}

export function generateStudioToml(s: StudioValues): string {
  let t = `# podbox.toml — generated by podbox Studio\n\n`;
  let first = true;
  for (const table of TABLE_ORDER) {
    if (TABLE_GATES[table] && !TABLE_GATES[table](s)) continue;
    const lines = [...scalarLines(s, table), ...customLines(s, table)];
    if (lines.length === 0) continue;
    if (!first) t += `\n`;
    first = false;
    t += `[${table}]\n`;
    t += `${lines.join('\n')}\n`;
  }
  return t;
}
