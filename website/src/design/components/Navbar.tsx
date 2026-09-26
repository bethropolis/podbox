import React, { useState, useEffect } from 'react';
import { useTheme } from './ThemeContext';
import { PodboxLogo } from './PodboxLogo';
import { GithubIcon } from './GithubIcon';
import { withBase } from '../base';
import {
  Search,
  Moon,
  Sun,
  Menu,
  X,
  Package,
  BookOpen,
  Terminal,
  Layers3,
  SlidersHorizontal,
  Home,
  FileCode2,
  ExternalLink,
} from 'lucide-react';

interface NavbarProps {
  currentPath: string;
  isDocsPage?: boolean;
}

export function Navbar({ currentPath, isDocsPage = false }: NavbarProps) {
  const { toggleTheme, isDark } = useTheme();
  const [mobileMenuOpen, setMobileMenuOpen] = useState(false);

  // Close mobile drawer on route change or Escape key
  useEffect(() => {
    setMobileMenuOpen(false);
  }, [currentPath]);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        setMobileMenuOpen(false);
      }
    };
    if (mobileMenuOpen) {
      window.addEventListener('keydown', handleKeyDown);
      document.body.style.overflow = 'hidden';
    }
    return () => {
      window.removeEventListener('keydown', handleKeyDown);
      document.body.style.overflow = 'unset';
    };
  }, [mobileMenuOpen]);

  const openSearch = () => window.dispatchEvent(new CustomEvent('podbox:open-search'));
  const toggleSidebar = () => window.dispatchEvent(new CustomEvent('podbox:toggle-sidebar'));

  const navLinks = [
    { label: 'Overview', path: '/', icon: Home },
    { label: 'Docs', path: '/docs', icon: BookOpen },
    { label: 'Quick Start', path: '/docs/getting-started', icon: Terminal },
    { label: 'Configuration', path: '/docs/config', icon: FileCode2 },
    { label: 'Architecture', path: '/docs/architecture', icon: Layers3 },
    { label: 'Studio', path: '/studio', icon: SlidersHorizontal, isStudio: true },
  ];

  return (
    <>
      <header className="sticky top-0 z-40 w-full h-14 bg-[var(--bg-crust)]/85 backdrop-blur-md border-b border-[var(--border)] font-sans select-none transition-colors">
        <div className="max-w-7xl mx-auto h-full px-4 sm:px-6 flex items-center justify-between gap-3">
          {/* Left: Hamburger (mobile) + Brand Logo */}
          <div className="flex items-center gap-2.5">
            {/* Sidebar toggle for docs on mobile */}
            {isDocsPage && (
              <button
                onClick={toggleSidebar}
                type="button"
                className="p-1.5 md:hidden text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-surface0)] rounded-[2px] cursor-pointer"
                aria-label="Open documentation sidebar"
              >
                <BookOpen className="w-4 h-4 text-[var(--accent-mauve)]" />
              </button>
            )}

            {/* Mobile Drawer Trigger */}
            <button
              onClick={() => setMobileMenuOpen(true)}
              type="button"
              className="p-1.5 md:hidden text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-surface0)] rounded-[2px] cursor-pointer"
              aria-label="Open navigation menu"
              aria-expanded={mobileMenuOpen}
              aria-controls="mobile-nav-drawer"
            >
              <Menu className="w-5 h-5" />
            </button>

            {/* Brand Title */}
            <a
              href={withBase('/')}
              className="flex items-center gap-2.5 cursor-pointer group text-left focus:outline-none focus:ring-1 focus:ring-[var(--accent-mauve)]/50 rounded-[2px]"
              aria-label="podbox home"
            >
              <PodboxLogo className="w-7 h-7 shrink-0 transition-transform duration-200 group-hover:scale-105" />
              <div className="flex items-center gap-2">
                <span className="font-bold text-base tracking-tight text-[var(--text-primary)]">
                  podbox
                </span>
                <span className="hidden sm:inline-block text-[10px] font-mono px-1.5 py-0.5 rounded-[2px] bg-[var(--accent-mauve)]/10 text-[var(--accent-mauve)] border border-[var(--accent-mauve)]/30 font-semibold">
                  v0.7.2
                </span>
              </div>
            </a>
          </div>

          {/* Center: Desktop Nav Links */}
          <nav className="hidden md:flex items-center gap-1 text-xs">
            {navLinks.map((link) => {
              const Icon = link.icon;
              const isActive =
                link.path === '/'
                  ? currentPath === '/'
                  : currentPath.startsWith(link.path);

              return (
                <a
                  key={link.path}
                  href={withBase(link.path)}
                  className={`px-3 py-1.5 rounded-[2px] transition-colors cursor-pointer flex items-center gap-1.5 ${
                    isActive
                      ? 'text-[var(--accent-mauve)] font-semibold bg-[var(--bg-surface0)]'
                      : 'text-[var(--text-subtext)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-surface0)]/50'
                  }`}
                >
                  <Icon
                    className={`w-3.5 h-3.5 ${
                      link.isStudio
                        ? 'text-[var(--accent-mauve)]'
                        : isActive
                        ? 'text-[var(--accent-mauve)]'
                        : 'text-[var(--text-muted)]'
                    }`}
                  />
                  <span>{link.label}</span>
                  {link.isStudio && (
                    <span className="text-[9px] font-mono uppercase px-1 py-0.2 rounded-[2px] bg-[var(--accent-mauve)]/20 text-[var(--accent-mauve)] font-bold">
                      PRO
                    </span>
                  )}
                </a>
              );
            })}
          </nav>

          {/* Right: Search, Repos, Theme Switcher */}
          <div className="flex items-center gap-1.5 sm:gap-2">
            {/* Search Trigger */}
            <button
              onClick={openSearch}
              type="button"
              className="flex items-center gap-2 px-2.5 py-1.5 rounded-[2px] bg-[var(--bg-base)] border border-[var(--border)] text-xs text-[var(--text-muted)] hover:border-[var(--accent-mauve)]/60 hover:text-[var(--text-subtext)] transition-colors cursor-pointer"
              aria-label="Search documentation and commands (Press ⌘K or /)"
            >
              <Search className="w-3.5 h-3.5 text-[var(--accent-mauve)] shrink-0" />
              <span className="hidden sm:inline">Search...</span>
              <kbd className="hidden sm:flex items-center gap-0.5 text-[10px] font-mono bg-[var(--bg-surface0)] px-1.5 py-0.5 rounded-[2px] border border-[var(--border)] text-[var(--text-subtext)]">
                ⌘K
              </kbd>
            </button>

            {/* GitHub */}
            <a
              href="https://github.com/bethropolis/podbox"
              target="_blank"
              rel="noopener noreferrer"
              className="p-2 text-[var(--text-subtext)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-surface0)] rounded-[2px] transition-colors"
              title="GitHub repository"
              aria-label="GitHub repository"
            >
              <GithubIcon className="w-4 h-4" />
            </a>

            {/* Crates.io */}
            <a
              href="https://crates.io/crates/podbox-cli"
              target="_blank"
              rel="noopener noreferrer"
              className="p-2 text-[var(--text-subtext)] hover:text-[var(--accent-peach)] hover:bg-[var(--bg-surface0)] rounded-[2px] transition-colors hidden sm:inline-flex"
              title="crates.io: podbox-cli"
              aria-label="crates.io package"
            >
              <Package className="w-4 h-4" />
            </a>

            {/* Theme switcher */}
            <button
              onClick={toggleTheme}
              type="button"
              className="p-2 text-[var(--text-subtext)] hover:text-[var(--accent-yellow)] hover:bg-[var(--bg-surface0)] rounded-[2px] transition-colors cursor-pointer"
              title={isDark ? 'Switch to Catppuccin Latte' : 'Switch to Catppuccin Mocha'}
              aria-label="Toggle theme"
            >
              {isDark ? <Sun className="w-4 h-4" /> : <Moon className="w-4 h-4" />}
            </button>
          </div>
        </div>
      </header>

      {/* Mobile Slide-out Terminal Drawer */}
      {mobileMenuOpen && (
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
            onClick={() => setMobileMenuOpen(false)}
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
                onClick={() => setMobileMenuOpen(false)}
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
                  const isActive =
                    link.path === '/'
                      ? currentPath === '/'
                      : currentPath.startsWith(link.path);

                  return (
                    <a
                      key={link.path}
                      href={withBase(link.path)}
                      onClick={() => setMobileMenuOpen(false)}
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
                          Workbench
                        </span>
                      )}
                    </a>
                  );
                })}
              </div>

              {/* Search shortcut */}
              <button
                onClick={() => {
                  setMobileMenuOpen(false);
                  openSearch();
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
              <span>v0.7.2 (MIT)</span>
              <button
                onClick={toggleTheme}
                className="flex items-center gap-1.5 text-[var(--text-subtext)] hover:text-[var(--accent-yellow)]"
              >
                {isDark ? <Sun className="w-3.5 h-3.5" /> : <Moon className="w-3.5 h-3.5" />}
                <span>{isDark ? 'Latte' : 'Mocha'}</span>
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
