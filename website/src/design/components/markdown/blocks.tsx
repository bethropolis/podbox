import React from 'react';
import type { Token, Tokens } from 'marked';
import { ChevronDown, Hash } from 'lucide-react';
import { TerminalCodeBlock } from '../TerminalCodeBlock';
import { Admonition } from '../Admonition';
import { renderInline, renderInlineTokens } from './inline';
import { diagramSwap, DiagramFigure, LogoFigure } from './diagrams';

// A list item's children, rendered in place. Tight lists give `text` tokens
// (inline content); loose lists, nested lists and fenced code inside an item
// give real block tokens, which go back through renderToken.
function renderListItem(item: Tokens.ListItem): React.ReactNode[] {
  return item.tokens.map((t, idx) => {
    if (t.type === 'text') {
      const tt = t as Tokens.Text;
      return <React.Fragment key={idx}>{tt.tokens ? renderInlineTokens(tt.tokens) : tt.text}</React.Fragment>;
    }
    return renderToken(t, idx);
  });
}
import { slugify } from './links';

export type AdmonitionToken = Token & { admType: string; admTitle: string; admInner: Token[] };
export type DetailsToken = Token & { detSummary: string; detInner: Token[] };

export function renderToken(token: Token, index: number): React.ReactNode {
  if ((token as any).type === 'admonition') {
    const adm = token as AdmonitionToken;
    return (
      <Admonition key={index} type={adm.admType as any} title={adm.admTitle}>
        {adm.admInner.map((t, idx) => renderToken(t, idx))}
      </Admonition>
    );
  }
  if ((token as any).type === 'details') {
    const det = token as DetailsToken;
    return (
      <details
        key={index}
        className="my-5 rounded-md border border-[var(--border)] group"
      >
        <summary className="px-4 py-3 text-sm text-[var(--text-primary)] cursor-pointer list-none flex items-center gap-2 hover:text-[var(--accent-mauve)] transition-colors [&::-webkit-details-marker]:hidden">
          <ChevronDown className="w-4 h-4 text-[var(--text-muted)] shrink-0 transition-transform group-open:rotate-180" />
          <span>{renderInline(det.detSummary)}</span>
        </summary>
        <div className="px-4 pb-3 pt-1 border-t border-[var(--border)]">
          {det.detInner.map((t, idx) => renderToken(t, idx))}
        </div>
      </details>
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
            className="group text-3xl sm:text-[2.25rem] font-semibold text-[var(--text-primary)] mt-2 mb-6 tracking-[-0.03em] leading-tight flex items-center gap-2"
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
            className="group text-xl sm:text-2xl font-semibold tracking-[-0.02em] text-[var(--text-primary)] mt-12 mb-3 pt-6 border-t border-[var(--border)]/70 flex items-center gap-2"
          >
            <span>{headingContent}</span>
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
            className="group text-base sm:text-lg font-semibold tracking-[-0.01em] text-[var(--text-primary)] mt-8 mb-2 flex items-center gap-2"
          >
            <span>{headingContent}</span>
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
          className="text-sm sm:text-base font-semibold text-[var(--text-primary)] mt-6 mb-2"
        >
          {headingContent}
        </h4>
      );
    }

    case 'paragraph': {
      // Admonitions are grouped at the token-stream level (see
      // MarkdownRenderer); the paragraph-text match the design relied on
      // never fires. Raw <picture> diagram blocks become framed figures.
      const diagram = diagramSwap(token.text);
      if (diagram) {
        if (diagram === 'logo') {
          return <LogoFigure key={index} />;
        }
        return <DiagramFigure key={index} name={diagram.name} alt={diagram.alt} />;
      }

      return (
        <p
          key={index}
          className="my-4 text-[15px] leading-7 text-[var(--text-subtext)]"
        >
          {renderInline(token.text)}
        </p>
      );
    }

    case 'code': {
      let filename: string | undefined;
      // Unlabelled fences are usually trees, paths or output: no highlighting.
      let lang = token.lang || 'text';
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
          className="my-6 overflow-x-auto rounded-md border border-[var(--border)] [&_code]:whitespace-nowrap [&_code]:[overflow-wrap:normal]"
        >
          <table className="w-full text-left border-collapse text-[13px] sm:text-sm">
            <thead>
              <tr className="border-b border-[var(--border)] bg-[var(--bg-mantle)]">
                {tableToken.header.map((headerCell, hIdx) => (
                  <th
                    key={hIdx}
                    className="px-3.5 py-2.5 text-[13px] font-medium text-[var(--text-primary)] whitespace-nowrap"
                  >
                    {renderInline(headerCell.text)}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody className="divide-y divide-[var(--border)]">
              {tableToken.rows.map((rowCells, rIdx) => (
                <tr key={rIdx}>
                  {rowCells.map((cell, cIdx) => (
                    <td
                      key={cIdx}
                      className={`px-3.5 py-2.5 align-top leading-relaxed text-[var(--text-subtext)] ${
                        cIdx === 0 ? 'text-[var(--text-primary)]' : ''
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
      const Tag = listToken.ordered ? 'ol' : 'ul';
      const start = listToken.ordered && listToken.start !== '' && listToken.start !== 1 ? Number(listToken.start) : undefined;
      return (
        <Tag
          key={index}
          start={start}
          className={`my-4 space-y-2 pl-6 text-[15px] leading-7 text-[var(--text-subtext)] marker:text-[var(--text-muted)] ${
            listToken.ordered ? 'list-decimal marker:font-mono marker:text-[0.85em]' : 'list-disc'
          } [&_ul]:my-2 [&_ol]:my-2 [&>li>p]:my-1`}
        >
          {listToken.items.map((item, itemIdx) => (
            <li key={itemIdx} className="pl-1">
              {item.task && (
                <input type="checkbox" checked={!!item.checked} readOnly disabled className="mr-2 align-middle accent-[var(--accent-mauve)]" />
              )}
              {renderListItem(item)}
            </li>
          ))}
        </Tag>
      );
    }

    case 'blockquote': {
      return (
        <blockquote
          key={index}
          className="my-5 border-l-2 border-[var(--border-focus)] pl-4 text-[var(--text-subtext)] [&>p]:my-2"
        >
          {token.tokens?.map((t: Token, idx: number) => renderToken(t, idx))}
        </blockquote>
      );
    }

    case 'hr': {
      return <hr key={index} className="my-10 border-[var(--border)]" />;
    }

    case 'html': {
      const diagram = diagramSwap(token.text);
      if (diagram === 'logo') {
        return <LogoFigure key={index} />;
      }
      if (diagram) {
        return <DiagramFigure key={index} name={diagram.name} alt={diagram.alt} />;
      }
      return null;
    }

    default:
      return null;
  }
}
