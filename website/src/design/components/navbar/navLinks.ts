import type { LucideIcon } from 'lucide-react';
import {
  Home,
  BookOpen,
  Terminal,
  FileCode2,
  Layers3,
  SlidersHorizontal,
} from 'lucide-react';

export interface NavLink {
  label: string;
  path: string;
  icon: LucideIcon;
  isStudio?: boolean;
}

export const navLinks: NavLink[] = [
  { label: 'Overview', path: '/', icon: Home },
  { label: 'Docs', path: '/docs', icon: BookOpen },
  { label: 'Quick Start', path: '/docs/getting-started', icon: Terminal },
  { label: 'Configuration', path: '/docs/config', icon: FileCode2 },
  { label: 'Architecture', path: '/docs/architecture', icon: Layers3 },
  { label: 'Studio', path: '/studio', icon: SlidersHorizontal, isStudio: true },
];

function matches(path: string, currentPath: string): boolean {
  if (path === '/') return currentPath === '/';
  return currentPath === path || currentPath.startsWith(`${path}/`);
}

// Only the most specific link is active: on /docs/getting-started that is
// "Quick Start", not "Docs" as well.
export function isLinkActive(link: NavLink, currentPath: string): boolean {
  if (!matches(link.path, currentPath)) return false;
  return !navLinks.some(
    (other) => other.path.length > link.path.length && matches(other.path, currentPath),
  );
}
