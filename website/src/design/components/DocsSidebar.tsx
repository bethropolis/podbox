import React, { useState, useEffect } from 'react';
import {
  BookOpen,
  Search,
  ExternalLink,
  Terminal,
  ShieldCheck,
  Layers3,
  Settings,
  ArrowLeftRight,
  PackageCheck,
  FileCode,
  Radio,
  FileText,
  HelpCircle,
  X
} from 'lucide-react';
import { withBase } from '../base';

export interface SidebarItem {
  id: string;
  title: string;
  path: string;
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

  const getIcon = (id: string) => {
    switch (id) {
      case 'index': return <BookOpen className="w-4 h-4 shrink-0 text-[var(--accent-mauve)]" />;
      case 'getting-started': return <Terminal className="w-4 h-4 shrink-0 text-[var(--accent-green)]" />;
      case 'cli': return <Terminal className="w-4 h-4 shrink-0 text-[var(--accent-mauve)]" />;
      case 'config': return <Settings className="w-4 h-4 shrink-0 text-[var(--accent-peach)]" />;
      case 'baked-in-packages': return <PackageCheck className="w-4 h-4 shrink-0 text-[var(--accent-teal)]" />;
      case 'architecture': return <Layers3 className="w-4 h-4 shrink-0 text-[var(--accent-blue)]" />;
      case 'export': return <ArrowLeftRight className="w-4 h-4 shrink-0 text-[var(--accent-yellow)]" />;
      case 'guest': return <ShieldCheck className="w-4 h-4 shrink-0 text-[var(--accent-red)]" />;
      case 'dbus-proxy': return <Radio className="w-4 h-4 shrink-0 text-[var(--accent-mauve)]" />;
      case 'quadlet': return <FileCode className="w-4 h-4 shrink-0 text-[var(--accent-green)]" />;
      case 'protocol': return <FileText className="w-4 h-4 shrink-0 text-[var(--accent-blue)]" />;
      case 'troubleshooting': return <HelpCircle className="w-4 h-4 shrink-0 text-[var(--accent-yellow)]" />;
      default: return <FileText className="w-4 h-4 shrink-0" />;
    }
  };

  const filteredNav = items.filter(item =>
    item.title.toLowerCase().includes(filter.toLowerCase())
  );

  return (
    <>
      {/* Mobile Backdrop */}
      {isOpenMobile && (
        <div
          className="fixed inset-0 z-40 bg-black/60 backdrop-blur-xs md:hidden"
          onClick={() => setIsOpenMobile(false)}
        />
      )}

      {/* Sidebar Container */}
      <aside
        className={`fixed top-14 bottom-0 left-0 z-40 w-64 md:w-72 bg-[var(--bg-mantle)] border-r border-[var(--border)] flex flex-col transition-transform duration-200 ease-in-out md:translate-x-0 ${
          isOpenMobile ? 'translate-x-0' : '-translate-x-full'
        }`}
      >
        {/* Mobile Header */}
        <div className="flex items-center justify-between p-3 border-b border-[var(--border)] md:hidden">
          <span className="text-xs font-bold text-[var(--text-subtext)] uppercase tracking-wider">
            Documentation Menu
          </span>
          <button
            onClick={() => setIsOpenMobile(false)}
            className="p-1 text-[var(--text-muted)] hover:text-[var(--text-primary)] rounded-[2px]"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Filter input */}
        <div className="p-3 border-b border-[var(--border)]">
          <div className="relative">
            <Search className="w-3.5 h-3.5 absolute left-2.5 top-1/2 -translate-y-1/2 text-[var(--text-muted)]" />
            <input
              type="text"
              value={filter}
              onChange={e => setFilter(e.target.value)}
              placeholder="Filter topics..."
              className="w-full pl-8 pr-3 py-1.5 bg-[var(--bg-base)] border border-[var(--border)] rounded-[2px] text-xs font-mono text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:border-[var(--accent-mauve)]"
            />
          </div>
        </div>

        {/* Navigation items */}
        <nav className="flex-1 overflow-y-auto p-3 space-y-1 text-xs font-mono">
          <div className="text-[10px] text-[var(--text-muted)] uppercase tracking-widest px-2 py-1 font-semibold">
            Guides & Reference
          </div>

          {filteredNav.map(item => {
            const isActive =
              currentPath === item.path ||
              (item.path === '/docs' && (currentPath === '/docs' || currentPath === '/docs/'));

            return (
              <a
                key={item.id}
                href={withBase(item.path)}
                onClick={() => setIsOpenMobile(false)}
                className={`w-full flex items-center justify-between px-2.5 py-2 rounded-[2px] text-left transition-colors cursor-pointer ${
                  isActive
                    ? 'border-l-2 border-[var(--accent-mauve)] bg-[var(--accent-mauve)]/10 text-[var(--accent-mauve)] font-bold'
                    : 'text-[var(--text-subtext)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-surface0)]/50 border-l-2 border-transparent'
                }`}
              >
                <div className="flex items-center gap-2.5 truncate">
                  <span className={isActive ? 'text-[var(--accent-mauve)]' : 'text-[var(--text-muted)]'}>
                    {getIcon(item.id)}
                  </span>
                  <span className="truncate">{item.title}</span>
                </div>
                {isActive && (
                  <span className="w-1.5 h-1.5 rounded-full bg-[var(--accent-mauve)] shrink-0" />
                )}
              </a>
            );
          })}
        </nav>

        {/* Sidebar Footer info */}
        <div className="p-3 border-t border-[var(--border)] bg-[var(--bg-crust)]/50 text-[11px] text-[var(--text-muted)] space-y-2">
          <div className="flex items-center justify-between">
            <span>Runtime</span>
            <span className="text-[var(--accent-green)] font-bold">Podman Native</span>
          </div>
          <div className="flex items-center justify-between">
            <span>Daemon</span>
            <span className="text-[var(--text-subtext)]">systemd user units</span>
          </div>
          <a
            href="https://github.com/bethropolis/podbox"
            target="_blank"
            rel="noopener noreferrer"
            className="flex items-center justify-between pt-1 text-[var(--accent-blue)] hover:underline"
          >
            <span>GitHub Repository</span>
            <ExternalLink className="w-3 h-3" />
          </a>
        </div>
      </aside>
    </>
  );
}
