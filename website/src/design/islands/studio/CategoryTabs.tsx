import React, { useEffect, useRef } from 'react';
import {
  Box,
  Boxes,
  Database,
  FolderGit2,
  ShieldCheck,
  Network,
  Monitor,
  RefreshCw,
  Radio,
} from 'lucide-react';
import type { StudioState } from './useStudioState';
import { CATEGORIES } from './schema';
import { useHorizontalWheel } from './useHorizontalWheel';

const icons: Record<string, typeof Box> = {
  image: Box,
  container: Boxes,
  dotfiles: FolderGit2,
  storage: Database,
  security: ShieldCheck,
  network: Network,
  integration: Monitor,
  lifecycle: RefreshCw,
  proxies: Radio,
};

const categories = CATEGORIES.map((cat) => ({ ...cat, icon: icons[cat.id] ?? Box }));

export function CategoryTabs({ st, errorMap, advisoryMap }: { st: Pick<StudioState, 'activeCategory' | 'setActiveCategory' | 'isFullscreen'>; errorMap?: Record<string, string>; advisoryMap?: Record<string, string> }) {
  const stripRef = useRef<HTMLDivElement>(null);
  useHorizontalWheel(stripRef);

  // The strip has no visible scrollbar, so keep the active tab in view when
  // something other than a click changes it (validation banner, preset, Esc).
  useEffect(() => {
    stripRef.current
      ?.querySelector('[data-active="true"]')
      ?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  }, [st.activeCategory]);

  return (
<div ref={stripRef} className={`shrink-0 bg-[var(--bg-crust)] border-b border-[var(--border)] px-2.5 flex items-center gap-1 overflow-x-auto scrollbar-none ${st.isFullscreen ? 'py-1' : 'py-1.5'}`}>
  {categories.map((cat) => {
    const Icon = cat.icon;
    const isActive = st.activeCategory === cat.id;
    // `[[provision]]` steps render inside the dotfiles panel, so their
    // `provision[i].*` fields badge that tab.
    const owns = (f: string) =>
      f === cat.id ||
      f.startsWith(`${cat.id}.`) ||
      (cat.id === 'proxies' && (f === 'dbus' || f.startsWith('dbus.') || f === 'wayland' || f.startsWith('wayland.'))) ||
      (cat.id === 'dotfiles' && (f === 'provision' || f.startsWith('provision[') || f.startsWith('provision.')));
    const hasError = !!errorMap && Object.keys(errorMap).some(owns);
    // Advisories (e.g. host_exec enabled without an allowlist) are still
    // worth flagging on the tab, just not in alarm red.
    const hasAdvisory = !hasError && !!advisoryMap && Object.keys(advisoryMap).some(owns);
    return (
      <button
        key={cat.id}
        data-active={isActive}
        onClick={() => st.setActiveCategory(cat.id as any)}
        type="button"
        className={`px-3 py-1.5 rounded-[2px] text-xs font-mono transition-colors flex items-center gap-1.5 whitespace-nowrap cursor-pointer shrink-0 ${
          isActive
            ? 'bg-[var(--bg-surface0)] text-[var(--accent-mauve)] font-bold shadow-xs'
            : 'text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-surface0)]/50'
        }`}
      >
        <Icon className="w-3.5 h-3.5" />
        <span>{cat.label}</span>
        {hasError && (
          <span className="w-1.5 h-1.5 rounded-full bg-[var(--accent-red)] inline-block ml-1" title="This section has validation errors" />
        )}
        {hasAdvisory && (
          <span className="w-1.5 h-1.5 rounded-full bg-[var(--accent-yellow)] inline-block ml-1" title="This section needs attention before podbox will accept it" />
        )}
      </button>
    );
  })}
</div>
  );
}
