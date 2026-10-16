// Prebuild sync (run with bun): keeps a single markdown source of truth.
// 1. Copies ../docs/*.md        -> src/content/docs/
// 2. Copies ../docs/assets/*     -> public/assets/
// 3. Copies install-online.sh    -> public/install.sh  (must keep serving /podbox/install.sh)
// 4. Regenerates src/design/searchIndex.ts (headings + full-text entries),
//    using the SAME slug algorithm as MarkdownRenderer so palette anchors
//    always match rendered heading ids.
import { readdirSync, readFileSync, writeFileSync, copyFileSync, mkdirSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { marked } from 'marked';

const ROOT = join(import.meta.dir, '..');
const DOCS_SRC = join(ROOT, '..', 'docs');
const CONTENT_DIR = join(ROOT, 'src', 'content', 'docs');
const ASSETS_DST = join(ROOT, 'public', 'assets');

mkdirSync(CONTENT_DIR, { recursive: true });
mkdirSync(ASSETS_DST, { recursive: true });

// Identical to slugify() in design/components/MarkdownRenderer.tsx
function slugify(text: string): string {
  return text.toLowerCase().replace(/[^\w\s-]/g, '').trim().replace(/\s+/g, '-');
}

function plainHeading(text: string): string {
  return text.replace(/\[([^\]]+)\]\([^)]+\)/g, '$1').replace(/`([^`]+)`/g, '$1');
}

function parseFrontmatter(raw: string): Record<string, string> {
  const out: Record<string, string> = {};
  if (!raw.startsWith('---\n')) return out;
  const end = raw.indexOf('\n---', 4);
  if (end === -1) return out;
  for (const line of raw.slice(4, end).split('\n')) {
    const m = line.match(/^(\w+):\s*(.*)$/);
    if (m) out[m[1]] = unquote(m[2].trim());
  }
  return out;
}

// Frontmatter values are often YAML-quoted (`description: "..."`). This is
// a line parser, not a YAML parser, so strip one pair of matching quotes;
// otherwise the quotes leak into the search index verbatim.
function unquote(value: string): string {
  if (value.length >= 2) {
    const first = value[0];
    const last = value[value.length - 1];
    if ((first === '"' && last === '"') || (first === "'" && last === "'")) {
      return value.slice(1, -1);
    }
  }
  return value;
}

interface Heading { id: string; text: string; level: number }
interface DocEntry { id: string; title: string; path: string; description: string; headings: Heading[] }

// Full-text search entries for the site palette. One per page, one per
// heading (with the text under it), one per config key in a reference table,
// and one per CLI command. Text is plain (markdown stripped) and capped so
// the lazily loaded index stays small.
interface SearchEntry {
  kind: 'page' | 'section' | 'key' | 'command';
  title: string;
  page: string;
  path: string;
  text: string;
}
const SECTION_TEXT_CAP = 900;

function plainInline(md: string): string {
  return md
    .replace(/!\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/\[([^\]]+)\]\([^)]*\)/g, '$1')
    .replace(/<[^>]+>/g, ' ')
    .replace(/[`*~]/g, '')
    // Only emphasis underscores; keep the ones inside names like pull_retry.
    .replace(/(^|\s)_+|_+(?=\s|$)/g, '$1')
    .replace(/\s+/g, ' ')
    .trim();
}

function tokenText(t: any): string {
  switch (t.type) {
    case 'paragraph':
    case 'text':
    case 'heading':
      return plainInline(t.text);
    case 'list':
      return t.items.map((i: any) => plainInline(i.text)).join(' · ');
    case 'code':
      return t.text.split('\n').slice(0, 8).join(' ');
    case 'table':
      return [t.header, ...t.rows].map((r: any[]) => r.map((c) => plainInline(c.text)).join(' ')).join(' · ');
    case 'blockquote':
      return (t.tokens ?? []).map(tokenText).join(' ');
    default:
      return '';
  }
}

