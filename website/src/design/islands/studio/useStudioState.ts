import { useEffect, useReducer, useRef, useState } from 'react';
import { shareFromHash, hasPendingSharedLoad } from './share';
import { initialStudioValues, UI_ONLY_KEYS, type StudioDefaults } from './schema';
import {
  studioReducer,
  setterName,
  type StudioKey,
  type StudioSetters,
} from './reducer';

// Bump the suffix when the value shape changes; older payloads are then
// ignored instead of half-applying stale keys.
const STORAGE_KEY = 'podbox-studio-v1';

// View state lives in its own key so a corrupted UI payload can never take
// the configuration down with it (and vice versa).
const UI_STORAGE_KEY = 'podbox-studio-ui-v1';
// Provision step collapse state has its own key for the same reason; see
// ProvisionPanel ('podbox-studio-provision-v1').
const PROVISION_STORAGE_KEY = 'podbox-studio-provision-v1';

// State keys whose shape changed after sessions were already saved.
const XDG_DIR_KEYS = [
  'xdgDocuments', 'xdgDownloads', 'xdgPictures', 'xdgMusic',
  'xdgVideos', 'xdgDesktop', 'xdgProjects',
];

function loadSaved(): Record<string, any> {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw);
    return parsed && typeof parsed === 'object' ? parsed : {};
  } catch {
    return {};
  }
}

// View state worth remembering across reloads: which tab and which preview
// were open. Everything transient (fullscreen, dropdowns, copy flashes)
// stays ephemeral on purpose. Unknown values are dropped, and a corrupted
// payload is ignored outright.
const KNOWN_CATEGORIES = [
  'image', 'container', 'security', 'network', 'integration',
  'lifecycle', 'proxies', 'dotfiles', 'storage',
] as const;
const KNOWN_VIEWS = ['toml', 'quadlet', 'containerfile', 'host', 'proxy', 'compositor'] as const;

function loadSavedUI(): {
  activeCategory?: (typeof KNOWN_CATEGORIES)[number];
  activeView?: (typeof KNOWN_VIEWS)[number];
} {
  try {
    if (typeof localStorage === 'undefined') return {};
    const raw = localStorage.getItem(UI_STORAGE_KEY);
    if (!raw) return {};
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return {};
    const rec = parsed as Record<string, unknown>;
    const out: ReturnType<typeof loadSavedUI> = {};
    if (typeof rec.activeCategory === 'string' && (KNOWN_CATEGORIES as readonly string[]).includes(rec.activeCategory)) {
      out.activeCategory = rec.activeCategory as (typeof KNOWN_CATEGORIES)[number];
    }
    if (typeof rec.activeView === 'string' && (KNOWN_VIEWS as readonly string[]).includes(rec.activeView)) {
      out.activeView = rec.activeView as (typeof KNOWN_VIEWS)[number];
    }
    return out;
  } catch {
    return {};
  }
}

