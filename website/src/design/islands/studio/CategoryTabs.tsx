import React, { useEffect, useRef } from 'react';
import {
  Box,
  Boxes,
  ShieldCheck,
  Network,
  Monitor,
  RefreshCw,
  Radio,
  Layers3,
} from 'lucide-react';
import type { StudioState } from './useStudioState';

const categories = [
  { id: 'image', label: '[image]', icon: Box, count: 'Distro & Packages' },
  { id: 'container', label: '[container]', icon: Boxes, count: 'Resources & Shell' },
  { id: 'security', label: '[security]', icon: ShieldCheck, count: 'UserNS & Caps' },
  { id: 'network', label: '[network]', icon: Network, count: 'Pasta & Ports' },
  { id: 'integration', label: '[integration]', icon: Monitor, count: 'Wayland & GPU' },
  { id: 'lifecycle', label: '[lifecycle]', icon: RefreshCw, count: 'Quadlet & Boot' },
  { id: 'dbus', label: '[dbus]', icon: Radio, count: 'Proxy Rules' },
  { id: 'wayland', label: '[wayland]', icon: Layers3, count: 'Filter Protocol' },
];

export function CategoryTabs({ st }: { st: Pick<StudioState, 'activeCategory' | 'setActiveCategory'> }) {
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
      </button>
    );
  })}
</div>
  );
}
