import React from 'react';
import { marked, type Token } from 'marked';
import { ArrowUpRight } from 'lucide-react';
import { withBase } from '../../base';
import { cleanMarkdownLink, resolveAssetUrl } from './links';

const LINK =
  'text-[var(--accent-blue)] underline underline-offset-[3px] decoration-[var(--accent-blue)]/35 hover:decoration-[var(--accent-blue)] transition-colors';

// Inline HTML the docs actually use. Anything else is reduced to its text so
// a stray tag never shows up literally on the page.
function renderInlineHtml(raw: string, key: number): React.ReactNode {
  if (/^<br\s*\/?>$/i.test(raw)) return <br key={key} />;
  return null;
}

function renderTokens(tokens: Token[]): React.ReactNode[] {
  const out: React.ReactNode[] = [];
  for (let i = 0; i < tokens.length; i++) {
    const token = tokens[i] as any;
    const key = i;

    // <kbd>X</kbd> arrives as three tokens: open tag, text, close tag.
    if (token.type === 'html' && /^<kbd>$/i.test(token.raw)) {
      const inner: Token[] = [];
      let j = i + 1;
      while (j < tokens.length && !((tokens[j] as any).type === 'html' && /^<\/kbd>$/i.test((tokens[j] as any).raw))) {
        inner.push(tokens[j]);
        j++;
      }
      out.push(
        <kbd
          key={key}
          className="inline-block rounded border border-[var(--border-focus)] border-b-2 bg-[var(--bg-mantle)] px-1.5 py-px font-mono text-[0.8em] text-[var(--text-primary)]"
        >
          {renderTokens(inner)}
        </kbd>,
      );
      i = j;
      continue;
    }

    switch (token.type) {
      case 'codespan':
        out.push(
          <code
            key={key}
            className="rounded-[4px] border border-[var(--border)] bg-[var(--bg-mantle)] px-[0.35em] py-[0.1em] font-mono text-[0.86em] text-[var(--text-primary)] [overflow-wrap:anywhere]"
          >
            {token.text}
          </code>,
        );
        break;
      case 'strong':
        out.push(
          <strong key={key} className="font-semibold text-[var(--text-primary)]">
            {renderTokens(token.tokens ?? [])}
          </strong>,
        );
        break;
      case 'em':
        out.push(
          <em key={key} className="italic">
            {renderTokens(token.tokens ?? [])}
          </em>,
        );
        break;
      case 'del':
        out.push(
          <del key={key} className="text-[var(--text-muted)] decoration-[var(--text-muted)]">
            {renderTokens(token.tokens ?? [])}
          </del>,
        );
        break;
      case 'br':
        out.push(<br key={key} />);
        break;
      case 'link': {
        const { isInternal, url } = cleanMarkdownLink(token.href);
        if (isInternal) {
          // Bare #anchors stay untouched: withBase would turn them into
          // `/podbox/#…` and kick the reader back to the site root.
          out.push(
            <a key={key} href={url.startsWith('#') ? url : withBase(url)} className={LINK}>
              {renderTokens(token.tokens ?? [])}
            </a>,
          );
        } else {
          out.push(
            <a key={key} href={url} target="_blank" rel="noopener noreferrer" className={LINK}>
              {renderTokens(token.tokens ?? [])}
              <ArrowUpRight className="inline w-3 h-3 ml-0.5 -mt-0.5 opacity-60" aria-hidden="true" />
            </a>,
          );
        }
        break;
      }
      case 'image':
        out.push(
          <img
            key={key}
            src={resolveAssetUrl(token.href)}
            alt={token.text || token.title || ''}
            loading="lazy"
            className="my-3 max-w-full rounded-md border border-[var(--border)]"
          />,
        );
        break;
      case 'html':
        out.push(renderInlineHtml(token.raw.trim(), key));
        break;
      case 'escape':
        out.push(token.text);
        break;
      case 'text':
        // Text tokens can carry their own inline children (e.g. inside list
        // items); render those so formatting survives.
        out.push(
          token.tokens?.length ? (
            <React.Fragment key={key}>{renderTokens(token.tokens)}</React.Fragment>
          ) : (
            <React.Fragment key={key}>{token.text}</React.Fragment>
          ),
        );
        break;
      default:
        out.push(<React.Fragment key={key}>{token.text ?? token.raw ?? ''}</React.Fragment>);
    }
  }
  return out;
}

export function renderInline(text: string): React.ReactNode[] {
  return renderTokens(marked.Lexer.lexInline(text));
}

export function renderInlineTokens(tokens: Token[]): React.ReactNode[] {
  return renderTokens(tokens);
}
