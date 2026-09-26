import React, { useState, useEffect, useRef } from 'react';
import { Terminal, FileText, CornerDownLeft, X, Layers } from 'lucide-react';
import { withBase } from '../base';
import type { SearchDoc } from '../searchIndex';
import { predefinedCommands, type SearchResult } from './palette/commands';

interface CommandPaletteProps {
  // No props: the search index lazy-loads on first open (separate chunk),
  // so it never ships with the initial page bundle.
}

export function CommandPalette() {
  const [isOpen, setIsOpen] = useState(false);
  const [docs, setDocs] = useState<SearchDoc[]>([]);
  const indexLoading = useRef(false);
  const [query, setQuery] = useState('');
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  // Global shortcuts: Cmd+K / Ctrl+K / `/`, plus the navbar trigger event.
  // The docs index loads on first open (dynamic import = separate chunk).
  useEffect(() => {
    const ensureIndex = () => {
      if (docs.length === 0 && !indexLoading.current) {
        indexLoading.current = true;
        import('../searchIndex').then(
          (m) => setDocs(m.SEARCH_DOCS),
          () => { indexLoading.current = false; },
        );
      }
    };
    const open = () => {
      ensureIndex();
      setIsOpen(true);
    };
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        setIsOpen(prev => {
          if (!prev) ensureIndex();
          return !prev;
        });
      } else if (e.key === '/' && !['INPUT', 'TEXTAREA'].includes((e.target as HTMLElement)?.tagName)) {
        e.preventDefault();
        ensureIndex();
        setIsOpen(true);
      } else if (e.key === 'Escape') {
        setIsOpen(false);
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    window.addEventListener('podbox:open-search', open);
    return () => {
      window.removeEventListener('keydown', handleKeyDown);
      window.removeEventListener('podbox:open-search', open);
    };
  }, []);

  const navigate = (path: string) => {
    setIsOpen(false);
    window.location.assign(withBase(path));
  };

  // Dynamic search across docs pages and headings
  const filteredResults: SearchResult[] = React.useMemo(() => {
    if (!query.trim()) {
      return predefinedCommands.slice(0, 8);
    }
    const q = query.toLowerCase();
    const results: SearchResult[] = [];

    // Check predefined commands
    predefinedCommands.forEach(cmd => {
      if (cmd.title.toLowerCase().includes(q) || (cmd.snippet && cmd.snippet.toLowerCase().includes(q))) {
        results.push(cmd);
      }
    });

    // Check doc pages and headings
    docs.forEach(doc => {
      if (doc.title.toLowerCase().includes(q)) {
        results.push({
          title: doc.title,
          category: 'Documentation',
          path: doc.path,
          snippet: doc.description || `Documentation: ${doc.title}`,
          badge: 'page',
        });
      }
      doc.headings.forEach(h => {
        if (h.text.toLowerCase().includes(q)) {
          results.push({
            title: `${doc.title} > ${h.text}`,
            category: 'Doc Section',
            path: `${doc.path}#${h.id}`,
            snippet: `Heading in ${doc.title}`,
            badge: 'section',
          });
        }
      });
    });

    return results.slice(0, 10);
  }, [query, docs]);

  useEffect(() => {
    setSelectedIndex(0);
  }, [query]);

  useEffect(() => {
    if (isOpen) {
      setTimeout(() => inputRef.current?.focus(), 50);
    }
  }, [isOpen]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setSelectedIndex(prev => (prev + 1) % Math.max(1, filteredResults.length));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setSelectedIndex(prev => (prev - 1 + filteredResults.length) % Math.max(1, filteredResults.length));
    } else if (e.key === 'Enter') {
      e.preventDefault();
      if (filteredResults[selectedIndex]) {
        navigate(filteredResults[selectedIndex].path);
      }
    } else if (e.key === 'Escape') {
      e.preventDefault();
      setIsOpen(false);
    }
  };

  if (!isOpen) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center pt-16 sm:pt-24 px-4 bg-black/70 backdrop-blur-xs animate-in fade-in duration-150"
      onClick={() => setIsOpen(false)}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Search documentation and commands"
        className="w-full max-w-2xl rounded-[4px] border border-[var(--border-focus)] bg-[var(--bg-mantle)] shadow-2xl overflow-hidden font-mono"
        onClick={e => e.stopPropagation()}
      >
        {/* Terminal Chrome Bar */}
        <div className="flex items-center justify-between px-3.5 py-2 bg-[var(--bg-crust)] border-b border-[var(--border)]">
          <div className="flex items-center gap-2" aria-hidden="true">
            <span className="w-2.5 h-2.5 rounded-full bg-[#f38ba8]/80 inline-block" />
            <span className="w-2.5 h-2.5 rounded-full bg-[#f9e2af]/80 inline-block" />
            <span className="w-2.5 h-2.5 rounded-full bg-[#a6e3a1]/80 inline-block" />
            <span className="text-xs text-[var(--text-subtext)] uppercase tracking-wider ml-1">
              term // podbox-search
            </span>
          </div>
          <button
            onClick={() => setIsOpen(false)}
            type="button"
            className="text-[var(--text-muted)] hover:text-[var(--text-primary)] cursor-pointer p-0.5 rounded"
            aria-label="Close search (Esc)"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Input Bar */}
        <div className="flex items-center gap-2.5 px-4 py-3 border-b border-[var(--border)] bg-[var(--bg-base)]">
          <span className="text-[var(--accent-mauve)] font-bold select-none text-sm">&gt;</span>
          <input
            ref={inputRef}
            type="text"
            role="searchbox"
            aria-label="Search query"
            value={query}
            onChange={e => setQuery(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Search commands, docs, config keys, quadlets... (Esc to close)"
            className="w-full bg-transparent text-[var(--text-primary)] placeholder-[var(--text-muted)] text-sm focus:outline-none"
          />
          {query && (
            <button
              onClick={() => setQuery('')}
              className="text-xs text-[var(--text-muted)] hover:text-[var(--text-primary)] cursor-pointer"
            >
              clear
            </button>
          )}
        </div>

        {/* Results List */}
        <div className="max-h-80 overflow-y-auto p-2 divide-y divide-[var(--border)]/40">
          {filteredResults.length === 0 ? (
            <div className="p-8 text-center text-xs text-[var(--text-muted)]">
              No matching commands or documentation entries found for &quot;{query}&quot;
            </div>
          ) : (
            filteredResults.map((item, index) => {
              const isSelected = index === selectedIndex;
              return (
                <div
                  key={`${item.path}-${index}`}
                  onClick={() => navigate(item.path)}
                  onMouseEnter={() => setSelectedIndex(index)}
                  className={`flex items-center justify-between p-2.5 rounded-[2px] cursor-pointer transition-colors text-xs ${
                    isSelected
                      ? 'bg-[var(--accent-mauve)]/15 border-l-2 border-[var(--accent-mauve)] text-[var(--text-primary)]'
                      : 'text-[var(--text-subtext)] hover:bg-[var(--bg-surface0)]/50'
                  }`}
                >
                  <div className="flex items-start gap-2.5 min-w-0 pr-2">
                    {item.category === 'CLI Command' ? (
                      <Terminal className="w-4 h-4 text-[var(--accent-mauve)] shrink-0 mt-0.5" />
                    ) : item.category === 'Configuration' ? (
                      <Layers className="w-4 h-4 text-[var(--accent-blue)] shrink-0 mt-0.5" />
                    ) : (
                      <FileText className="w-4 h-4 text-[var(--accent-green)] shrink-0 mt-0.5" />
                    )}
                    <div className="min-w-0">
                      <div className="font-semibold text-[var(--text-primary)] flex items-center gap-2">
                        <span className="truncate">{item.title}</span>
                        {item.badge && (
                          <span className="text-[10px] uppercase px-1.5 py-0.2 rounded-[2px] bg-[var(--bg-surface0)] text-[var(--text-muted)] border border-[var(--border)]">
                            {item.badge}
                          </span>
                        )}
                      </div>
                      {item.snippet && (
                        <p className="text-[11px] text-[var(--text-muted)] truncate mt-0.5">
                          {item.snippet}
                        </p>
                      )}
                    </div>
                  </div>

                  <div className="flex items-center gap-1.5 text-[11px] text-[var(--text-muted)] shrink-0">
                    <span className="hidden sm:inline text-[10px] text-[var(--text-muted)]">
                      {item.category}
                    </span>
                    {isSelected && <CornerDownLeft className="w-3.5 h-3.5 text-[var(--accent-mauve)]" />}
                  </div>
                </div>
              );
            })
          )}
        </div>

        {/* Footer shortcuts */}
        <div className="flex items-center justify-between px-3 py-2 bg-[var(--bg-crust)] border-t border-[var(--border)] text-[11px] text-[var(--text-muted)] select-none">
          <div className="flex items-center gap-3">
            <span>
              <kbd className="px-1.5 py-0.5 bg-[var(--bg-surface0)] border border-[var(--border)] rounded-[2px] text-[10px]">↑</kbd>{' '}
              <kbd className="px-1.5 py-0.5 bg-[var(--bg-surface0)] border border-[var(--border)] rounded-[2px] text-[10px]">↓</kbd> navigate
            </span>
            <span>
              <kbd className="px-1.5 py-0.5 bg-[var(--bg-surface0)] border border-[var(--border)] rounded-[2px] text-[10px]">↵</kbd> select
            </span>
            <span>
              <kbd className="px-1.5 py-0.5 bg-[var(--bg-surface0)] border border-[var(--border)] rounded-[2px] text-[10px]">esc</kbd> close
            </span>
          </div>
          <span className="text-[var(--accent-mauve)]">podbox CLI v0.7.2</span>
        </div>
      </div>
    </div>
  );
}