export function useStudioState() {
  // One state object keyed exactly by the schema defaults. Every transition
  // goes through `studioReducer` (the same reducer `check:studio` drives),
  // so a key added to the defaults is automatically patched, persisted
  // and reset — there is no hand-maintained setters map left to drift.
  // Reducer construction is lazy on both the ordering and engine timing: by
  // the time this hook mounts, the wasm engine is up (StudioEntry gates on
  // it), so the wasm-backed defaults are safely readable.
  const [state, dispatch] = useReducer(studioReducer, undefined, () => initialStudioValues());

  const { isFullscreen, activeCategory, activeView } = state;

  // Keyboard shortcut for Esc when fullscreen and body scroll lock
  useEffect(() => {
    if (isFullscreen) {
      document.body.style.overflow = 'hidden';
    } else {
      document.body.style.overflow = '';
    }
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && isFullscreen) {
        dispatch({ type: 'set', key: 'isFullscreen', value: false });
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => {
      window.removeEventListener('keydown', handleKeyDown);
      document.body.style.overflow = '';
    };
  }, [isFullscreen]);

  const handleContainerNameChange = (val: string) => {
    const clean = val.toLowerCase().replace(/[^a-z0-9_-]/g, '');
    dispatch({
      type: 'patch',
      patch: { containerName: clean, imageName: clean, containerHome: `~/containers/${clean}` },
    });
  };

  // Per-key setters keep the panel APIs stable (`st.setHwWebcam(...)`).
  // They are derived from the schema defaults, so new keys arrive with their
  // setter instead of needing a second hand-written list.
  const setters = Object.fromEntries(
    (Object.keys(initialStudioValues()) as StudioKey[]).map((key) => [
      setterName(key),
      (value: unknown) =>
        dispatch({ type: 'set', key, value: value as StudioDefaults[StudioKey] }),
    ]),
  ) as StudioSetters;

  const set = <K extends StudioKey>(key: K, value: StudioDefaults[K]) =>
    dispatch({ type: 'set', key, value });

  const applyPatch = (patch: Record<string, any>) => {
    dispatch({ type: 'patch', patch });
  };

  const values = state;

  // Persist only configuration, never view state (a reload must not
  // re-open the fullscreen editor or a dropdown). The remembered view
  // (tab + preview) lives under UI_STORAGE_KEY instead.
  const UI_ONLY = new Set<string>(UI_ONLY_KEYS);
  const persistable = Object.fromEntries(
    Object.entries(values).filter(([k]) => !UI_ONLY.has(k))
  ) as Record<string, any>;

  const restoredRef = useRef(false);
  const [hasRestoredSession, setHasRestoredSession] = useState(false);
  const lastSaved = useRef('');

  // Restore once on mount. Declared before the save effects so the
  // restored values land before anything can be written back.
  useEffect(() => {
    const savedUI = loadSavedUI();
    if (savedUI.activeCategory) dispatch({ type: 'set', key: 'activeCategory', value: savedUI.activeCategory });
    if (savedUI.activeView) dispatch({ type: 'set', key: 'activeView', value: savedUI.activeView });
    // A shared link (#c=…) is this visit's entry point, so it is applied
    // instead of the saved session: opening someone else's config must
    // never inherit your own. Marking the session as restored here also
    // keeps the debounced save below from firing with stale values before
    // the shared config lands. An unreadable link reloads without the hash
    // (see StudioPage), which takes the normal path on that fresh visit.
    if (shareFromHash(window.location.hash)) {
      restoredRef.current = true;
      setHasRestoredSession(false);
      return;
    }
    const saved = loadSaved();
    if (Object.keys(saved).length > 0) {
      // Sessions saved before RUN commands became a list stored them as one
      // newline-joined string; split it back so old sessions survive.
      if (typeof saved.runCommands === 'string') {
        saved.runCommands = (saved.runCommands as string).split('\n').filter((c) => c.trim());
      }
      // XDG dirs were plain booleans before the read-write mode existed.
      for (const key of XDG_DIR_KEYS) {
        if (typeof saved[key] === 'boolean') {
          saved[key] = saved[key] ? 'ro' : 'off';
        }
      }
      applyPatch(saved);
      setHasRestoredSession(true);
    }
    restoredRef.current = true;
    // applyPatch is a stable-shape dispatch wrapper; re-running the restore
    // on identity change would double-apply the session.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Debounced write: the studio re-renders on every keystroke, so write
  // at most every 400ms and skip when the payload is unchanged. A visit
  // carrying a shared link holds defaults until that link lands; writing
  // them would erase the session the link is meant to be an alternative
  // to, so hold off until the load resolves.
  useEffect(() => {
    if (!restoredRef.current || hasPendingSharedLoad()) return;
    const timer = window.setTimeout(() => {
      try {
        const json = JSON.stringify(persistable);
        if (json === lastSaved.current) return;
        lastSaved.current = json;
        localStorage.setItem(STORAGE_KEY, json);
      } catch {
        // Private mode or quota exceeded: session just won't persist.
      }
    }, 400);
    return () => window.clearTimeout(timer);
    // persistable is rebuilt per render; the JSON compare inside is the
    // real change detection.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [state]);

  // View state writes straight through (no debounce): tabs and previews
  // change rarely, and the payload is two strings.
  useEffect(() => {
    if (!restoredRef.current || hasPendingSharedLoad()) return;
    try {
      localStorage.setItem(UI_STORAGE_KEY, JSON.stringify({ activeCategory, activeView }));
    } catch {
      // Private mode or quota exceeded: view just won't persist.
    }
  }, [activeCategory, activeView]);

  const clearSavedSession = () => {
    try {
      localStorage.removeItem(STORAGE_KEY);
      localStorage.removeItem(UI_STORAGE_KEY);
      localStorage.removeItem(PROVISION_STORAGE_KEY);
    } catch {}
    lastSaved.current = '';
    setHasRestoredSession(false);
  };

  return { ...state, ...setters, set, handleContainerNameChange, applyPatch, values, hasRestoredSession, clearSavedSession };
}

export type StudioState = ReturnType<typeof useStudioState>;
export type StudioValues = StudioState['values'];
