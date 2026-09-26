import React from 'react';
import { X, Search, Package, ExternalLink, Moon, Sun } from 'lucide-react';
import { PodboxLogo } from '../PodboxLogo';
import { GithubIcon } from '../GithubIcon';
import { withBase } from '../../base';
import { navLinks, isLinkActive } from './navLinks';

interface NavbarDrawerProps {
  version: string;
  currentPath: string;
  isDark: boolean;
  onToggleTheme: () => void;
  onClose: () => void;
  onOpenSearch: () => void;
}

export function NavbarDrawer({ currentPath, version, isDark, onToggleTheme, onClose, onOpenSearch }: NavbarDrawerProps) {
  return (
    <div
      id="mobile-nav-drawer"
      role="dialog"
      aria-modal="true"
      aria-label="Navigation menu"
      className="fixed inset-0 z-50 md:hidden flex"
    >
      {/* Backdrop */}
      <div
        className="fixed inset-0 bg-black/60 backdrop-blur-sm transition-opacity"
        onClick={onClose}
        aria-hidden="true"
      />

      {/* Drawer content styled as terminal window */}
      <div className="relative w-4/5 max-w-sm h-full bg-[var(--bg-mantle)] border-r border-[var(--border)] shadow-2xl flex flex-col z-10 animate-slideRight">
        {/* Terminal Window Chrome */}
        <div className="h-12 px-4 bg-[var(--bg-crust)] border-b border-[var(--border)] flex items-center justify-between">
          <div className="flex items-center gap-2">
            <div className="flex gap-1.5" aria-hidden="true">
              <span className="w-2.5 h-2.5 rounded-full bg-[var(--accent-red)]/80 inline-block" />
              <span className="w-2.5 h-2.5 rounded-full bg-[var(--accent-yellow)]/80 inline-block" />
              <span className="w-2.5 h-2.5 rounded-full bg-[var(--accent-green)]/80 inline-block" />
            </div>
            <span className="text-[11px] font-mono text-[var(--text-muted)] ml-2">
              guest@podbox:~
            </span>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="p-1 text-[var(--text-muted)] hover:text-[var(--text-primary)] rounded-[2px]"
            aria-label="Close navigation menu"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Drawer Body */}
        <div className="p-4 flex-1 overflow-y-auto space-y-6">
          {/* Brand in drawer */}
          <div className="flex items-center gap-3 pb-3 border-b border-[var(--border)]">
            <PodboxLogo className="w-8 h-8 shrink-0" />
            <div>
              <div className="font-bold text-sm text-[var(--text-primary)]">
                podbox
              </div>
              <div className="text-[11px] text-[var(--text-muted)] font-mono">
                podman-native containers
              </div>
            </div>
          </div>

          {/* Navigation Links */}
          <div className="space-y-1">
            <div className="text-[10px] font-mono uppercase tracking-wider text-[var(--text-muted)] px-2 mb-2">
              Navigation
            </div>
            {navLinks.map((link) => {
              const Icon = link.icon;
              const isActive = isLinkActive(link, currentPath);

              return (
                <a
                  key={link.path}
                  href={withBase(link.path)}
                  onClick={onClose}
                  className={`w-full flex items-center justify-between px-3 py-2.5 rounded-[3px] text-xs font-medium transition-colors cursor-pointer ${
                    isActive
                      ? 'bg-[var(--accent-mauve)]/10 text-[var(--accent-mauve)] border border-[var(--accent-mauve)]/30 font-semibold'
                      : 'text-[var(--text-subtext)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-surface0)]'
                  }`}
                >
                  <div className="flex items-center gap-2.5">
                    <Icon className="w-4 h-4" />
                    <span>{link.label}</span>
                  </div>
                  {link.isStudio && (
                    <span className="text-[9px] font-mono uppercase px-1.5 py-0.5 rounded-[2px] bg-[var(--accent-mauve)] text-[var(--bg-crust)] font-bold">
                      Try
                    </span>
                  )}
                </a>
              );
            })}
          </div>

          {/* Search shortcut */}
          <button
            onClick={() => {
              onClose();
              onOpenSearch();
            }}
            className="w-full flex items-center gap-2 px-3 py-2 rounded-[3px] bg-[var(--bg-base)] border border-[var(--border)] text-xs text-[var(--text-subtext)]"
          >
            <Search className="w-3.5 h-3.5 text-[var(--accent-mauve)]" />
            <span>Search Documentation (⌘K)</span>
          </button>

          {/* External Links */}
          <div className="pt-2 border-t border-[var(--border)] space-y-1">
            <div className="text-[10px] font-mono uppercase tracking-wider text-[var(--text-muted)] px-2 mb-1">
              Community & Code
            </div>
            <a
              href="https://github.com/bethropolis/podbox"
              target="_blank"
              rel="noopener noreferrer"
              className="flex items-center justify-between px-3 py-2 rounded-[2px] text-xs text-[var(--text-subtext)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-surface0)]"
            >
              <div className="flex items-center gap-2">
                <GithubIcon className="w-3.5 h-3.5" />
                <span>GitHub Repository</span>
              </div>
              <ExternalLink className="w-3 h-3 text-[var(--text-muted)]" />
            </a>
            <a
              href="https://crates.io/crates/podbox-cli"
              target="_blank"
              rel="noopener noreferrer"
              className="flex items-center justify-between px-3 py-2 rounded-[2px] text-xs text-[var(--text-subtext)] hover:text-[var(--accent-peach)] hover:bg-[var(--bg-surface0)]"
            >
              <div className="flex items-center gap-2">
                <Package className="w-3.5 h-3.5" />
                <span>crates.io / podbox-cli</span>
              </div>
              <ExternalLink className="w-3 h-3 text-[var(--text-muted)]" />
            </a>
          </div>
        </div>

        {/* Drawer Footer */}
        <div className="p-4 bg-[var(--bg-crust)] border-t border-[var(--border)] flex items-center justify-between text-xs text-[var(--text-muted)] font-mono">
          <span>v{version} (MIT)</span>
          <button
            onClick={onToggleTheme}
            className="flex items-center gap-1.5 text-[var(--text-subtext)] hover:text-[var(--accent-yellow)]"
          >
            {isDark ? <Sun className="w-3.5 h-3.5" /> : <Moon className="w-3.5 h-3.5" />}
            <span>{isDark ? 'Latte' : 'Mocha'}</span>
          </button>
        </div>
      </div>
    </div>
  );
}
