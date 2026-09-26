// Sidebar order mirrors mkdocs.yml `nav` so URLs and IA survive migration.
// `slug` is the content id without `.md`; 'index' renders at /docs/.
export interface NavEntry {
  slug: string;
  label: string;
}

export const docsNav: NavEntry[] = [
  { slug: 'index', label: 'Home' },
  { slug: 'getting-started', label: 'Getting Started' },
  { slug: 'cli', label: 'CLI Reference' },
  { slug: 'config', label: 'Configuration' },
  { slug: 'baked-in-packages', label: 'Baked-in Base Packages' },
  { slug: 'architecture', label: 'Architecture' },
  { slug: 'export', label: 'Desktop Integration' },
  { slug: 'guest', label: 'Guest Daemon' },
  { slug: 'dbus-proxy', label: 'D-Bus Proxy' },
  { slug: 'quadlet', label: 'Quadlet Reference' },
  { slug: 'protocol', label: 'Protocol' },
  { slug: 'troubleshooting', label: 'Troubleshooting' },
];

export function docsHref(slug: string): string {
  const base = import.meta.env.BASE_URL;
  return slug === 'index' ? `${base}docs/` : `${base}docs/${slug}/`;
}
