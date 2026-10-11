// One-click fixes for validation errors Studio can repair without guessing
// the user's intent.
//
// The banner carried diagnostics once; now field errors live inline next to
// their input. This module is the inline affordance: a fix only exists when
// the engine's rule pins down exactly one sensible replacement. Ambiguous
// cases ("abc" for a memory limit, a CPU count of 0) get no button rather
// than a wrong value.

export interface QuickFix {
  label: string;
  run: () => void;
}

/** `container.memory`: bare digits are the common typo ("2" for "2g"). */
export function memoryFix(value: string, set: (v: string) => void): QuickFix | null {
  const trimmed = value.trim();
  const digits = /^\d+/.exec(trimmed)?.[0];
  if (!digits) return null;
  const fixed = `${digits}g`;
  if (fixed === trimmed) return null;
  return { label: `use ${fixed}`, run: () => set(fixed) };
}

/** `container.cpus`: trailing junk ("2.5.1") or an unparsable prefix. */
export function cpusFix(value: string, set: (v: string) => void): QuickFix | null {
  const trimmed = value.trim();
  const prefix = /^\d+(\.\d+)?/.exec(trimmed)?.[0];
  const parsed = Number(prefix);
  if (!Number.isFinite(parsed) || parsed <= 0) return null;
  const fixed = parsed.toFixed(1);
  if (fixed === trimmed) return null;
  return { label: `use ${fixed}`, run: () => set(fixed) };
}

/**
 * `lifecycle.idle_timeout`: anything that is not `off` or `<digits><s|m|h>`.
 * "5min" collapses to the unit it carries; a bare number reads as minutes.
 */
export function idleTimeoutFix(value: string, set: (v: string) => void): QuickFix | null {
  const trimmed = value.trim();
  if (trimmed === 'off') return null;
  const digits = /^\d+/.exec(trimmed)?.[0];
  if (!digits) {
    return { label: 'use off', run: () => set('off') };
  }
  const unit = ['s', 'm', 'h'].find((u) => trimmed.includes(u));
  const fixed = unit ? `${digits}${unit}` : `${digits}m`;
  if (fixed === trimmed) return null;
  return { label: `use ${fixed}`, run: () => set(fixed) };
}

/** Rules exist but `integration.dbus` is off: the engine rejects them. */
export function dbusEnableFix(set: (v: boolean) => void): QuickFix {
  return { label: 'enable D-Bus', run: () => set(true) };
}

/** Firewall rules exist but `integration.wayland` is off. */
export function waylandEnableFix(set: (v: boolean) => void): QuickFix {
  return { label: 'enable Wayland', run: () => set(true) };
}
