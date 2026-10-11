// Preset catalogue: one file per preset, shared types in `./shared`.
// Importers (`StudioHeader`, `check-studio-parity`) keep using this barrel.

import { STUDIO_DEFAULTS, UI_ONLY_KEYS } from './schema';
import type { StudioValues } from './useStudioState';
import { archGuiPreset } from './presets/arch-gui';
import { fullstackPreset } from './presets/fullstack';
import { gamingPreset } from './presets/gaming';
import { minimalPreset } from './presets/minimal';
import { rustPreset } from './presets/rust';
import type { PresetDef, PresetId } from './presets/shared';

export type { PresetDef, PresetId };

export const PRESETS: readonly PresetDef[] = [rustPreset, archGuiPreset, fullstackPreset, gamingPreset, minimalPreset];

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
