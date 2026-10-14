import React, { useEffect, useMemo, useRef, useState } from 'react';
import { ArrowRight, Clock, CornerDownLeft, FileText, Hash, KeyRound, Search, SquareTerminal, X } from 'lucide-react';
import { withBase } from '../base';
import { highlightParts, rank, snippet, type Hit, type SearchEntry } from '../search';
import { isPagePaletteActive } from '../paletteGuard';

interface CommandPaletteProps {
  // Only the version string crosses the Astro boundary; the search index
  // lazy-loads on first open (separate chunk), so it never ships with the
  // initial page bundle.
  version: string;
}

// Destinations that aren't docs pages, so they never come from the index.
const EXTRA: SearchEntry[] = [
  { kind: 'page', title: 'Studio', page: 'Studio', path: '/studio', text: 'Build a podbox.toml interactively and preview the generated units' },
  { kind: 'page', title: 'Install podbox', page: 'Home', path: '/#install', text: 'curl script, mise, Homebrew, AUR, cargo, from source' },
  { kind: 'page', title: 'Compare with Distrobox', page: 'Home', path: '/#comparison', text: 'podbox vs distrobox vs raw podman' },
];

const QUICK_LINKS: SearchEntry[] = [
  { kind: 'page', title: 'Getting started', page: 'Docs', path: '/docs/getting-started', text: '' },
  { kind: 'page', title: 'Configuration reference', page: 'Docs', path: '/docs/config', text: '' },
  { kind: 'page', title: 'CLI reference', page: 'Docs', path: '/docs/cli', text: '' },
  { kind: 'page', title: 'Studio', page: 'Studio', path: '/studio', text: '' },
  { kind: 'page', title: 'Troubleshooting', page: 'Docs', path: '/docs/troubleshooting', text: '' },
];

const RECENT_KEY = 'podbox-search-recent';
const RECENT_MAX = 5;

function loadRecent(): SearchEntry[] {
  try {
    const raw = JSON.parse(localStorage.getItem(RECENT_KEY) || '[]');
    return Array.isArray(raw) ? raw.filter((e) => e && typeof e.path === 'string').slice(0, RECENT_MAX) : [];
  } catch {
    return [];
  }
}

function saveRecent(entry: SearchEntry) {
  try {
    const next = [entry, ...loadRecent().filter((e) => e.path !== entry.path || e.title !== entry.title)].slice(0, RECENT_MAX);
    localStorage.setItem(RECENT_KEY, JSON.stringify(next));
  } catch {
    /* private mode: recents are a convenience only */
  }
}

const KIND_LABEL: Record<SearchEntry['kind'], string> = {
  command: 'Command',
  key: 'Config key',
  page: 'Page',
  section: 'Section',
};

function KindIcon({ kind, className }: { kind: SearchEntry['kind']; className: string }) {
  if (kind === 'command') return <SquareTerminal className={className} />;
  if (kind === 'key') return <KeyRound className={className} />;
  if (kind === 'section') return <Hash className={className} />;
  return <FileText className={className} />;
}

function Highlighted({ text, terms }: { text: string; terms: string[] }) {
  return (
    <>
      {highlightParts(text, terms).map((p, i) =>
        p.match ? (
          <mark key={i} className="rounded-[2px] bg-[var(--accent-mauve)]/20 px-px text-inherit">
            {p.text}
          </mark>
        ) : (
          <React.Fragment key={i}>{p.text}</React.Fragment>
        ),
      )}
    </>
  );
}

