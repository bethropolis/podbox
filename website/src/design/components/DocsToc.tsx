import React, { useEffect, useRef, useState } from 'react';
import { AlignLeft, ArrowUp } from 'lucide-react';

interface TocHeading {
  id: string;
  text: string;
  level: number;
}

// Headings come from the page at build time (same slugify the renderer
// uses), so the rail has content in SSR HTML too — the client only adds
// scroll-spy highlighting.
export function DocsToc({ headings: initial = [] }: { headings?: TocHeading[] }) {
  const [headings] = useState<TocHeading[]>(initial);
  const [activeId, setActiveId] = useState<string>(initial[0]?.id || '');
  // While a TOC jump is in flight, intermediate headings streaming
  // past the spy band must not steal the highlight. The lock holds
  // the tapped target until it arrives (or a fallback timeout fires).
  const jumpLock = useRef(false);
  const jumpTarget = useRef<string | null>(null);
  const lockTimer = useRef(0);

  useEffect(() => () => window.clearTimeout(lockTimer.current), []);

  // Scroll-spy via IntersectionObserver: zero per-scroll work, no
  // layout thrash on long pages. The top band below the sticky
  // navbar decides the active heading.
  useEffect(() => {
    if (headings.length === 0) return;
    const els = headings
      .map((h) => document.getElementById(h.id))
      .filter((el): el is HTMLElement => el !== null);
    if (els.length === 0) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (jumpLock.current) {
          const arrived = entries.some(
            (e) => e.isIntersecting && e.target.id === jumpTarget.current
          );
          if (!arrived) return;
          jumpLock.current = false;
          jumpTarget.current = null;
          window.clearTimeout(lockTimer.current);
          return;
        }
        const visible = entries.filter((e) => e.isIntersecting);
        if (visible.length === 0) return;
        visible.sort((a, b) =>
          a.target.compareDocumentPosition(b.target) & Node.DOCUMENT_POSITION_PRECEDING ? 1 : -1
        );
        setActiveId(visible[0].target.id);
      },
      { rootMargin: '-90px 0px -75% 0px' }
    );
    els.forEach((el) => observer.observe(el));
    return () => observer.disconnect();
  }, [headings]);

  if (headings.length === 0) return null;

  const scrollToHeading = (id: string) => {
    const el = document.getElementById(id);
    if (el) {
      jumpLock.current = true;
      jumpTarget.current = id;
      window.clearTimeout(lockTimer.current);
      lockTimer.current = window.setTimeout(() => {
        jumpLock.current = false;
        jumpTarget.current = null;
      }, 1500);
      el.scrollIntoView({ behavior: 'smooth' });
      setActiveId(id);
      window.history.replaceState(null, '', `#${id}`);
    }
  };

  const scrollToTop = () => {
    window.scrollTo({ top: 0, behavior: 'smooth' });
  };

  return (
    <nav className="w-56 xl:w-64 p-4 text-xs font-mono select-none">
      <div className="flex items-center gap-1.5 text-[11px] font-bold text-[var(--text-subtext)] uppercase tracking-wider mb-3">
        <AlignLeft className="w-3.5 h-3.5 text-[var(--accent-mauve)]" />
        <span>On this page</span>
      </div>

      <div className="space-y-1 border-l border-[var(--border)] pl-2 max-h-[calc(100vh-12rem)] overflow-y-auto pr-1">
        {headings.map((heading, idx) => {
          const isActive = activeId === heading.id;
          // Bar geometry lives on the button; level indent on the inner
          // span, so the two never compete for the same property.
          const indent =
            heading.level === 1 ? '' : heading.level === 2 ? 'pl-3' : 'pl-6';
          return (
            <button
              key={`${heading.id}-${idx}`}
              onClick={() => scrollToHeading(heading.id)}
              type="button"
              className={`w-full text-left py-1 text-[12px] transition-colors cursor-pointer block ${
                heading.level === 3 ? 'text-[11px]' : ''
              } ${
                isActive
                  ? '-ml-[9px] pl-2 border-l-2 border-[var(--accent-mauve)] text-[var(--accent-mauve)] font-semibold'
                  : 'text-[var(--text-muted)] hover:text-[var(--text-primary)]'
              }`}
              title={heading.text}
            >
              <span className={`block truncate ${indent}`}>{heading.text}</span>
            </button>
          );
        })}
      </div>

      <div className="mt-6 pt-3 border-t border-[var(--border)]">
        <button
          onClick={scrollToTop}
          type="button"
          className="flex items-center gap-1.5 text-[11px] text-[var(--text-muted)] hover:text-[var(--accent-blue)] transition-colors cursor-pointer"
        >
          <ArrowUp className="w-3.5 h-3.5" />
          <span>Back to top</span>
        </button>
      </div>
    </nav>
  );
}
