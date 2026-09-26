import React, { useEffect, useState } from 'react';
import { AlignLeft, ArrowUp } from 'lucide-react';

interface TocHeading {
  id: string;
  text: string;
  level: number;
}

// Builds its links from the rendered article DOM, so TOC ids and heading
// ids can never drift apart (single source of truth: the article itself).
export function DocsToc() {
  const [headings, setHeadings] = useState<TocHeading[]>([]);
  const [activeId, setActiveId] = useState<string>('');

  useEffect(() => {
    const els = Array.from(
      document.querySelectorAll('article h1[id], article h2[id], article h3[id]')
    );
    const found: TocHeading[] = els.map(el => ({
      id: el.id,
      text: (el.textContent || '').trim(),
      level: el.tagName === 'H1' ? 1 : el.tagName === 'H2' ? 2 : 3,
    }));
    setHeadings(found);
    setActiveId(found[0]?.id || '');
  }, []);

  useEffect(() => {
    if (headings.length === 0) return;

    const handleScroll = () => {
      const scrollPosition = window.scrollY + 120;
      let currentActive = headings[0]?.id || '';

      for (const heading of headings) {
        const el = document.getElementById(heading.id);
        if (el && el.offsetTop <= scrollPosition) {
          currentActive = heading.id;
        }
      }

      setActiveId(currentActive);
    };

    window.addEventListener('scroll', handleScroll, { passive: true });
    handleScroll();

    return () => window.removeEventListener('scroll', handleScroll);
  }, [headings]);

  if (headings.length === 0) return null;

  const scrollToHeading = (id: string) => {
    const el = document.getElementById(id);
    if (el) {
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
          return (
            <button
              key={`${heading.id}-${idx}`}
              onClick={() => scrollToHeading(heading.id)}
              type="button"
              className={`w-full text-left py-1 text-[12px] truncate transition-colors cursor-pointer block ${
                heading.level === 3 ? 'pl-3 text-[11px]' : ''
              } ${
                isActive
                  ? 'text-[var(--accent-mauve)] font-semibold -ml-[9px] pl-2 border-l-2 border-[var(--accent-mauve)]'
                  : 'text-[var(--text-muted)] hover:text-[var(--text-primary)]'
              }`}
              title={heading.text}
            >
              {heading.text}
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
