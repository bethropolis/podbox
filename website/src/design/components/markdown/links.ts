import { githubBlob, withBase } from '../../base';

export function slugify(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^\w\s-]/g, '')
    .trim()
    .replace(/\s+/g, '-');
}

// Stateful heading slugger with GitHub-style dedup (foo, foo-1, foo-2).
// Single algorithm shared by the renderer and the build-time TOC
// extractor so anchors can never drift. Plain-text normalization must
// match MarkdownRenderer's heading pass exactly.
export function plainHeadingText(raw: string): string {
  return raw
    .replace(/\[([^\]]+)\]\([^)]+\)/g, '$1')
    .replace(/`([^`]+)`/g, '$1');
}

export function createSlugger(): (text: string) => string {
  const counts = new Map<string, number>();
  return (text: string) => {
    const base = slugify(text);
    const count = counts.get(base) || 0;
    counts.set(base, count + 1);
    return count === 0 ? base : `${base}-${count}`;
  };
}

export function cleanMarkdownLink(href: string): { isInternal: boolean; url: string } {
  if (!href) return { isInternal: false, url: '#' };
  if (href.startsWith('http://') || href.startsWith('https://')) {
    return { isInternal: false, url: href };
  }

  // Links that only make sense on github.com (dev docs, README) go to blobs.
  if (href.startsWith('../')) {
    return { isInternal: false, url: githubBlob(href) };
  }

  // Handle docs relative links
  let clean = href.replace(/^docs\//, '');
  const hashIdx = clean.indexOf('#');
  let anchor = '';
  if (hashIdx !== -1) {
    anchor = clean.slice(hashIdx);
    clean = clean.slice(0, hashIdx);
  }

  if (clean.endsWith('.md')) {
    const pageId = clean.replace(/\.md$/, '');
    if (pageId === 'index') {
      return { isInternal: true, url: `/docs${anchor}` };
    }
    return { isInternal: true, url: `/docs/${pageId}${anchor}` };
  }

  if (href.startsWith('#')) {
    return { isInternal: true, url: href };
  }

  return { isInternal: true, url: href };
}

export function resolveAssetUrl(src: string): string {
  if (!src) return '';
  if (src.startsWith('http://') || src.startsWith('https://')) return src;
  const clean = src.replace(/^(docs\/)?assets\//, 'assets/');
  return withBase(clean.startsWith('/') ? clean : `/${clean}`);
}
