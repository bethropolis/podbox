// Base-aware URL helper. Astro serves the site under `/podbox` (Pages +
// `base` config), so every root-absolute path in the vendored design must
// be prefixed. In dev `BASE_URL` is `/podbox/` as well.

export function withBase(path: string): string {
  const base = import.meta.env.BASE_URL || '/';
  if (/^https?:\/\//.test(path)) return path;
  const normalizedBase = base.endsWith('/') ? base.slice(0, -1) : base;
  if (path.startsWith(normalizedBase + '/') || path === normalizedBase) return path;
  return `${normalizedBase}${path.startsWith('/') ? path : `/${path}`}`;
}

export const GITHUB_REPO = 'https://github.com/bethropolis/podbox';

// Files carry `---\nkey: value\n---` frontmatter for metadata. `marked`
// would misread that as a setext h2, so strip it before lexing/rendering.
export function stripFrontmatter(raw: string): string {
  if (!raw.startsWith('---\n')) return raw;
  const end = raw.indexOf('\n---', 4);
  if (end === -1) return raw;
  return raw.slice(end + 4).replace(/^\n/, '');
}

// Files that only make sense on github.com (dev docs, README) resolve to
// blob URLs instead of dead site-relative links.
export function githubBlob(path: string): string {
  const clean = path.replace(/^\.\.\//, '');
  return `${GITHUB_REPO}/blob/main/${clean}`;
}
