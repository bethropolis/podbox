import React from 'react';
import { marked, type Token } from 'marked';
import { stripFrontmatter } from '../base';
import { createSlugger, plainHeadingText } from './markdown/links';
import { renderToken, type AdmonitionToken, type DetailsToken } from './markdown/blocks';

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

      // Collapsible <details> blocks. The open/summary/close lines become
      // sentinels; everything between them flows through the normal path
      // (including admonitions), and the grouping phase below assembles a
      // synthetic `details` token. Raw HTML would otherwise hit the `html`
      // case in renderToken and render as nothing.
      if (/^<details>\s*$/.test(line)) {
        result.push('\n<!-- DETAILS -->\n');
        i++;
        continue;
      }
      {
        const summary = line.match(/^<summary>(.*)<\/summary>\s*$/);
        if (summary) {
          result.push(`\n<!-- SUMMARY:${summary[1]} -->\n`);
          i++;
          continue;
        }
      }
      if (/^<\/details>\s*$/.test(line)) {
        result.push('\n<!-- /DETAILS -->\n');
        i++;
        continue;
      }

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
    const slugger = createSlugger();

    const grouped: Token[] = [];
    let i = 0;
    while (i < rawTokens.length) {
      const token = rawTokens[i] as Token & Partial<AdmonitionToken> & Partial<DetailsToken>;
      if (token.type === 'html') {
        // Collapsible <details>: gather everything up to the close sentinel.
        // The summary arrives as its own sentinel and is lifted out; the
        // rest lexed normally, so code blocks and admonitions work inside.
        if (/^<!-- DETAILS -->\s*$/.test(token.text || '')) {
          const inner: Token[] = [];
          let summary = '';
          i++;
          while (i < rawTokens.length) {
            const next = rawTokens[i] as Token;
            if (next.type === 'html' && /^<!-- \/DETAILS -->\s*$/.test(next.text || '')) {
              break;
            }
            if (next.type === 'html') {
              const sum = (next.text || '').match(/^<!-- SUMMARY:(.*) -->\s*$/);
              if (sum) {
                summary = sum[1];
                i++;
                continue;
              }
            }
            inner.push(next);
            i++;
          }
          i++; // consume the closing sentinel
          token.type = 'details' as Token['type'];
          token.detSummary = summary;
          token.detInner = inner;
          grouped.push(token);
          continue;
        }
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
        const headingId = slugger(plainHeadingText(token.text));
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
    <div className="markdown-body text-[var(--text-primary)]">
      {tokens.map((token, index) => renderToken(token, index))}
    </div>
  );
}
