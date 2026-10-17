// Studio TOML emission, driven by studio/schema.ts.
//
// Scalar fields come straight from FIELDS (one line per field, no per-key
// code); only genuinely custom shapes have hand-written emitters below.
// Tables render in TABLE_ORDER and are omitted when empty, so sections
// appear exactly when the user configures them.

import type { StudioValues } from './useStudioState';
import type { ProvisionItem } from './types';
import { isBareSecret } from './types';
import { PROVISION_DEFAULT_TIMEOUT } from './types';
import {
  FIELDS,
  CACHE_ORDER,
  tableOf,
  keyOf,
  shouldEmit,
  formatScalar,
} from './schema';
import { escapeTomlBasic, tomlKey, tomlString } from './toml';

type V = Record<string, any>;

const TABLE_ORDER = [
  'image',
  'image.packages',
  'image.run',
  'container',
  'container.mounts',
  'container.services',
  'dotfiles',
  'provision',
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
  // A step with a blank name or blank command cannot parse, so the section
  // only appears once the row is usable.
  provision: (s) => validProvisionSteps(s).length > 0,
};

const validProvisionSteps = (s: StudioValues): ProvisionItem[] =>
  s.provisionSteps.filter((p) => p.name.trim() && p.run.trim());

// Tables that write their own headers: `[[provision]]` is an array of tables,
// so a bare `[provision]` header would be a duplicate key.
const SELF_HEADED_TABLES = new Set(['provision']);

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
      return [`base = ${tomlString(s.imageType === 'preset' ? s.selectedPresetDistro : s.customImageBase)}`];
    }
    case 'image.run': {
      const cmds = s.runCommands.filter((r) => r.trim());
      if (cmds.length === 0) return [];
      return ['commands = [', ...cmds.map((r) => `  ${tomlString(r.trim())},`), ']'];
    }
    case 'container': {
      const valid = s.envVars.filter((e) => e.key.trim());
      if (valid.length === 0) return [];
      // Inline tables must stay on one line: a multi-line `env = {` is not
      // valid TOML and the engine rejects the whole file. `forward` lives in
      // the same table — the engine flattens the string entries and names
      // this one explicitly. Keys are quoted when they need it, so `my.var`
      // stays one key instead of becoming a nested table.
      const pairs = valid.map((e) => `${tomlKey(e.key)} = ${tomlString(e.value)}`);
      const forward = s.envForward.map((f) => f.trim()).filter(Boolean);
      if (forward.length > 0) {
        pairs.push(`forward = [${forward.map((f) => tomlString(f)).join(', ')}]`);
      }
      return pairs.length === 0 ? [] : [`env = { ${pairs.join(', ')} }`];
    }
    case 'container.mounts': {
      const valid = s.extraMounts.filter((m) => m.host.trim() && m.guest.trim());
      if (valid.length === 0) return [];
      return ['extra = [', ...valid.map((m) => `  ${tomlString(`${m.host}:${m.guest}:${m.mode || 'z'}`)},`), ']'];
    }
    case 'container.services': {
      const valid = s.services.filter((svc) => svc.name.trim() && svc.command.trim());
      if (valid.length === 0) return [];
      const lines: string[] = [];
      // Bare `name = "command"` lines first, detailed `[table.name]`
      // subtables after: TOML assigns a bare key to the last header seen,
      // so interleaving them would file later commands under the wrong
      // service.
      for (const svc of valid.filter((svc) => (svc.restart || 'on-failure') === 'on-failure')) {
        lines.push(`${tomlKey(svc.name.trim())} = ${tomlString(svc.command.trim())}`);
      }
      for (const svc of valid.filter((svc) => (svc.restart || 'on-failure') !== 'on-failure')) {
        const restart = svc.restart || 'on-failure';
        lines.push(`[${table}.${tomlKey(svc.name.trim())}]`);
        lines.push(`command = ${tomlString(svc.command.trim())}`);
        lines.push(`restart = ${tomlString(restart)}`);
      }
      return lines;
    }
    case 'storage.shared_caches':
      return cacheLines(s.sharedCaches);
    case 'storage.host_caches':
      return cacheLines(s.hostCaches);
    case 'integration': {
      // gpu accepts bare bools or "auto"/"nvidia" strings in the schema.
      // "auto" is the engine default — like every other default, it is not
      // written out.
      const g = s.intGpu;
      if (g === 'auto') return [];
      return [`gpu = ${g === 'true' || g === 'false' ? g : tomlString(g)}`];
    }
    case 'integration.host_exec': {
      if (!s.hostExecEnabled) return [];
      const lines = ['enabled = true'];
      const valid = s.hostExecList.filter((e) => e.alias.trim() && e.path.trim());
      if (valid.length > 0) {
        lines.push(`allowlist = { ${valid.map((e) => `${tomlKey(e.alias)} = ${tomlString(e.path)}`).join(', ')} }`);
      }
      return lines;
    }
    case 'security': {
      // `secrets` is a list of strings or a list of tables, never both, so the
      // shorthand form is only usable when every entry is bare.
      const valid = s.secrets.filter((sec) => sec.name.trim());
      if (valid.length === 0) return [];
      if (valid.every(isBareSecret)) {
        return [`secrets = [${valid.map((sec) => tomlString(sec.name.trim())).join(', ')}]`];
      }
      const lines: string[] = [];
      for (const sec of valid) {
        lines.push(`[[${table}.secrets]]`);
        lines.push(`name = ${tomlString(sec.name.trim())}`);
        if (sec.secretType !== 'env') lines.push(`type = ${tomlString(sec.secretType)}`);
        if (sec.source !== 'podman') lines.push(`source = ${tomlString(sec.source)}`);
        if (sec.target.trim()) lines.push(`target = ${tomlString(sec.target.trim())}`);
        if (sec.mode.trim()) lines.push(`mode = ${tomlString(sec.mode.trim())}`);
      }
      return lines;
    }
    case 'systemd': {
      const reqList = s.sysRequires.split(',').map((r) => r.trim()).filter(Boolean);
      const afterList = s.sysAfter.split(',').map((a) => a.trim()).filter(Boolean);
      const lines: string[] = [];
      if (reqList.length > 0) lines.push(`requires = [${reqList.map((r) => tomlString(r)).join(', ')}]`);
      // sysAfter defaults to network-online.target; a custom value (or an
      // explicit empty, meaning "no ordering") is emitted.
      if (reqList.length > 0 || s.sysAfter !== 'network-online.target') {
        if (afterList.length > 0) lines.push(`after = [${afterList.map((a) => tomlString(a)).join(', ')}]`);
      }
      return lines;
    }
    case 'wayland': {
      if (!s.intWayland) return [];
      return s.waylandFirewall ? [] : ['firewall = false'];
    }
    case 'provision': {
      const lines: string[] = [];
      for (const step of validProvisionSteps(s)) {
        lines.push('[[provision]]');
        lines.push(`name = ${tomlString(step.name.trim())}`);
        lines.push(`run = ${tomlBlockString(step.run.trim())}`);
        // `5m` is the engine default — never written out.
        const timeout = step.timeout.trim();
        if (timeout && timeout !== PROVISION_DEFAULT_TIMEOUT) {
          lines.push(`timeout = ${tomlString(timeout)}`);
        }
        if (step.onFailure === 'abort') lines.push('on_failure = "abort"');
        const env = step.envList.filter((e) => e.key.trim());
        if (env.length > 0) {
          lines.push(`env = { ${env.map((e) => `${tomlKey(e.key.trim())} = ${tomlString(e.value)}`).join(', ')} }`);
        }
        if (step.root) lines.push('root = true');
      }
      return lines;
    }
    default:
      return [];
  }
}

/**
 * Quote a run script for TOML: single-line values stay `"..."`, multi-line
 * ones become a `"""` block with the delimiters on their own lines. The
 * newline right after the opener is trimmed by the parser; the one before
 * the closer becomes a trailing newline in the value — insignificant to
 * `sh -c`, and the trim-then-emit cycle is a stable fixed point.
 * Escaping up front means content can never terminate the delimiter early.
 */
function tomlBlockString(s: string): string {
  if (s.includes('\n')) return `"""\n${escapeTomlBasic(s)}\n"""`;
  return tomlString(s);
}

function cacheLines(enabled: string[]): string[] {
  // Canonical order first; names the engine knows but Studio doesn't list
  // (or a future cache) survive the round-trip after them instead of being
  // silently dropped.
  const known = CACHE_ORDER.filter((c) => enabled.includes(c));
  const extra = enabled.filter((c) => !CACHE_ORDER.includes(c));
  return [...known, ...extra].map((c) => `${tomlKey(c)} = true`);
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
    if (!SELF_HEADED_TABLES.has(table)) t += `[${table}]\n`;
    t += `${lines.join('\n')}\n`;
  }
  return t;
}
