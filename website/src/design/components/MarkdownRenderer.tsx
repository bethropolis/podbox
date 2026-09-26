import React from 'react';
import { marked, type Token, type Tokens } from 'marked';
import { TerminalCodeBlock } from './TerminalCodeBlock';
import { Admonition } from './Admonition';
import { Hash, ExternalLink } from 'lucide-react';
import { withBase, githubBlob, stripFrontmatter } from '../base';

interface MarkdownRendererProps {
  content: string;
}

function slugify(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^\w\s-]/g, '')
    .trim()
    .replace(/\s+/g, '-');
}

function cleanMarkdownLink(href: string): { isInternal: boolean; url: string } {
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

function resolveAssetUrl(src: string): string {
  if (!src) return '';
  if (src.startsWith('http://') || src.startsWith('https://')) return src;
  const clean = src.replace(/^(docs\/)?assets\//, 'assets/');
  return withBase(clean.startsWith('/') ? clean : `/${clean}`);
}

// architecture.md embeds diagrams as raw <picture> blocks. `marked` emits
// them as paragraph or html tokens depending on context; either way, swap
// them for the framed figure the design intends. Returns null when the text
// holds no known diagram.
const DIAGRAMS: Record<string, string> = {
  'architecture-build': 'podbox build-time architecture',
  'architecture-runtime': 'podbox runtime architecture',
  'codegen_pipeline': 'Codegen pipeline',
  'socket_protocol': 'Host-guest socket protocol',
  'runtime_flow': 'Runtime flow',
  'how_it_works': 'How podbox works',
};

function diagramSwap(text: string): { src: string; alt: string } | 'logo' | null {
  if (!text.includes('<picture>')) return null;
  if (text.includes('podbox-logo')) return 'logo';
  for (const [name, alt] of Object.entries(DIAGRAMS)) {
    if (text.includes(name)) return { src: withBase(`/assets/${name}.svg`), alt };
  }
  return null;
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
      const token = rawTokens[i] as Token & { admType?: string; admTitle?: string; admInner?: Token[] };
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

  const renderInline = (text: string) => {
    // Parse inline formatting: code, bold, link, text
    const inlineTokens = marked.Lexer.lexInline(text);

    return inlineTokens.map((token, index) => {
      if (token.type === 'codespan') {
        return (
          <code
            key={index}
            className="px-1.5 py-0.5 rounded-[2px] bg-[var(--bg-surface0)] text-[var(--accent-mauve)] text-[12.5px] border border-[var(--border)] font-mono"
          >
            {token.text}
          </code>
        );
      }
      if (token.type === 'strong') {
        return (
          <strong key={index} className="font-bold text-[var(--text-primary)]">
            {renderInline(token.text)}
          </strong>
        );
      }
      if (token.type === 'em') {
        return (
          <em key={index} className="italic text-[var(--accent-peach)]">
            {renderInline(token.text)}
          </em>
        );
      }
      if (token.type === 'link') {
        const { isInternal, url } = cleanMarkdownLink(token.href);
        if (isInternal) {
          return (
            <a
              key={index}
              href={withBase(url)}
              className="text-[var(--accent-blue)] underline underline-offset-4 decoration-[var(--accent-blue)]/50 hover:decoration-[var(--accent-blue)] transition-colors cursor-pointer"
            >
              {renderInline(token.text)}
            </a>
          );
        }
        return (
          <a
            key={index}
            href={url}
            target="_blank"
            rel="noopener noreferrer"
            className="text-[var(--accent-blue)] inline-flex items-center gap-0.5 underline underline-offset-4 decoration-[var(--accent-blue)]/50 hover:decoration-[var(--accent-blue)]"
          >
            {renderInline(token.text)}
            <ExternalLink className="w-3 h-3 ml-0.5 opacity-70" />
          </a>
        );
      }
      if (token.type === 'image') {
        const resolvedSrc = resolveAssetUrl(token.href);
        return (
          <img
            key={index}
            src={resolvedSrc}
            alt={(token as any).title || (token as any).text || ''}
            className="my-3 max-w-full rounded-[3px] border border-[var(--border)]"
          />
        );
      }
      const rawText = 'text' in token ? (token as any).text : ('raw' in token ? (token as any).raw : '');
      return <span key={index}>{rawText}</span>;
    });
  };

  const renderToken = (token: Token, index: number): React.ReactNode => {
    if ((token as any).type === 'admonition') {
      const adm = token as Token & { admType: string; admTitle: string; admInner: Token[] };
      return (
        <Admonition key={index} type={adm.admType as any} title={adm.admTitle}>
          {adm.admInner.map((t, idx) => renderToken(t, idx))}
        </Admonition>
      );
    }
    switch (token.type) {
      case 'heading': {
        const id = (token as any).headingId || slugify(token.text);
        const headingContent = renderInline(token.text);

        if (token.depth === 1) {
          return (
            <h1
              key={index}
              id={id}
              className="group text-2xl sm:text-3xl font-bold text-[var(--text-primary)] mt-6 mb-4 tracking-tight pb-2 border-b border-[var(--border)] flex items-center justify-between"
            >
              <span>{headingContent}</span>
              <a
                href={`#${id}`}
                className="opacity-0 group-hover:opacity-100 text-[var(--text-muted)] hover:text-[var(--accent-mauve)] transition-opacity"
                title="Direct link"
              >
                <Hash className="w-5 h-5" />
              </a>
            </h1>
          );
        }
        if (token.depth === 2) {
          return (
            <h2
              key={index}
              id={id}
              className="group text-xl sm:text-2xl font-bold text-[var(--text-primary)] mt-8 mb-3 pb-1 border-b border-[var(--border)]/70 flex items-center justify-between"
            >
              <span className="flex items-center gap-2">
                <span className="text-[var(--accent-mauve)] text-base select-none">#</span>
                {headingContent}
              </span>
              <a
                href={`#${id}`}
                className="opacity-0 group-hover:opacity-100 text-[var(--text-muted)] hover:text-[var(--accent-mauve)] transition-opacity"
                title="Direct link"
              >
                <Hash className="w-4 h-4" />
              </a>
            </h2>
          );
        }
        if (token.depth === 3) {
          return (
            <h3
              key={index}
              id={id}
              className="group text-base sm:text-lg font-bold text-[var(--text-primary)] mt-6 mb-2 flex items-center justify-between"
            >
              <span className="flex items-center gap-2">
                <span className="text-[var(--accent-blue)] text-sm select-none">##</span>
                {headingContent}
              </span>
              <a
                href={`#${id}`}
                className="opacity-0 group-hover:opacity-100 text-[var(--text-muted)] hover:text-[var(--accent-mauve)] transition-opacity"
                title="Direct link"
              >
                <Hash className="w-3.5 h-3.5" />
              </a>
            </h3>
          );
        }
        return (
          <h4
            key={index}
            id={id}
            className="text-sm sm:text-base font-bold text-[var(--text-primary)] mt-4 mb-2"
          >
            {headingContent}
          </h4>
        );
      }

      case 'paragraph': {
        // Admonitions are grouped at the token-stream level (see above);
        // the paragraph-text match the design relied on never fires.
        // Raw <picture> diagram blocks become framed figures.
        const diagram = diagramSwap(token.text);
        if (diagram) {
          if (diagram === 'logo') {
            return (
              <div key={index} className="my-6 flex justify-center">
                <img src={withBase('/assets/podbox-logo.svg')} alt="podbox logo" className="max-w-[320px] h-auto" />
              </div>
            );
          }
          return (
            <div key={index} className="my-6 p-4 rounded-[3px] border border-[var(--border)] bg-[var(--bg-mantle)] flex justify-center">
              <img
                src={diagram.src}
                alt={diagram.alt}
                className="w-full max-w-[780px] h-auto"
              />
            </div>
          );
        }

        return (
          <p
            key={index}
            className="my-3 text-sm sm:text-[14px] leading-relaxed text-[var(--text-primary)]"
          >
            {renderInline(token.text)}
          </p>
        );
      }

      case 'code': {
        let filename: string | undefined;
        let lang = token.lang || 'bash';
        if (lang.includes(':')) {
          const parts = lang.split(':');
          lang = parts[0];
          filename = parts[1];
        }
        return (
          <TerminalCodeBlock
            key={index}
            code={token.text}
            language={lang}
            filename={filename}
            showLineNumbers={token.text.split('\n').length > 5}
          />
        );
      }

      case 'table': {
        const tableToken = token as Tokens.Table;
        return (
          <div
            key={index}
            className="my-6 overflow-x-auto rounded-[3px] border border-[var(--border)] bg-[var(--bg-mantle)]"
          >
            <table className="w-full text-left border-collapse text-xs sm:text-sm font-mono">
              <thead>
                <tr className="border-b border-[var(--border)] bg-[var(--bg-crust)]">
                  {tableToken.header.map((headerCell, hIdx) => (
                    <th
                      key={hIdx}
                      className="p-3 font-semibold text-[var(--text-primary)]"
                    >
                      {renderInline(headerCell.text)}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody className="divide-y divide-[var(--border)]">
                {tableToken.rows.map((rowCells, rIdx) => (
                  <tr
                    key={rIdx}
                    className="hover:bg-[var(--bg-surface0)]/40 transition-colors"
                  >
                    {rowCells.map((cell, cIdx) => (
                      <td
                        key={cIdx}
                        className={`p-3 text-[var(--text-subtext)] ${
                          cIdx === 0 ? 'font-medium text-[var(--text-primary)]' : ''
                        }`}
                      >
                        {renderInline(cell.text)}
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        );
      }

      case 'list': {
        const listToken = token as Tokens.List;
        return (
          <ul
            key={index}
            className={`my-3 space-y-1.5 text-sm sm:text-[14px] text-[var(--text-primary)] ${
              listToken.ordered ? 'list-decimal' : 'list-disc'
            } pl-6 leading-relaxed`}
          >
            {listToken.items.map((item, itemIdx) => (
              <li key={itemIdx} className="leading-relaxed">
                {renderInline(item.text)}
              </li>
            ))}
          </ul>
        );
      }

      case 'blockquote': {
        return (
          <blockquote
            key={index}
            className="my-4 border-l-2 border-[var(--accent-mauve)] pl-4 py-1 text-sm italic text-[var(--text-subtext)] bg-[var(--bg-mantle)]/50"
          >
            {token.tokens?.map((t: Token, idx: number) => renderToken(t, idx))}
          </blockquote>
        );
      }

      case 'hr': {
        return <hr key={index} className="my-8 border-[var(--border)]" />;
      }

      case 'html': {
        const diagram = diagramSwap(token.text);
        if (diagram === 'logo') {
          return (
            <div key={index} className="my-6 flex justify-center">
              <img src={withBase('/assets/podbox-logo.svg')} alt="podbox logo" className="max-w-[320px] h-auto" />
            </div>
          );
        }
        if (diagram) {
          return (
            <div key={index} className="my-6 p-4 rounded-[3px] border border-[var(--border)] bg-[var(--bg-mantle)] flex justify-center">
              <img
                src={diagram.src}
                alt={diagram.alt}
                className="w-full max-w-[780px] h-auto"
              />
            </div>
          );
        }
        return null;
      }

      default:
        return null;
    }
  };

  return (
    <div className="markdown-body font-mono text-[var(--text-primary)]">
      {tokens.map((token, index) => renderToken(token, index))}
    </div>
  );
}
