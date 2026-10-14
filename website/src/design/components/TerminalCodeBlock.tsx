import React from 'react';
import { Copy, FileText } from 'lucide-react';
import { highlightDocument, langFromFence } from '../../design/highlight';

interface TerminalCodeBlockProps {
  code: string;
  language?: string;
  filename?: string;
  showLineNumbers?: boolean;
  /**
   * Drop the terminal-window framing (title bar, traffic lights, outer
   * margin) and render only the code. Callers that already provide their own
   * heading or tab bar use this so the code is not boxed inside a box.
   */
  bare?: boolean;
}

export function TerminalCodeBlock({
  code,
  language = 'bash',
  filename,
  showLineNumbers = false,
  bare = false,
}: TerminalCodeBlockProps) {
  // Copy is handled by a single delegated document listener (see
  // SiteLayout copy script): the button carries the payload in data-code,
  // so this component stays static-renderable with zero client JS.
  const cleanCode = code.replace(/\r\n/g, '\n').trimEnd();

  // Same tokenizer as the Studio preview (design/highlight.tsx); unknown
  // fence labels render as plain text rather than guessing.
  const grammar = langFromFence(language);
  const highlighted = highlightDocument(cleanCode, grammar);

  const lines = cleanCode.split('\n');

  const copyButton = (positionClass = '') => (
    <button
      type="button"
      data-copy-btn
      data-code={cleanCode}
      className={`flex items-center gap-1.5 px-2 py-1 text-xs font-mono text-[var(--text-subtext)] hover:text-[var(--accent-mauve)] hover:bg-[var(--bg-surface0)] rounded-[2px] transition-colors cursor-pointer ${positionClass}`}
      title="Copy code to clipboard"
      aria-label="Copy code to clipboard"
    >
      <Copy className="w-3.5 h-3.5" data-copy-icon />
      <span className="text-[11px]" data-copy-label>copy</span>
    </button>
  );

  const codeBody = (
    <div className={`overflow-x-auto text-[12.5px] sm:text-[13px] leading-relaxed font-mono ${bare ? 'px-4 pb-4 pt-11 sm:p-5 sm:pr-24' : 'p-3.5'}`}>
      <pre className="m-0 p-0 font-mono">
        <code>
          {lines.map((line, idx) => (
            <div key={idx} className="table-row">
              {showLineNumbers && (
                <span className="table-cell pr-4 text-right select-none text-[var(--text-muted)] opacity-50 text-xs">
                  {idx + 1}
                </span>
              )}
              <span className="table-cell break-all">
                {/* A blank line rendered as-is produces no line box, so the
                    row collapses to zero height and the gap disappears from
                    the code while still being present in the copied text. A
                    non-breaking space keeps the line box. */}
                {line.trim() === '' ? '\u00A0' : highlighted[idx]}
              </span>
            </div>
          ))}
        </code>
      </pre>
    </div>
  );

  // Unlabelled output, trees and paths: no header row with nothing in it.
  if (!bare && !filename && grammar === 'text') {
    return (
      <div className="relative my-5 rounded-md border border-[var(--border)] bg-[var(--bg-mantle)]">
        {copyButton('absolute top-1.5 right-1.5 z-10')}
        <div className="overflow-x-auto py-3.5 pl-3.5 pr-20 text-[12.5px] sm:text-[13px] leading-relaxed font-mono">
          <pre className="m-0 p-0 font-mono"><code>{cleanCode}</code></pre>
        </div>
      </div>
    );
  }

  if (bare) {
    // No title bar: the caller labels the block. The copy button stays, pinned
    // to the top-right so the panel keeps its one useful affordance.
    return (
      <div className="relative">
        {copyButton('absolute top-2.5 right-2.5 z-10')}
        {codeBody}
      </div>
    );
  }

  return (
    <div className="my-5 rounded-md border border-[var(--border)] bg-[var(--bg-mantle)] overflow-hidden">
      {/* Label bar: filename or language plus copy. No traffic-light dots —
          they are decoration on every snippet and add up fast on long pages. */}
      <div className="flex items-center justify-between gap-3 pl-3.5 pr-1.5 py-1 border-b border-[var(--border)] select-none">
        <div className="flex min-w-0 items-center gap-2">
          {filename ? (
            <span className="flex min-w-0 items-center gap-1.5 text-xs font-mono text-[var(--text-subtext)]">
              <FileText className="w-3.5 h-3.5 shrink-0 text-[var(--text-muted)]" />
              <span className="truncate">{filename}</span>
            </span>
          ) : (
            <span className="text-xs font-mono text-[var(--text-muted)]">
              {grammar === 'text' ? '' : (language || '').toLowerCase()}
            </span>
          )}
        </div>

        {copyButton()}
      </div>

      {/* Code body */}
      {codeBody}
    </div>
  );
}
