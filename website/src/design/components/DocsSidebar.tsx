import React, { useState, useEffect } from 'react';
import { Search, ExternalLink, X } from 'lucide-react';
import { withBase } from '../base';

export interface SidebarItem {
  id: string;
  title: string;
  path: string;
  group: string;
}

interface DocsSidebarProps {
  items: SidebarItem[];
  currentPath: string;
}

export function DocsSidebar({ items, currentPath }: DocsSidebarProps) {
  const [filter, setFilter] = useState('');
  const [isOpenMobile, setIsOpenMobile] = useState(false);

  // The navbar island toggles the drawer via event (islands can't share props).
  useEffect(() => {
    const toggle = () => setIsOpenMobile(prev => !prev);
    window.addEventListener('podbox:toggle-sidebar', toggle);
    return () => window.removeEventListener('podbox:toggle-sidebar', toggle);
  }, []);


  const filteredNav = items.filter(item =>
    item.title.toLowerCase().includes(filter.toLowerCase())
  );
  // Preserve nav order while bucketing into sections.
  const groups: [string, SidebarItem[]][] = [];
  for (const item of filteredNav) {
    const last = groups[groups.length - 1];
    if (last && last[0] === item.group) last[1].push(item);
    else groups.push([item.group, [item]]);
  }

  return (
    <>
      {/* Mobile Backdrop */}
      {isOpenMobile && (
        <div
          className="fixed inset-0 z-40 bg-black/60 backdrop-blur-xs lg:hidden"
          onClick={() => setIsOpenMobile(false)}
        />
      )}

      {/* Sidebar Container */}
      <aside
        className={`fixed top-14 bottom-0 left-0 z-40 w-72 max-w-[85vw] bg-[var(--bg-mantle)] border-r border-[var(--border)] flex flex-col transition-transform duration-200 ease-in-out lg:sticky lg:z-0 lg:h-[calc(100vh-3.5rem)] lg:w-64 xl:w-72 lg:max-w-none lg:shrink-0 lg:translate-x-0 lg:bg-transparent ${
          isOpenMobile ? 'translate-x-0' : '-translate-x-full'
        }`}
      >
        {/* Mobile Header */}
        <div className="flex items-center justify-between p-3 border-b border-[var(--border)] lg:hidden">
          <span className="text-sm font-medium text-[var(--text-primary)]">Documentation</span>
          <button
            onClick={() => setIsOpenMobile(false)}
            type="button"
            aria-label="Close documentation menu"
            className="p-1 text-[var(--text-muted)] hover:text-[var(--text-primary)] rounded-[2px]"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Filter input */}
        <div className="px-3 pt-3 pb-1">
          <div className="relative">
            <Search className="w-3.5 h-3.5 absolute left-2.5 top-1/2 -translate-y-1/2 text-[var(--text-muted)]" />
            <input
              type="text"
              value={filter}
              aria-label="Filter documentation topics"
              onChange={e => setFilter(e.target.value)}
              placeholder="Filter topics..."
              className="w-full pl-8 pr-3 py-1.5 bg-[var(--bg-base)] border border-[var(--border)] rounded-md font-sans text-[13px] text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:border-[var(--accent-mauve)]"
            />
          </div>
        </div>

        {/* Navigation items */}
        <nav className="flex-1 overflow-y-auto px-3 py-2 text-[13.5px] font-sans" aria-label="Documentation">
          {groups.length === 0 && (
            <p className="px-2.5 py-2 text-[var(--text-muted)]">No page matches “{filter}”.</p>
          )}
          {groups.map(([group, entries]) => (
            <div key={group} className="pb-3">
              <div className="px-2.5 pb-1.5 pt-2 text-xs font-medium text-[var(--text-muted)]">{group}</div>
              {entries.map((item) => {
                const isActive =
                  currentPath === item.path ||
                  (item.path === '/docs' && (currentPath === '/docs' || currentPath === '/docs/'));
                return (
                  <a
                    key={item.id}
                    href={withBase(item.path)}
                    aria-current={isActive ? 'page' : undefined}
                    onClick={() => setIsOpenMobile(false)}
                    className={`relative flex items-center rounded-md px-2.5 py-[7px] transition-colors ${
                      isActive
                        ? 'bg-[var(--bg-surface0)] text-[var(--text-primary)] font-medium before:absolute before:left-0 before:top-1.5 before:bottom-1.5 before:w-[2px] before:rounded-full before:bg-[var(--accent-mauve)]'
                        : 'text-[var(--text-subtext)] hover:bg-[var(--bg-surface0)]/50 hover:text-[var(--text-primary)]'
                    }`}
                  >
                    <span className="truncate">{item.title}</span>
                  </a>
                );
              })}
            </div>
          ))}
        </nav>

        {/* Sidebar Footer info */}
        <div className="px-5 py-3 border-t border-[var(--border)] text-xs">
          <a
            href="https://github.com/bethropolis/podbox"
            target="_blank"
            rel="noopener noreferrer"
            className="flex items-center justify-between text-[var(--text-muted)] hover:text-[var(--text-primary)]"
          >
            <span>Source on GitHub</span>
            <ExternalLink className="w-3 h-3" />
          </a>
        </div>
      </aside>
    </>
  );
}
