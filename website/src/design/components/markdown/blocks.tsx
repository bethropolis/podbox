import React from 'react';
import type { Token, Tokens } from 'marked';
import { ChevronDown, Hash } from 'lucide-react';
import { TerminalCodeBlock } from '../TerminalCodeBlock';
import { Admonition } from '../Admonition';
import { renderInline } from './inline';
import { diagramSwap, DiagramFigure, LogoFigure } from './diagrams';
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
        className="my-4 rounded-[3px] border border-[var(--border)] bg-[var(--bg-mantle)] group"
      >
        <summary className="px-4 py-3 text-sm text-[var(--text-primary)] cursor-pointer list-none flex items-center gap-2 hover:text-[var(--accent-mauve)] transition-colors [&::-webkit-details-marker]:hidden">
          <ChevronDown className="w-4 h-4 text-[var(--text-muted)] shrink-0 transition-transform group-open:rotate-180" />
          <span>{renderInline(det.detSummary)}</span>
        </summary>
        <div className="px-4 pb-2 pt-1 border-t border-[var(--border)]">
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
            className="group text-xl sm:text-2xl font-bold text-[var(--text-primary)] mt-10 mb-3 pb-2 border-b border-[var(--border)]/70 flex items-center justify-between"
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
            className="group text-base sm:text-lg font-bold text-[var(--text-primary)] mt-8 mb-2 flex items-center justify-between"
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
          className="text-sm sm:text-base font-bold text-[var(--text-primary)] mt-4 mb-2"
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
        return <DiagramFigure key={index} src={diagram.src} alt={diagram.alt} />;
      }

      return (
        <p
          key={index}
          className="my-3 text-[15px] leading-7 text-[var(--text-subtext)]"
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
          className={`my-3 space-y-1.5 text-[15px] text-[var(--text-subtext)] ${
            listToken.ordered ? 'list-decimal' : 'list-disc'
          } pl-6 leading-7`}
        >
          {listToken.items.map((item, itemIdx) => (
            <li key={itemIdx} className="leading-7">
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
        return <LogoFigure key={index} />;
      }
      if (diagram) {
        return <DiagramFigure key={index} src={diagram.src} alt={diagram.alt} />;
      }
      return null;
    }

    default:
      return null;
  }
}