const search: SearchEntry[] = [];
const entries: DocEntry[] = [];
const syncedDocs = new Set<string>();
for (const file of readdirSync(DOCS_SRC).filter((f) => f.endsWith('.md')).sort()) {
  const id = file.replace(/\.md$/, '');
  copyFileSync(join(DOCS_SRC, file), join(CONTENT_DIR, file));
  syncedDocs.add(file);

  const raw = readFileSync(join(DOCS_SRC, file), 'utf8');
  const fm = parseFrontmatter(raw);
  // Strip frontmatter before lexing: marked would read it as a setext h2.
  const body = raw.replace(/^---\n[\s\S]*?\n---\n?/, '');
  const tokens = marked.lexer(body);
  const counts = new Map<string, number>();
  const headings: Heading[] = [];
  let h1 = '';
  const path = id === 'index' ? '/docs' : `/docs/${id}`;
  // Sections: text accumulates under the most recent heading (h1-h3).
  const sections: { heading: Heading; parts: string[] }[] = [];
  let current: { heading: Heading; parts: string[] } | null = null;
  for (const t of tokens as any[]) {
    if (t.type === 'heading') {
      const text = plainHeading(t.text);
      if (t.depth === 1 && !h1) h1 = text;
      const base = slugify(text);
      const n = counts.get(base) ?? 0;
      counts.set(base, n + 1);
      const h = { id: n === 0 ? base : `${base}-${n}`, text, level: t.depth };
      headings.push(h);
      if (t.depth <= 3) {
        current = { heading: h, parts: [] };
        sections.push(current);
      }
      continue;
    }
    if (current && !(t.type === 'html' && /<picture>/.test(t.text ?? ''))) {
      const txt = tokenText(t);
      if (txt) current.parts.push(txt);
    }
    // Reference tables whose first column is `Key`: one entry per row.
    if (t.type === 'table' && current && /^key$/i.test(plainInline(t.header[0]?.text ?? ''))) {
      const sectionName = current.heading.text.replace(/^\[|\]$/g, '').replace(/\]\.?/, '.').replace(/[\[\]]/g, '');
      for (const row of t.rows) {
        const key = plainInline(row[0]?.text ?? '');
        if (!key) continue;
        const rest = row.slice(1).map((c: any) => plainInline(c.text)).filter(Boolean);
        search.push({
          kind: 'key',
          title: /^[a-z]/i.test(sectionName) && !/\s/.test(sectionName) ? `${sectionName}.${key}` : key,
          page: fm.title || h1 || id,
          path: `${path}#${current.heading.id}`,
          text: rest.join(' · '),
        });
      }
    }
  }
  const title = fm.title || h1 || id;
  entries.push({
    id,
    title,
    path,
    description: fm.description || '',
    headings: headings.filter((h) => h.level <= 3),
  });
  search.push({ kind: 'page', title, page: title, path, text: fm.description || '' });
  for (const sec of sections) {
    if (sec.heading.level === 1) continue;
    search.push({
      kind: 'section',
      title: sec.heading.text,
      page: title,
      path: `${path}#${sec.heading.id}`,
      text: sec.parts.join(' ').slice(0, SECTION_TEXT_CAP),
    });
  }

  // CLI commands from the "Command groups" table.
  if (id === 'cli') {
    const table = (tokens as any[]).find((t) => t.type === 'table' && /group/i.test(t.header[0]?.text ?? ''));
    for (const row of table?.rows ?? []) {
      const group = plainInline(row[0].text);
      for (const m of row[1].text.matchAll(/`([^`]+)`/g)) {
        search.push({
          kind: 'command',
          title: `podbox ${m[1]}`,
          page: 'CLI reference',
          path: `${path}#command-groups`,
          text: group,
        });
      }
    }
  }
}

// Assets + installer. Mirror the source: pages or assets deleted upstream
// must not linger as stale copies (deploys re-sync, but `bun run dev`
// serves these files directly, via the predev hook below).
for (const f of readdirSync(CONTENT_DIR).filter((f) => f.endsWith('.md'))) {
  if (!syncedDocs.has(f)) rmSync(join(CONTENT_DIR, f));
}
const syncedAssets = new Set<string>();
for (const f of readdirSync(join(DOCS_SRC, 'assets'))) {
  copyFileSync(join(DOCS_SRC, 'assets', f), join(ASSETS_DST, f));
  syncedAssets.add(f);
}
for (const f of readdirSync(ASSETS_DST)) {
  if (!syncedAssets.has(f)) rmSync(join(ASSETS_DST, f), { recursive: true });
}
copyFileSync(join(ROOT, '..', 'scripts', 'install-online.sh'), join(ROOT, 'public', 'install.sh'));

// Social cards: X/Discord scrapers reject SVG og:image, so render a
// 1200x630 PNG from the cover SVG (requires rsvg-convert, present in CI).
try {
  const proc = Bun.spawnSync([
    'rsvg-convert',
    '-w', '1200',
    join(ROOT, 'public', 'podbox-cover-image.svg'),
    '-o', join(ROOT, 'public', 'og-image.png'),
  ]);
  if (proc.exitCode !== 0) throw new Error('rsvg-convert failed');
  console.log('rendered og-image.png');
} catch {
  console.warn('rsvg-convert unavailable, skipping og-image.png');
}

const out =
  `// GENERATED by scripts/sync-content.ts — do not edit by hand.\n` +
  `export interface SearchHeading { id: string; text: string; level: number }\n` +
  `export interface SearchDoc { id: string; title: string; path: string; description: string; headings: SearchHeading[] }\n` +
  `export interface SearchEntry { kind: 'page' | 'section' | 'key' | 'command'; title: string; page: string; path: string; text: string }\n` +
  `export const SEARCH_DOCS: SearchDoc[] = ${JSON.stringify(entries, null, 2)};\n` +
  `export const SEARCH_ENTRIES: SearchEntry[] = ${JSON.stringify(search)};\n`;
writeFileSync(join(ROOT, 'src', 'design', 'searchIndex.ts'), out);
console.log(`synced ${entries.length} docs + assets + install.sh + search index (${search.length} entries)`);
