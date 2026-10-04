import React from 'react';
import {
  Database,
  Plus,
  Trash2,
} from 'lucide-react';
import {
  StudioSwitch,
} from '../../../components/StudioControls';
import { StudioTooltip } from '../../../components/StudioTooltip';
import type { StudioState } from '../useStudioState';

const CACHE_ORDER = ['cargo', 'npm', 'pnpm', 'pip', 'uv', 'yarn', 'bun', 'composer', 'maven', 'gradle', 'ccache', 'go', 'rustup', 'mbx'];

const CACHE_DESCRIPTIONS: Record<string, string> = {
  cargo: 'Rust crate registry + git checkouts',
  npm: 'Node package cache',
  pnpm: 'pnpm content-addressable store',
  pip: 'Python wheel cache',
  uv: 'uv package cache',
  yarn: 'Yarn Classic + Berry caches',
  bun: 'Bun install cache',
  composer: 'PHP Composer cache',
  maven: 'Maven local repository',
  gradle: 'Gradle build caches',
  ccache: 'C/C++ compiler cache',
  go: 'Go module cache',
  rustup: 'Rust toolchain installs',
  mbx: 'Mr Boxington shared cache',
};

type StoragePanelProps = Pick<StudioState, 'extraMounts' | 'hostCaches' | 'setExtraMounts' | 'setHostCaches' | 'setSharedCaches' | 'sharedCaches'>;

export function StoragePanel({ st }: { st: StoragePanelProps }) {
  const { extraMounts, hostCaches, setExtraMounts, setHostCaches, setSharedCaches, sharedCaches } = st;

  const toggle = (list: string[], setList: (v: string[]) => void, name: string) => {
    setList(list.includes(name) ? list.filter((c) => c !== name) : [...list, name]);
  };

  const cacheSwitch = (
    name: string,
    enabled: boolean,
    onChange: (v: boolean) => void,
    kind: string,
  ) => (
    <StudioSwitch
      key={`${kind}-${name}`}
      id={`storage-${kind}-${name}`}
      checked={enabled}
      onChange={onChange}
      label={
        <div className="flex items-center">
          <span className="font-mono">{name}</span>
          <StudioTooltip
            section="[storage]"
            title={`${name} = true`}
            description={CACHE_DESCRIPTIONS[name] ?? 'Persistent cache volume'}
          />
        </div>
      }
      description={CACHE_DESCRIPTIONS[name]}
    />
  );

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
    <div className="flex items-center">
      <span className="text-xs font-medium text-[var(--text-subtext)]">Shared Caches (managed volumes)</span>
      <StudioTooltip
        section="[storage.shared_caches]"
        title="cargo = true"
        description="Podbox provisions a named volume per cache and mounts it into the container home."
        quadlet="Volume=podbox-cache-cargo:/home/user/.cargo/registry"
      />
    </div>
    <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
      {CACHE_ORDER.map((c) =>
        cacheSwitch(c, sharedCaches.includes(c), (v) => toggle(sharedCaches, setSharedCaches, c), 'shared'),
      )}
    </div>
  </div>

  <div className="space-y-2 pt-2 border-t border-[var(--border)]">
    <div className="flex items-center">
      <span className="text-xs font-medium text-[var(--text-subtext)]">Host Caches (bind mounts)</span>
      <StudioTooltip
        section="[storage.host_caches]"
        title="npm = true"
        description="Bind-mount the cache directory straight from your host home instead of a managed volume."
      />
    </div>
    <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
      {CACHE_ORDER.map((c) =>
        cacheSwitch(c, hostCaches.includes(c), (v) => toggle(hostCaches, setHostCaches, c), 'host'),
      )}
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