export function CommandPalette({ version }: CommandPaletteProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [entries, setEntries] = useState<SearchEntry[] | null>(null);
  const indexLoading = useRef(false);
  const [query, setQuery] = useState('');
  const [selected, setSelected] = useState(0);
  const [recent, setRecent] = useState<SearchEntry[]>([]);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const returnFocus = useRef<HTMLElement | null>(null);
  const openRef = useRef(false);
  openRef.current = isOpen;

  // Global shortcuts: Cmd/Ctrl+K and `/`, plus the navbar trigger event.
  useEffect(() => {
    const ensureIndex = () => {
      if (indexLoading.current) return;
      indexLoading.current = true;
      import('../searchIndex').then(
        (m) => setEntries([...EXTRA, ...m.SEARCH_ENTRIES]),
        () => {
          indexLoading.current = false;
        },
      );
    };
    const open = () => {
      returnFocus.current = document.activeElement as HTMLElement | null;
      ensureIndex();
      setRecent(loadRecent());
      setIsOpen(true);
    };
    const handleKeyDown = (e: KeyboardEvent) => {
      // Studio (and any page that marks itself) owns the shortcut while it
      // is on screen; its own palette opens instead of this one.
      if (isPagePaletteActive()) return;
      const target = e.target as HTMLElement | null;
      const typing = !!target && (['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName) || target.isContentEditable);
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        if (openRef.current) setIsOpen(false);
        else open();
      } else if (e.key === 'Escape' && openRef.current) {
        setIsOpen(false);
      } else if (e.key === '/' && !typing) {
        e.preventDefault();
        open();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    window.addEventListener('podbox:open-search', open);
    return () => {
      window.removeEventListener('keydown', handleKeyDown);
      window.removeEventListener('podbox:open-search', open);
    };
  }, []);

  const close = () => {
    setIsOpen(false);
    setQuery('');
    returnFocus.current?.focus?.();
  };

  const hits: Hit[] = useMemo(() => (entries && query.trim() ? rank(entries, query, 30) : []), [entries, query]);

  // Rows shown: ranked hits for a query; recents + quick links when empty.
  const rows: { entry: SearchEntry; terms: string[]; section?: string }[] = useMemo(() => {
    if (query.trim()) return hits.map((h) => ({ entry: h.entry, terms: h.terms }));
    const r = recent.map((entry, i) => ({ entry, terms: [], section: i === 0 ? 'Recent' : undefined }));
    const q = QUICK_LINKS.filter((l) => !recent.some((e) => e.path === l.path)).map((entry, i) => ({
      entry,
      terms: [],
      section: i === 0 ? 'Jump to' : undefined,
    }));
    return [...r, ...q];
  }, [query, hits, recent]);

  useEffect(() => setSelected(0), [query]);

  useEffect(() => {
    if (isOpen) requestAnimationFrame(() => inputRef.current?.focus());
  }, [isOpen]);

  // Keep the keyboard selection visible while arrowing through a long list.
  useEffect(() => {
    listRef.current?.querySelector<HTMLElement>(`[data-row="${selected}"]`)?.scrollIntoView({ block: 'nearest' });
  }, [selected]);

  const go = (entry: SearchEntry, newTab = false) => {
    saveRecent({ ...entry, text: '' });
    const url = withBase(entry.path);
    if (newTab) {
      window.open(url, '_blank', 'noopener');
      return;
    }
    setIsOpen(false);
    setQuery('');
    window.location.assign(url);
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setSelected((i) => (rows.length ? (i + 1) % rows.length : 0));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setSelected((i) => (rows.length ? (i - 1 + rows.length) % rows.length : 0));
    } else if (e.key === 'Enter') {
      e.preventDefault();
      const row = rows[selected];
      if (row) go(row.entry, e.metaKey || e.ctrlKey);
    } else if (e.key === 'Escape') {
      e.preventDefault();
      close();
    } else if (e.key === 'Tab') {
      // Focus stays in the input; Tab moves the selection like the arrows.
      e.preventDefault();
      setSelected((i) => (rows.length ? (i + (e.shiftKey ? -1 : 1) + rows.length) % rows.length : 0));
    }
  };

  if (!isOpen) return null;

  const loading = entries == null && query.trim() !== '';
  const activeId = rows[selected] ? `search-row-${selected}` : undefined;

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/55 backdrop-blur-[2px] sm:px-4 sm:pt-[12vh]"
      onMouseDown={close}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Search the documentation"
        className="flex h-full w-full flex-col overflow-hidden bg-[var(--bg-mantle)] font-sans shadow-2xl sm:h-auto sm:max-h-[min(640px,76vh)] sm:max-w-2xl sm:rounded-lg sm:border sm:border-[var(--border-focus)]"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-3 border-b border-[var(--border)] px-4">
          <Search className="h-4 w-4 shrink-0 text-[var(--text-muted)]" aria-hidden="true" />
          <input
            ref={inputRef}
            type="search"
            role="combobox"
            aria-expanded="true"
            aria-controls="search-results"
            aria-activedescendant={activeId}
            aria-autocomplete="list"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={onKeyDown}
            placeholder="Search docs, commands, config keys…"
            spellCheck={false}
            autoComplete="off"
            className="h-14 w-full bg-transparent text-[15px] text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none [&::-webkit-search-cancel-button]:hidden"
          />
          {query ? (
            <button
              type="button"
              onClick={() => {
                setQuery('');
                inputRef.current?.focus();
              }}
              className="rounded p-1 text-[var(--text-muted)] hover:text-[var(--text-primary)]"
              aria-label="Clear search"
            >
              <X className="h-4 w-4" />
            </button>
          ) : (
            <button
              type="button"
              onClick={close}
              className="rounded border border-[var(--border)] px-1.5 py-0.5 font-mono text-[11px] text-[var(--text-muted)] hover:text-[var(--text-primary)]"
              aria-label="Close search"
            >
              esc
            </button>
          )}
        </div>

        <div ref={listRef} id="search-results" role="listbox" aria-label="Results" className="min-h-0 flex-1 overflow-y-auto overscroll-contain p-2">
          {loading && <p className="px-3 py-6 text-center text-sm text-[var(--text-muted)]">Loading the index…</p>}
          {!loading && query.trim() && rows.length === 0 && (
            <div className="px-3 py-10 text-center">
              <p className="text-sm text-[var(--text-subtext)]">Nothing matches “{query.trim()}”.</p>
              <p className="mt-1 text-xs text-[var(--text-muted)]">Try a config key like gpu, a command like snapshot, or fewer words.</p>
            </div>
          )}
          {rows.map(({ entry, terms, section }, i) => {
            const isSel = i === selected;
            const sub = query.trim() ? snippet(entry.text, terms, 110) : '';
            return (
              <React.Fragment key={`${entry.kind}-${entry.path}-${entry.title}-${i}`}>
                {section && (
                  <div className="flex items-center gap-1.5 px-3 pb-1.5 pt-3 text-xs font-medium text-[var(--text-muted)]">
                    {section === 'Recent' && <Clock className="h-3.5 w-3.5" aria-hidden="true" />}
                    {section}
                  </div>
                )}
                <a
                  id={`search-row-${i}`}
                  data-row={i}
                  role="option"
                  aria-selected={isSel}
                  href={withBase(entry.path)}
                  onClick={(e) => {
                    if (e.metaKey || e.ctrlKey || e.shiftKey || e.button === 1) {
                      saveRecent({ ...entry, text: '' });
                      return;
                    }
                    e.preventDefault();
                    go(entry);
                  }}
                  onMouseMove={() => selected !== i && setSelected(i)}
                  className={`flex items-start gap-3 rounded-md px-3 py-2.5 transition-colors ${
                    isSel ? 'bg-[var(--bg-surface0)]' : ''
                  }`}
                >
                  <KindIcon
                    kind={entry.kind}
                    className={`mt-0.5 h-4 w-4 shrink-0 ${isSel ? 'text-[var(--accent-mauve)]' : 'text-[var(--text-muted)]'}`}
                  />
                  <span className="min-w-0 flex-1">
                    <span className="flex items-baseline gap-2">
                      <span
                        className={`truncate text-sm text-[var(--text-primary)] ${
                          entry.kind === 'command' || entry.kind === 'key' ? 'font-mono text-[13px]' : 'font-medium'
                        }`}
                      >
                        <Highlighted text={entry.title} terms={terms} />
                      </span>
                      {entry.kind !== 'page' && (
                        <span className="hidden shrink-0 truncate text-xs text-[var(--text-muted)] sm:inline">{entry.page}</span>
                      )}
                    </span>
                    {sub && (
                      <span className="mt-0.5 line-clamp-2 block text-[13px] leading-snug text-[var(--text-subtext)]">
                        <Highlighted text={sub} terms={terms} />
                      </span>
                    )}
                  </span>
                  <span className="mt-0.5 flex shrink-0 items-center gap-1.5 text-[11px] text-[var(--text-muted)]">
                    {query.trim() && <span className="hidden sm:inline">{KIND_LABEL[entry.kind]}</span>}
                    {isSel ? (
                      <CornerDownLeft className="h-3.5 w-3.5 text-[var(--text-subtext)]" aria-hidden="true" />
                    ) : (
                      <ArrowRight className="h-3.5 w-3.5 opacity-0" aria-hidden="true" />
                    )}
                  </span>
                </a>
              </React.Fragment>
            );
          })}
        </div>

        <div className="hidden items-center justify-between gap-4 border-t border-[var(--border)] px-4 py-2.5 text-xs text-[var(--text-muted)] sm:flex">
          <div className="flex items-center gap-4">
            <span className="flex items-center gap-1.5">
              <Kbd>↑</Kbd>
              <Kbd>↓</Kbd> move
            </span>
            <span className="flex items-center gap-1.5">
              <Kbd>↵</Kbd> open
            </span>
            <span className="flex items-center gap-1.5">
              <Kbd>ctrl ↵</Kbd> new tab
            </span>
          </div>
          <span>
            {entries ? `${entries.length} entries` : 'index'} · v{version}
          </span>
        </div>
      </div>
    </div>
  );
}

function Kbd({ children }: { children: React.ReactNode }) {
  return (
    <kbd className="rounded border border-[var(--border)] bg-[var(--bg-base)] px-1.5 py-px font-mono text-[10.5px] text-[var(--text-subtext)]">
      {children}
    </kbd>
  );
}
