import React, { useEffect, useMemo, useRef, useState } from 'react';
import { Search } from 'lucide-react';
import { CATEGORIES, FIELDS } from './schema';
import type { StudioState } from './useStudioState';

interface SettingsPaletteProps {
  /** Jump to a field: switches its tab and focuses the input. */
  onPick: (field: string) => void;
  /** Switch tab only. */
  onCategory: (category: StudioState['activeCategory']) => void;
  onClose: () => void;
}

interface Entry {
  id: string;
  label: string;
  kind: 'field' | 'tab';
  keywords: string;
}

/**
 * Cmd+K over the Studio workspace: every setting reachable in one jump.
 *
 * Entries come from the same schema table the form and the TOML generator
 * read, so a new field appears here without a second list to maintain.
 * Tabs are included because they are the coarsest jump ("proxies" lands on
 * both proxy sections at once).
 */
export function SettingsPalette({ onPick, onCategory, onClose }: SettingsPaletteProps) {
  const [query, setQuery] = useState('');
  const [selected, setSelected] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  const entries = useMemo<Entry[]>(() => {
    const fields: Entry[] = FIELDS.map((f) => ({
      id: f.tomlPath,
      label: f.tomlPath,
      kind: 'field',
      keywords: `${f.tomlPath} ${f.stateKey}`,
    }));
    const tabs: Entry[] = CATEGORIES.map((c) => ({
      id: c.id,
      label: `${c.name} — ${c.blurb}`,
      kind: 'tab',
      keywords: `${c.id} ${c.name} ${c.label} ${c.blurb}`,
    }));
    return [...tabs, ...fields];
  }, []);

  const results = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return entries.filter((e) => e.kind === 'tab');
    const terms = q.split(/\s+/);
    return entries
      .filter((e) => terms.every((t) => e.keywords.toLowerCase().includes(t)))
      .slice(0, 24);
  }, [entries, query]);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  useEffect(() => {
    setSelected(0);
  }, [query]);

  const choose = (entry: Entry | undefined) => {
    if (!entry) return;
    if (entry.kind === 'field') onPick(entry.id);
    else onCategory(entry.id as StudioState['activeCategory']);
    onClose();
  };

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center pt-[10vh] px-4">
      <div className="fixed inset-0 bg-black/60" onClick={onClose} />
      <div className="relative w-full max-w-lg rounded-[3px] bg-[var(--bg-mantle)] border border-[var(--border)] shadow-2xl overflow-hidden">
        <div className="flex items-center gap-2 px-3 py-2.5 border-b border-[var(--border)]">
          <Search className="w-3.5 h-3.5 text-[var(--text-muted)] shrink-0" />
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'ArrowDown') {
                e.preventDefault();
                setSelected((s) => Math.min(s + 1, results.length - 1));
              } else if (e.key === 'ArrowUp') {
                e.preventDefault();
                setSelected((s) => Math.max(s - 1, 0));
              } else if (e.key === 'Enter') {
                e.preventDefault();
                choose(results[selected]);
              } else if (e.key === 'Escape') {
                e.preventDefault();
                onClose();
              }
            }}
            placeholder="Jump to a setting — try port, gpu, idle, dbus"
            spellCheck={false}
            autoComplete="off"
            className="w-full bg-transparent text-sm font-mono text-[var(--text-primary)] placeholder:text-[var(--text-muted)] focus:outline-none"
          />
          <kbd className="text-[10px] font-mono text-[var(--text-muted)] px-1.5 py-0.5 rounded-[2px] border border-[var(--border)]">
            esc
          </kbd>
        </div>
        <div className="max-h-[50vh] overflow-y-auto scrollbar-slim py-1">
          {results.length === 0 ? (
            <p className="px-3 py-4 text-[11px] font-mono text-[var(--text-muted)]">
              No setting matches “{query}”.
            </p>
          ) : (
            results.map((entry, idx) => (
              <button
                key={entry.id}
                type="button"
                onMouseEnter={() => setSelected(idx)}
                onClick={() => choose(entry)}
                className={`w-full text-left px-3 py-1.5 flex items-center gap-2 text-xs font-mono cursor-pointer transition-colors ${
                  idx === selected
                    ? 'bg-[var(--accent-mauve)]/15 text-[var(--accent-mauve)]'
                    : 'text-[var(--text-subtext)] hover:bg-[var(--bg-surface0)]'
                }`}
              >
                <span className="truncate">{entry.label}</span>
                <span className="ml-auto shrink-0 text-[10px] uppercase tracking-wider text-[var(--text-muted)]">
                  {entry.kind}
                </span>
              </button>
            ))
          )}
        </div>
      </div>
    </div>
  );
}
