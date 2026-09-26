import React from 'react';
import { marked, type Token } from 'marked';
import { stripFrontmatter } from '../base';
import { slugify } from './markdown/links';
import { renderToken, type AdmonitionToken } from './markdown/blocks';

interface MarkdownRendererProps {
  content: string;
}

export function MarkdownRenderer({ content }: MarkdownRendererProps) {
  // Pre-process MkDocs admonitions and raw picture tags
  const processedContent = React.useMemo(() => {
    const lines = stripFrontmatter(content).split('\n');
    const result: string[] = [];
    let i = 0;

    while (i < lines.length) {
      const line = lines[i];

      // Admonitions check: !!! note, !!! tip, etc.
      const match = line.match(/^(!{3}|\?{3}\+?)\s+(note|tip|warning|info|danger|caution)(?:\s+"([^"]*)")?/);
      if (match) {
        const type = match[2];
        const title = match[3] || '';
        i++;
        const bodyLines: string[] = [];
        while (i < lines.length) {
          if (lines[i].startsWith('    ')) {
            bodyLines.push(lines[i].slice(4));
            i++;
          } else if (lines[i].trim() === '') {
            bodyLines.push('');
            i++;
          } else {
            break;
          }
        }
        result.push(`\n<!-- ADMONITION:${type}:${title} -->\n${bodyLines.join('\n')}\n<!-- /ADMONITION -->\n`);
      } else {
        result.push(line);
        i++;
      }
    }

    return result.join('\n');
  }, [content]);

  // Tokenize using marked and disambiguate duplicate heading slugs.
  // NOTE: marked splits the ADMONITION sentinel comments into standalone
  // `html` tokens, so group `open … /close` runs here into synthetic
  // `admonition` tokens (the paragraph-text regex the design relied on never
  // fires under marked 18).
  const tokens = React.useMemo(() => {
    const rawTokens = marked.lexer(processedContent);
    const slugCounts = new Map<string, number>();

    const grouped: Token[] = [];
    let i = 0;
    while (i < rawTokens.length) {
      const token = rawTokens[i] as Token & Partial<AdmonitionToken>;
      if (token.type === 'html') {
        const open = (token.text || '').match(/^<!-- ADMONITION:(note|tip|warning|info|danger|caution):(.*) -->\s*$/);
        if (open) {
          const inner: Token[] = [];
          i++;
          while (i < rawTokens.length) {
            const next = rawTokens[i] as Token;
            if (next.type === 'html' && /^<!-- \/ADMONITION -->\s*$/.test(next.text || '')) {
              break;
            }
            inner.push(next);
            i++;
          }
          i++; // consume the closing sentinel
          token.type = 'admonition' as Token['type'];
          token.admType = open[1] === 'caution' ? 'warning' : open[1];
          token.admTitle = open[2];
          token.admInner = inner;
          grouped.push(token);
          continue;
        }
        // Swallow stray sentinels so they never leak into the HTML.
        if (/^<!-- \/?ADMONITION/.test(token.text || '')) {
          i++;
          continue;
        }
      }
      if (token.type === 'heading') {
        let plainText = token.text
          .replace(/\[([^\]]+)\]\([^)]+\)/g, '$1')
          .replace(/`([^`]+)`/g, '$1');
        const baseSlug = slugify(plainText);
        const count = slugCounts.get(baseSlug) || 0;
        slugCounts.set(baseSlug, count + 1);
        const headingId = count === 0 ? baseSlug : `${baseSlug}-${count}`;
        grouped.push(Object.assign(token, { headingId }));
        i++;
        continue;
      }
      grouped.push(token);
      i++;
    }

    return grouped;
  }, [processedContent]);

  return (
    <div className="markdown-body font-mono text-[var(--text-primary)]">
      {tokens.map((token, index) => renderToken(token, index))}
    </div>
  );
}
