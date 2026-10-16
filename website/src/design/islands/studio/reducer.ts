// Pure Studio state transitions, shared by the `useStudioState` hook and by
// `bun run check:studio`. Everything here is framework-free on purpose: the
// parity script drives the same reducer the UI does, so a key the reducer
// drops fails the check instead of silently losing user data (presets,
// Reset, import, share links and session restore all funnel through it).

import { STUDIO_DEFAULTS, UI_ONLY_KEYS, type StudioDefaults } from './schema';

export type StudioKey = keyof StudioDefaults;

/** `set` + capitalized key, e.g. `hwWebcam` -> `setHwWebcam`. */
export type SetterName<K extends StudioKey> = `set${Capitalize<K>}`;

export type StudioSetters = {
  [K in StudioKey as SetterName<K>]: (value: StudioDefaults[K]) => void;
};

export function setterName(key: StudioKey): SetterName<StudioKey> {
  return `set${key[0].toUpperCase()}${key.slice(1)}` as SetterName<StudioKey>;
}

export type StudioAction =
  | { type: 'set'; key: StudioKey; value: StudioDefaults[StudioKey] }
  | { type: 'patch'; patch: Record<string, unknown> }
  | { type: 'reset' };

const hasOwn = (obj: object, key: string): boolean =>
  Object.prototype.hasOwnProperty.call(obj, key);

/**
 * Merge `patch` over `state`, keeping only keys declared in
 * `STUDIO_DEFAULTS`. Unknown keys are ignored with a warning: they are
 * always a bug (a renamed panel key, a stale saved session), never data.
 */
export function studioReducer(state: StudioDefaults, action: StudioAction): StudioDefaults {
  switch (action.type) {
    case 'set':
      return { ...state, [action.key]: action.value };
    case 'patch': {
      const next = { ...state };
      for (const [key, value] of Object.entries(action.patch)) {
        if (hasOwn(STUDIO_DEFAULTS, key)) {
          (next as Record<string, unknown>)[key] = value;
        } else {
          console.warn(`[studio] ignoring unknown state key in patch: ${key}`);
        }
      }
      return next;
    }
    case 'reset': {
      // Reset restores configuration but keeps view state: resetting must
      // not close a dropdown or throw the user out of fullscreen.
      const next = { ...state };
      const uiOnly = new Set<string>(UI_ONLY_KEYS);
      for (const [key, value] of Object.entries(STUDIO_DEFAULTS)) {
        if (!uiOnly.has(key)) {
          (next as Record<string, unknown>)[key] = value;
        }
      }
      return next;
    }
  }
}
