import React from 'react';
import {
  Database,
  Plus,
  Trash2,
} from 'lucide-react';
import { StudioTooltip } from '../../../components/StudioTooltip';
import { CACHE_DESCRIPTIONS, CACHE_ORDER } from '../schema';
import type { StudioState } from '../useStudioState';

type CacheMode = 'off' | 'shared' | 'host';

const MODE_OPTIONS: { id: CacheMode; label: string; title: string }[] = [
  { id: 'off', label: 'off', title: 'Not persisted' },
  { id: 'shared', label: 'shared', title: 'Podbox-managed named volume (podbox-cache-<name>), shared between podbox containers' },
  { id: 'host', label: 'host', title: "Bind-mount the cache directory from your host home, reusing work done outside the container" },
];

const ACTIVE_CLASS: Record<CacheMode, string> = {
  off: 'bg-[var(--bg-surface1)] text-[var(--text-primary)]',
  shared: 'bg-[var(--accent-mauve)] text-[var(--bg-crust)] font-bold',
  host: 'bg-[var(--accent-blue)] text-[var(--bg-crust)] font-bold',
};

// One row per cache with a three-way choice, instead of two near-identical
// grids of toggles: shared and host are mutually exclusive by construction.
function ModeControl({ mode, onChange }: { mode: CacheMode; onChange: (m: CacheMode) => void }) {
  return (
    <div className="flex items-center gap-0.5 p-0.5 rounded-[2px] bg-[var(--bg-mantle)] border border-[var(--border)] shrink-0">
      {MODE_OPTIONS.map((o) => (
        <button
          key={o.id}
          type="button"
          onClick={() => onChange(o.id)}
          title={o.title}
          aria-pressed={mode === o.id}
          className={`px-1.5 py-0.5 rounded-[1px] text-[10px] font-mono uppercase tracking-tight transition-colors cursor-pointer ${
            mode === o.id ? ACTIVE_CLASS[o.id] : 'text-[var(--text-muted)] hover:text-[var(--text-primary)]'
          }`}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

type StoragePanelProps = Pick<StudioState, 'extraMounts' | 'hostCaches' | 'setExtraMounts' | 'setHostCaches' | 'setSharedCaches' | 'sharedCaches'>;

export function StoragePanel({ st }: { st: StoragePanelProps }) {
  const { extraMounts, hostCaches, setExtraMounts, setHostCaches, setSharedCaches, sharedCaches } = st;

  // Rebuilding from a Set keeps the two lists exclusive and in the canonical
  // BUILTIN_CACHES order, so the emitted TOML never depends on click order.
  const setCacheMode = (name: string, mode: CacheMode) => {
    const shared = new Set(sharedCaches.filter((c) => c !== name));
    const host = new Set(hostCaches.filter((c) => c !== name));
    if (mode === 'shared') shared.add(name);
    if (mode === 'host') host.add(name);
    setSharedCaches(CACHE_ORDER.filter((c) => shared.has(c)));
    setHostCaches(CACHE_ORDER.filter((c) => host.has(c)));
  };

  const enabled = sharedCaches.length + hostCaches.length;

  return (
<div className="space-y-5 animate-fadeIn">
  <div className="pb-3 border-b border-[var(--border)]">
    <div className="flex items-center gap-2">
      <Database className="w-4 h-4 text-[var(--accent-mauve)]" />
      <h2 className="text-sm font-bold text-[var(--text-primary)] font-mono">
        [storage] — Caches &amp; Mounts
      </h2>
    </div>
    <p className="text-xs text-[var(--text-subtext)] mt-1 font-sans">
      Persistent caches that survive rebuilds, plus extra host bind mounts. Shared caches are podbox-managed named volumes; host caches bind-mount your existing host directories.
    </p>
  </div>

  <div className="space-y-2">
    <div className="flex items-center justify-between">
      <div className="flex items-center">
        <span className="text-xs font-medium text-[var(--text-subtext)]">Caches</span>
        <StudioTooltip
          section="[storage.shared_caches]"
          title="cargo = true"
          description="Each cache is off, a podbox-managed volume shared between podbox containers, or a bind mount of the directory that already lives on your host."
          quadlet="Volume=podbox-cache-cargo:/home/user/.cargo/registry"
        />
      </div>
      <span className="text-[11px] font-mono text-[var(--text-muted)]">
        {enabled === 0 ? 'none enabled' : `${sharedCaches.length} shared · ${hostCaches.length} host`}
      </span>
    </div>

    <div className="divide-y divide-[var(--border)] rounded-[2px] border border-[var(--border)] overflow-hidden">
      {CACHE_ORDER.map((name) => {
        const mode: CacheMode = sharedCaches.includes(name)
          ? 'shared'
          : hostCaches.includes(name)
            ? 'host'
            : 'off';
        return (
          <div
            key={name}
            className="flex items-center justify-between gap-3 px-2.5 py-1 hover:bg-[var(--bg-surface0)]/40 transition-colors"
          >
            <div className="flex items-baseline gap-2 min-w-0">
              <span
                className={`text-xs font-mono font-medium shrink-0 ${
                  mode === 'off' ? 'text-[var(--text-muted)]' : 'text-[var(--text-primary)]'
                }`}
              >
                {name}
              </span>
              <span className="text-[11px] text-[var(--text-muted)] truncate hidden md:block">
                {CACHE_DESCRIPTIONS[name]}
              </span>
            </div>
            <ModeControl mode={mode} onChange={(m) => setCacheMode(name, m)} />
          </div>
        );
      })}
    </div>
  </div>

  {/* Extra Host Mounts (moved from [container]) */}
  <div className="space-y-2 pt-2 border-t border-[var(--border)]">
    <div className="flex items-center justify-between">
      <div className="flex items-center">
        <span className="text-xs font-medium text-[var(--text-subtext)]">Extra Host Mounts</span>
        <StudioTooltip
          section="[container.mounts]"
          title="extra = [&quot;host:guest:mode&quot;]"
          description="Bind mounts sharing directory trees between host and guest. Use :z or :Z for SELinux relabeling."
          quadlet="Volume=%h/Projects:/home/user/Projects:z"
        />
      </div>
      <button
        type="button"
        onClick={() =>
          setExtraMounts([
            ...extraMounts,
            { host: '~/Downloads', guest: '/home/user/Downloads', mode: 'z' },
          ])
        }
        className="text-xs text-[var(--accent-mauve)] hover:text-white flex items-center gap-1 cursor-pointer"
      >
        <Plus className="w-3 h-3" />
        <span>Add Mount</span>
      </button>
    </div>

    <div className="space-y-2">
      {extraMounts.map((m, idx) => (
        <div key={idx} className="flex items-center gap-2">
          <input
            type="text"
            value={m.host}
            aria-label={`Mount ${idx + 1} host path`}
            onChange={(e) => {
              const updated = [...extraMounts];
              updated[idx].host = e.target.value;
              setExtraMounts(updated);
            }}
            placeholder="Host path (e.g. ~/Projects)"
            className="flex-1 px-2.5 py-1.5 text-xs font-mono rounded-[2px] bg-[var(--bg-mantle)] border border-[var(--border)] text-[var(--text-primary)] focus:outline-none focus:border-[var(--accent-mauve)]"
          />
          <span className="text-[var(--text-muted)] text-xs font-mono">→</span>
          <input
            type="text"
            value={m.guest}
            aria-label={`Mount ${idx + 1} container path`}
            onChange={(e) => {
              const updated = [...extraMounts];
              updated[idx].guest = e.target.value;
              setExtraMounts(updated);
            }}
            placeholder="Guest path"
            className="flex-1 px-2.5 py-1.5 text-xs font-mono rounded-[2px] bg-[var(--bg-mantle)] border border-[var(--border)] text-[var(--text-primary)] focus:outline-none focus:border-[var(--accent-mauve)]"
          />
          <button
            type="button"
            onClick={() => setExtraMounts(extraMounts.filter((_, i) => i !== idx))}
            className="p-1.5 text-[var(--text-muted)] hover:text-[var(--accent-red)] rounded-[2px] cursor-pointer"
            title="Remove mount"
          >
            <Trash2 className="w-3.5 h-3.5" />
          </button>
        </div>
      ))}
    </div>
  </div>
</div>
  );
}
