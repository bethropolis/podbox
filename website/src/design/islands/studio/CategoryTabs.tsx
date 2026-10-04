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
  Layers3,
} from 'lucide-react';
import type { StudioState } from './useStudioState';
import { CATEGORIES } from './schema';

const icons: Record<string, typeof Box> = {
  image: Box,
  container: Boxes,
  dotfiles: FolderGit2,
  storage: Database,
  security: ShieldCheck,
  network: Network,
  integration: Monitor,
  lifecycle: RefreshCw,
  dbus: Radio,
  wayland: Layers3,
};

const categories = CATEGORIES.map((cat) => ({ ...cat, icon: icons[cat.id] ?? Box }));

export function CategoryTabs({ st, errorMap }: { st: Pick<StudioState, 'activeCategory' | 'setActiveCategory'>; errorMap?: Record<string, string> }) {
  const stripRef = useRef<HTMLDivElement>(null);

  // Translate vertical wheel into horizontal scroll while hovering the
  // strip. Native listener (non-passive) so preventDefault actually
  // applies; at either edge the event propagates and the page scrolls.
  useEffect(() => {
    const el = stripRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      if (Math.abs(e.deltaY) <= Math.abs(e.deltaX)) return;
      const max = el.scrollWidth - el.clientWidth;
      if (max <= 0) return;
      const next = el.scrollLeft + e.deltaY;
      if (next < 0 || next > max) return;
      e.preventDefault();
      el.scrollLeft = next;
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  }, []);

  return (
<div ref={stripRef} className="shrink-0 bg-[var(--bg-crust)] border-b border-[var(--border)] px-2.5 py-1.5 flex items-center gap-1 overflow-x-auto scrollbar-none">
  {categories.map((cat) => {
    const Icon = cat.icon;
    const isActive = st.activeCategory === cat.id;
    const hasError = !!errorMap && Object.keys(errorMap).some((f) => f === cat.id || f.startsWith(`${cat.id}.`));
    return (
      <button
        key={cat.id}
        onClick={() => st.setActiveCategory(cat.id as any)}
        type="button"
        className={`px-3 py-1.5 rounded-[2px] text-xs font-mono transition-all flex items-center gap-1.5 whitespace-nowrap cursor-pointer shrink-0 ${
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
      </button>
    );
  })}
</div>
  );
}
