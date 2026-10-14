import React, { useState, useEffect } from 'react';
import { useTheme } from './ThemeContext';
import { PodboxLogo } from './PodboxLogo';
import { GithubIcon } from './GithubIcon';
import { withBase } from '../base';
import { navLinks, isLinkActive } from './navbar/navLinks';
import { NavbarDrawer } from './navbar/NavbarDrawer';
import {
  Search,
  Moon,
  Sun,
  Menu,
  BookOpen,
  Package,
} from 'lucide-react';

interface NavbarProps {
  currentPath: string;
  isDocsPage?: boolean;
  version: string;
}

export function Navbar({ currentPath, isDocsPage = false, version }: NavbarProps) {
  const { toggleTheme, isDark } = useTheme();
  const [mobileMenuOpen, setMobileMenuOpen] = useState(false);
  // SSR can't know the platform; start with Ctrl (most visitors run Linux)
  // and switch to ⌘ after mount on Apple devices.
  const [modKey, setModKey] = useState('Ctrl');
  useEffect(() => {
    if (/Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent)) setModKey('⌘');
  }, []);

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
                className="p-1.5 lg:hidden text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-surface0)] rounded-[2px] cursor-pointer"
                aria-label="Open documentation sidebar"
              >
                <BookOpen className="w-4 h-4 text-[var(--accent-mauve)]" />
              </button>
            )}

            {/* Mobile Drawer Trigger */}
            <button
              onClick={() => setMobileMenuOpen(true)}
              type="button"
              className="p-1.5 lg:hidden text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-surface0)] rounded-[2px] cursor-pointer"
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
              <PodboxLogo className="w-7 h-7 shrink-0 opacity-80 transition-[transform,opacity] duration-200 group-hover:scale-105 group-hover:opacity-100" />
              <div className="flex items-center gap-2">
                <span className="font-bold text-base tracking-tight text-[var(--text-primary)]">
                  podbox
                </span>
                <span className="hidden sm:inline-block text-[10px] font-mono text-[var(--text-muted)]/60 font-normal">
                  v{version}
                </span>
              </div>
            </a>
          </div>

          {/* Center: Desktop Nav Links */}
          <nav className="hidden lg:flex items-center gap-0.5 xl:gap-1 text-[13px]">
            {navLinks.map((link) => {
              const Icon = link.icon;
              const isActive = isLinkActive(link, currentPath);

              return (
                <a
                  key={link.path}
                  href={withBase(link.path)}
                  className={`px-2.5 xl:px-3 py-1.5 rounded-[3px] whitespace-nowrap transition-colors cursor-pointer flex items-center gap-1.5 ${
                    isActive
                      ? 'text-[var(--accent-mauve)] font-semibold bg-[var(--bg-surface0)]'
                      : 'text-[var(--text-subtext)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-surface0)]/50'
                  }`}
                >
                  <Icon
                    className={`hidden xl:block w-3.5 h-3.5 ${
                      isActive
                        ? 'text-[var(--accent-mauve)]'
                        : 'text-[var(--text-muted)]'
                    }`}
                  />
                  <span>{link.label}</span>
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
              className="flex items-center gap-2 px-2.5 py-1.5 rounded-md bg-[var(--bg-base)] border border-[var(--border)] text-[13px] text-[var(--text-muted)] hover:border-[var(--border-focus)] hover:text-[var(--text-subtext)] transition-colors cursor-pointer sm:min-w-[11rem]"
              aria-label="Search the docs (press / or Ctrl K)"
            >
              <Search className="w-3.5 h-3.5 shrink-0" />
              <span className="hidden sm:inline">Search</span>
              <kbd className="hidden sm:flex ml-auto items-center gap-0.5 text-[10.5px] font-mono px-1.5 py-px rounded border border-[var(--border)] text-[var(--text-muted)]">
                {modKey} K
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
        <NavbarDrawer
          currentPath={currentPath}
          version={version}
          isDark={isDark}
          onToggleTheme={toggleTheme}
          onClose={() => setMobileMenuOpen(false)}
          onOpenSearch={openSearch}
        />
      )}
    </>
  );
}
