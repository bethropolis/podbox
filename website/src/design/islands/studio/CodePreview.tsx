import React from 'react';
import {
  Box,
  Check,
  Copy,
  Download,
  FileCode2,
  Terminal,
} from 'lucide-react';
import type { StudioState } from './useStudioState';
import { useOutputActions } from './useOutputActions';

const highlightCodeLine = (line: string, lang: string) => {
  const trimmed = line.trim();
  if (trimmed.startsWith('#') || trimmed.startsWith('//') || trimmed.startsWith(';')) {
    return <span className="syntax-comment">{line}</span>;
  }
  if (lang === 'toml' || lang === 'ini') {
    if (trimmed.startsWith('[') && trimmed.endsWith(']')) {
      return <span className="syntax-section font-bold text-[var(--accent-mauve)]">{line}</span>;
    }
    const eqIdx = line.indexOf('=');
    if (eqIdx !== -1) {
      const key = line.slice(0, eqIdx);
      const val = line.slice(eqIdx + 1);
      return (
        <>
          <span className="syntax-variable text-[var(--accent-blue)]">{key}</span>
          <span className="syntax-operator text-[var(--text-muted)]">=</span>
          <span className={val.includes('"') ? 'syntax-string text-[var(--accent-green)]' : 'syntax-number text-[var(--accent-peach)]'}>
            {val}
          </span>
        </>
      );
    }
  }
  return <span>{line}</span>;
};

interface CodePreviewProps { st: Pick<StudioState, 'activeView' | 'setActiveView' | 'containerName' | 'copied'>; toml: string; quadlet: string; containerfile: string | null; wasmReady: boolean; engine: 'rust' | 'ts'; }

