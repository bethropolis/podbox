// Ranking for the site search palette. Pure functions, no React.
//
// Every query word must match somewhere in an entry (AND). A word matches a
// field when it is a substring of it; matches at a word start score higher,
// and a word in the title outweighs one in the page name or body text. Words
// of 4+ letters that match nothing get one more chance with a single typo
// against the title's words, so "snapshto" still finds snapshot.

import type { SearchEntry } from './searchIndex';

export type { SearchEntry };

export interface Hit {
  entry: SearchEntry;
  score: number;
  terms: string[];
}

const KIND_ORDER: Record<SearchEntry['kind'], number> = { command: 0, key: 1, page: 2, section: 3 };

export function tokenize(q: string): string[] {
  return q
    .toLowerCase()
    .split(/[\s,]+/)
    .map((t) => t.replace(/^\[|\]$/g, ''))
    .filter(Boolean);
}

const isWordStart = (hay: string, i: number) => i === 0 || /[\s._\-/[(`"'·]/.test(hay[i - 1]);

function fieldScore(hay: string, term: string, weight: number): number {
  let i = hay.indexOf(term);
  if (i === -1) return 0;
  let best = 0;
  while (i !== -1) {
    best = Math.max(best, isWordStart(hay, i) ? 1.6 : 1);
    if (best === 1.6) break;
    i = hay.indexOf(term, i + 1);
  }
  return best * weight;
}

/** True when a and b differ by at most one edit (insert, delete, substitute, swap). */
function withinOneEdit(a: string, b: string): boolean {
  if (a === b) return true;
  if (Math.abs(a.length - b.length) > 1) return false;
  let i = 0;
  while (i < a.length && i < b.length && a[i] === b[i]) i++;
  if (a.length === b.length) {
    if (a.slice(i + 1) === b.slice(i + 1)) return true; // substitution
    return a[i] === b[i + 1] && a[i + 1] === b[i] && a.slice(i + 2) === b.slice(i + 2); // swap
  }
  const [short, long] = a.length < b.length ? [a, b] : [b, a];
  return short.slice(i) === long.slice(i + 1);
}

function fuzzyTitle(title: string, term: string): boolean {
  if (term.length < 4) return false;
  // Whole word with one typo, or a typo while still typing the word.
  return title
    .split(/[^a-z0-9_]+/)
    .some((w) => w.length >= 3 && (withinOneEdit(w, term) || withinOneEdit(w.slice(0, term.length), term)));
}

export function rank(entries: SearchEntry[], query: string, limit = 40): Hit[] {
  const terms = tokenize(query);
  if (terms.length === 0) return [];
  const phrase = query.trim().toLowerCase();
  const hits: Hit[] = [];

  for (const entry of entries) {
    const title = entry.title.toLowerCase();
    const page = entry.page.toLowerCase();
    const text = entry.text.toLowerCase();
    let score = 0;
    let ok = true;
    for (const term of terms) {
      const s = Math.max(fieldScore(title, term, 10), fieldScore(page, term, 3), fieldScore(text, term, 1.2));
      if (s > 0) {
        score += s;
      } else if (fuzzyTitle(title, term)) {
        score += 4;
      } else {
        ok = false;
        break;
      }
    }
    if (!ok) continue;
    if (title === phrase || title === `podbox ${phrase}`) score += 60;
    else if (title.startsWith(phrase)) score += 25;
    else if (title.includes(phrase)) score += 12;
    // Short, specific entries beat long sections that mention a word once.
    if (entry.kind === 'command' || entry.kind === 'key') score += 4;
    if (entry.kind === 'page') score += 6;
    hits.push({ entry, score, terms });
  }

  hits.sort((a, b) => b.score - a.score || KIND_ORDER[a.entry.kind] - KIND_ORDER[b.entry.kind] || a.entry.title.length - b.entry.title.length);
  // Many keys share a page anchor; keep results varied by capping repeats.
  const seen = new Map<string, number>();
  const out: Hit[] = [];
  for (const h of hits) {
    const k = `${h.entry.kind}:${h.entry.path}`;
    const n = seen.get(k) ?? 0;
    if (h.entry.kind === 'section' && n >= 1) continue;
    seen.set(k, n + 1);
    out.push(h);
    if (out.length >= limit) break;
  }
  return out;
}

/** A short window of `text` around the first matched term. */
export function snippet(text: string, terms: string[], width = 120): string {
  if (!text) return '';
  const lower = text.toLowerCase();
  let at = -1;
  for (const t of terms) {
    const i = lower.indexOf(t);
    if (i !== -1 && (at === -1 || i < at)) at = i;
  }
  if (at === -1 || text.length <= width) return text.length <= width ? text : `${text.slice(0, width).trimEnd()}…`;
  const start = Math.max(0, at - Math.floor(width / 3));
  const end = Math.min(text.length, start + width);
  return `${start > 0 ? '…' : ''}${text.slice(start, end).trim()}${end < text.length ? '…' : ''}`;
}

/** Split text into plain and matched parts for highlighting. */
export function highlightParts(text: string, terms: string[]): { text: string; match: boolean }[] {
  const usable = terms.filter((t) => t.length >= 2).map((t) => t.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'));
  if (usable.length === 0) return [{ text, match: false }];
  const split = new RegExp(`(${usable.join('|')})`, 'gi');
  const isMatch = new RegExp(`^(?:${usable.join('|')})$`, 'i');
  return text
    .split(split)
    .filter(Boolean)
    .map((part) => ({ text: part, match: isMatch.test(part) }));
}
