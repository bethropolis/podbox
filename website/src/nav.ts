// `slug` is the content id without `.md`; 'index' renders at /docs/.
export interface NavEntry {
  slug: string;
  label: string;
  /** Sidebar section. Entries of one group must stay contiguous. */
  group: string;
}

// Order is reading order: prev/next links follow it.
export const docsNav: NavEntry[] = [
  { slug: 'index', label: 'Overview', group: 'Start here' },
  { slug: 'getting-started', label: 'Getting started', group: 'Start here' },
  { slug: 'config', label: 'Configuration', group: 'Using podbox' },
  { slug: 'cli', label: 'CLI reference', group: 'Using podbox' },
  { slug: 'baked-in-packages', label: 'Base packages', group: 'Using podbox' },
  { slug: 'setup', label: 'Home setup', group: 'Using podbox' },
  { slug: 'export', label: 'Desktop integration', group: 'Using podbox' },
  { slug: 'dbus-proxy', label: 'D-Bus proxy', group: 'Using podbox' },
  { slug: 'architecture', label: 'Architecture', group: 'How it works' },
  { slug: 'guest', label: 'Guest daemon', group: 'How it works' },
  { slug: 'quadlet', label: 'Quadlet reference', group: 'How it works' },
  { slug: 'protocol', label: 'Socket protocol', group: 'How it works' },
  { slug: 'clients', label: 'Building clients', group: 'How it works' },
  { slug: 'troubleshooting', label: 'Troubleshooting', group: 'Help' },
];

export function docsHref(slug: string): string {
  const base = import.meta.env.BASE_URL;
  return slug === 'index' ? `${base}/docs/` : `${base}/docs/${slug}/`;
}