export function CodePreview({ st, toml, quadlet, containerfile, wasmReady, engine }: CodePreviewProps) {
  const { activeView, setActiveView, containerName, copied } = st;
  const { handleDownloadToml, handleDownloadQuadlet, handleDownloadContainerfile, handleCopyConfig } = useOutputActions(
    st as Pick<StudioState, 'activeView' | 'containerName' | 'copied' | 'setCopied' | 'setShowExportMenu'>, toml, quadlet, containerfile);
  const containerfileView = containerfile ?? (wasmReady
    ? '# Custom base image: Containerfile generation needs the guest binary.\n# Run `podbox build` on Linux to bake it into the image.'
    : '# Loading Rust engine…');
  const code = activeView === 'toml' ? toml : activeView === 'containerfile' ? containerfileView : quadlet;
  const lines = code.split('\n');
  return (
<div className="lg:col-span-5 flex flex-col min-h-0 h-full overflow-hidden rounded-[3px] border border-[var(--border)] bg-[var(--bg-mantle)] shadow-sm">
  {/* Header tabs for Output */}
  <div className="shrink-0 flex items-center justify-between bg-[var(--bg-crust)] px-3 py-2 border-b border-[var(--border)]">
    <div className="flex items-center gap-1.5">
      <button
        type="button"
        onClick={() => setActiveView('toml')}
        className={`px-3 py-1.5 rounded-[2px] text-xs font-mono font-medium transition-colors cursor-pointer flex items-center gap-1.5 ${
          activeView === 'toml'
            ? 'bg-[var(--accent-mauve)] text-[var(--bg-crust)] font-bold shadow-xs'
            : 'text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-surface0)]'
        }`}
      >
        <FileCode2 className="w-3.5 h-3.5" />
        <span>podbox.toml</span>
      </button>

      <button
        type="button"
        onClick={() => setActiveView('quadlet')}
        className={`px-3 py-1.5 rounded-[2px] text-xs font-mono font-medium transition-colors cursor-pointer flex items-center gap-1.5 ${
          activeView === 'quadlet'
            ? 'bg-[var(--accent-peach)] text-[var(--bg-crust)] font-bold shadow-xs'
            : 'text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-surface0)]'
        }`}
      >
        <Terminal className="w-3.5 h-3.5" />
        <span>{containerName || 'podbox'}.container</span>
      </button>

      <button
        type="button"
        onClick={() => setActiveView('containerfile')}
        className={`px-3 py-1.5 rounded-[2px] text-xs font-mono font-medium transition-colors cursor-pointer flex items-center gap-1.5 ${
          activeView === 'containerfile'
            ? 'bg-[var(--accent-teal)] text-[var(--bg-crust)] font-bold shadow-xs'
            : 'text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-surface0)]'
        }`}
      >
        <Box className="w-3.5 h-3.5" />
        <span>Containerfile</span>
      </button>
    </div>

    <div className="flex items-center gap-1.5">
      <button
        onClick={activeView === 'toml' ? handleDownloadToml : activeView === 'containerfile' ? handleDownloadContainerfile : handleDownloadQuadlet}
        type="button"
        className="px-2 py-1 text-xs rounded-[2px] bg-[var(--bg-surface0)] hover:bg-[var(--bg-surface1)] text-[var(--text-primary)] transition-colors cursor-pointer flex items-center gap-1 font-mono"
        title={`Download ${activeView === 'toml' ? `${containerName || 'podbox'}.toml` : activeView === 'containerfile' ? 'Containerfile' : `${containerName || 'podbox'}.container`}`}
      >
        <Download className="w-3.5 h-3.5 text-[var(--accent-blue)]" />
        <span className="text-[11px] hidden sm:inline">Save</span>
      </button>

      <button
        onClick={handleCopyConfig}
        type="button"
        className="px-2.5 py-1 text-xs rounded-[2px] bg-[var(--bg-surface0)] hover:bg-[var(--bg-surface1)] text-[var(--text-primary)] transition-colors cursor-pointer flex items-center gap-1 font-mono"
        title="Copy to clipboard"
      >
        {copied ? (
          <>
            <Check className="w-3.5 h-3.5 text-[var(--accent-green)]" />
            <span className="text-[var(--accent-green)] text-[11px]">Copied</span>
          </>
        ) : (
          <>
            <Copy className="w-3.5 h-3.5 text-[var(--accent-mauve)]" />
            <span className="text-[11px]">Copy</span>
          </>
        )}
      </button>
    </div>
  </div>

  {/* Subheader info bar */}
  <div className="shrink-0 px-3 py-1.5 bg-[var(--bg-surface0)]/40 border-b border-[var(--border)] flex items-center justify-between text-[11px] font-mono text-[var(--text-muted)]">
    <span className="truncate">
      {activeView === 'toml'
        ? 'Declarative Podbox Spec'
        : activeView === 'containerfile'
          ? 'OCI image build recipe'
          : `systemd Unit (~/.config/containers/systemd/${containerName || 'podbox'}.container)`}
    </span>
    <span className="text-[10px] uppercase font-bold text-[var(--accent-mauve)] shrink-0 ml-2">
      {engine === 'rust' ? 'Rust Engine' : 'Live Synthesizer'}
    </span>
  </div>

  {/* Code Body - flex-1 min-h-0 overflow-auto */}
  <div className="flex-1 min-h-0 overflow-auto p-3.5 font-mono text-[12.5px] leading-relaxed select-text custom-scrollbar bg-[var(--bg-crust)]/50">
    <div className="table w-full">
      {lines.map((line, idx) => (
        <div key={idx} className="table-row hover:bg-[var(--bg-surface0)]/40 transition-colors">
          <span className="table-cell pr-4 pl-1 text-right select-none text-[var(--text-muted)]/40 text-[11px] w-9">
            {idx + 1}
          </span>
          <span className="table-cell whitespace-pre font-mono">
            {highlightCodeLine(line, activeView)}
          </span>
        </div>
      ))}
    </div>
  </div>

  {/* Bottom Bar: Line count & CLI Hint */}
  <div className="shrink-0 px-3 py-2 bg-[var(--bg-crust)] border-t border-[var(--border)] flex flex-col sm:flex-row sm:items-center justify-between gap-1.5 text-[11px] font-mono">
    <div className="flex items-center gap-2 text-[var(--text-muted)]">
      <span className="w-1.5 h-1.5 rounded-full bg-[var(--accent-green)]" />
      <span>{lines.length} lines</span>
      <span className="text-[var(--border)]">|</span>
      <span className="text-[var(--text-subtext)]">{activeView === 'toml' ? 'podbox.toml' : activeView === 'containerfile' ? 'Containerfile' : 'systemd unit'}</span>
    </div>
    <div className="text-[10px] text-[var(--accent-teal)] truncate">
      $ podbox build . && podbox enter {containerName || 'dev-box'}
    </div>
  </div>
</div>
  );
}