import React from 'react';
import { Copy, Terminal } from 'lucide-react';

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

  const highlightLine = (line: string, lang: string) => {
    const trimmed = line.trim();
    if (trimmed.startsWith('#') || trimmed.startsWith('//') || trimmed.startsWith(';')) {
      return <span className="syntax-comment">{line}</span>;
    }

    if (lang === 'toml' || lang === 'ini' || lang === 'systemd') {
      if (trimmed.startsWith('[') && trimmed.endsWith(']')) {
        return <span className="syntax-section font-semibold">{line}</span>;
      }
      const eqIdx = line.indexOf('=');
      if (eqIdx !== -1) {
        const key = line.slice(0, eqIdx);
        const value = line.slice(eqIdx + 1);
        return (
          <>
            <span className="syntax-variable">{key}</span>
            <span className="syntax-operator">=</span>
            <span className={value.includes('"') ? 'syntax-string' : 'syntax-number'}>
              {value}
            </span>
          </>
        );
      }
    }

    if (lang === 'bash' || lang === 'sh') {
      if (line.trim().startsWith('$ ')) {
        const cmd = line.trim().slice(2);
        return (
          <>
            <span className="text-[var(--accent-mauve)] font-bold select-none">$ </span>
            <span className="text-[var(--text-primary)] font-medium">{cmd}</span>
          </>
        );
      }
      if (line.trim().startsWith('podbox ') || line.trim().startsWith('podman ') || line.trim().startsWith('systemctl ') || line.trim().startsWith('curl ') || line.trim().startsWith('brew ') || line.trim().startsWith('paru ') || line.trim().startsWith('cargo ')) {
        const parts = line.split(' ');
        const first = parts[0];
        const rest = parts.slice(1).join(' ');
        return (
          <>
            <span className="text-[var(--accent-blue)] font-bold">{first} </span>
            <span className="text-[var(--text-primary)]">{rest}</span>
          </>
        );
      }
    }

    return <span>{line}</span>;
  };

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
    <div className={`overflow-x-auto text-[13px] leading-relaxed font-mono ${bare ? 'p-4 sm:p-5' : 'p-3.5'}`}>
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
                {highlightLine(line, language)}
              </span>
            </div>
          ))}
        </code>
      </pre>
    </div>
  );

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
    <div className="my-4 rounded-[4px] border border-[var(--border)] bg-[var(--bg-mantle)] overflow-hidden shadow-sm">
      {/* Titlebar Chrome */}
      <div className="flex items-center justify-between px-3.5 py-2 border-b border-[var(--border)] bg-[var(--bg-crust)]/80 select-none">
        <div className="flex items-center gap-2">
          <div className="flex items-center gap-1.5 mr-2">
            <span className="w-2.5 h-2.5 rounded-full bg-[#f38ba8]/90 inline-block" />
            <span className="w-2.5 h-2.5 rounded-full bg-[#f9e2af]/90 inline-block" />
            <span className="w-2.5 h-2.5 rounded-full bg-[#a6e3a1]/90 inline-block" />
          </div>
          {filename ? (
            <span className="text-xs font-mono text-[var(--text-subtext)] flex items-center gap-1.5">
              <Terminal className="w-3 h-3 text-[var(--accent-mauve)]" />
              {filename}
            </span>
          ) : (
            <span className="text-xs font-mono text-[var(--text-muted)] uppercase tracking-wider">
              {language || 'term'}
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
