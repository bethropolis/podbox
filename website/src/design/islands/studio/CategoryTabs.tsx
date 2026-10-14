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
  const stripRef = useRef<HTMLElement>(null);
  useHorizontalWheel(stripRef);

  // The strip has no visible scrollbar, so keep the active tab in view when
  // something other than a click changes it (validation banner, preset, Esc).
  useEffect(() => {
    stripRef.current
      ?.querySelector('[data-active="true"]')
      ?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  }, [st.activeCategory]);

  return (
<nav
  ref={stripRef}
  aria-label="Settings sections"
  className={`shrink-0 bg-[var(--bg-mantle)] border-b border-[var(--border)] flex gap-1 overflow-x-auto scrollbar-none px-2 ${
    st.isFullscreen ? 'py-1' : 'py-1.5'
  } [mask-image:linear-gradient(to_right,transparent,black_12px,black_calc(100%-28px),transparent)]
    xl:w-52 xl:flex-col xl:gap-0.5 xl:overflow-x-visible xl:overflow-y-auto xl:border-b-0 xl:border-r xl:px-2 xl:py-2.5 xl:[mask-image:none]`}
>
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
        aria-current={isActive ? 'page' : undefined}
        onClick={() => st.setActiveCategory(cat.id as any)}
        type="button"
        title={`${cat.label} · ${cat.blurb}`}
        className={`group relative flex shrink-0 items-center gap-2 whitespace-nowrap rounded-md px-2.5 py-1.5 text-left text-[13px] transition-colors cursor-pointer xl:w-full xl:py-2 ${
          isActive
            ? 'bg-[var(--bg-surface0)] text-[var(--text-primary)] font-medium xl:before:absolute xl:before:left-0 xl:before:top-2 xl:before:bottom-2 xl:before:w-[2px] xl:before:rounded-full xl:before:bg-[var(--accent-mauve)]'
            : 'text-[var(--text-subtext)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-surface0)]/50'
        }`}
      >
        <Icon className={`w-4 h-4 shrink-0 ${isActive ? 'text-[var(--accent-mauve)]' : 'text-[var(--text-muted)]'}`} aria-hidden="true" />
        <span className="flex min-w-0 flex-col">
          <span>{cat.name}</span>
          <span className="hidden xl:block truncate text-[11px] font-normal text-[var(--text-muted)]">{cat.blurb}</span>
        </span>
        {hasError && (
          <span className="ml-auto w-1.5 h-1.5 rounded-full bg-[var(--accent-red)] shrink-0" title="This section has validation errors" />
        )}
        {hasAdvisory && (
          <span className="ml-auto w-1.5 h-1.5 rounded-full bg-[var(--accent-yellow)] shrink-0" title="This section needs attention before podbox will accept it" />
        )}
      </button>
    );
  })}
</nav>
  );
}
