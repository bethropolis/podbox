import React, { useEffect, useMemo, useRef, useState } from 'react';
import {
  Box,
  Check,
  Copy,
  Eye,
  FileCode2,
  Pencil,
  Terminal,
} from 'lucide-react';
import type { StudioState } from './useStudioState';
import { useOutputActions } from './useOutputActions';
import { highlightDocument, type PreviewLang } from '../../highlight';
import { diffLines } from './tomlLines';
import { useHorizontalWheel } from './useHorizontalWheel';

type UnitView = 'toml' | 'quadlet' | 'containerfile' | 'host' | 'proxy' | 'compositor';

interface CodePreviewProps {
  st: Pick<StudioState, 'activeView' | 'setActiveView' | 'containerName' | 'copied' | 'isFullscreen'>;
  toml: string;
  quadlet: string;
  containerfile: string | null;
  hostService: string | null;
  proxyService: string | null;
  compositorService: string | null;
  engine: 'rust' | 'ts';
  /** 1-based TOML line -> message, for error squiggles. */
  errorLines?: Record<number, string>;
  /** 1-based TOML line -> dotted field, for click-to-edit. */
  lineFields?: Record<number, string>;
  /** 1-based TOML line highlighted from form hover/focus. */
  hoverLine?: number | null;
  /** 1-based TOML line to scroll to (banner click). Consumed on arrival. */
  focusLine?: number | null;
  onFocusLineConsumed?: () => void;
  onLineClick?: (field: string) => void;
  /** Baseline TOML for the "what changed" diff (preset or blank defaults). */
  baselineToml?: string | null;
  baselineLabel?: string;
  /** Engine check of the edit-mode draft. Returns the first error or null. */
  onDraftValidate?: (draft: string) => string | null;
  /** Parse the draft into Studio state. `true` when it was applied. */
  onApplyToml?: (draft: string) => boolean;
  /** Below lg the page shows either the form or the preview; true hides this one. */
  mobileHidden?: boolean;
}

export function CodePreview({ st, toml, quadlet, containerfile, hostService, proxyService, compositorService, engine, errorLines, lineFields, hoverLine, focusLine, onFocusLineConsumed, onLineClick, baselineToml, baselineLabel, onDraftValidate, onApplyToml, mobileHidden = false }: CodePreviewProps) {
  const { activeView, setActiveView, containerName, copied, isFullscreen } = st;
  // The engine always renders a Containerfile: prebuilt images get the short
  // overlay, custom bases the full recipe whose guest layer carries an inline
  // comment. That comment is the only signal — no tab badge, since it would
  // flag every config rather than anything actionable.
  const showContainerfile = containerfile != null && containerfile.length > 0;
  const units: { view: UnitView; label: string; title: string; code: string | null }[] = [
    { view: 'toml', label: `${containerName || 'podbox'}.toml`, title: `${containerName || 'podbox'}.toml`, code: toml },
    { view: 'quadlet', label: `${containerName || 'podbox'}.container`, title: `${containerName || 'podbox'}.container`, code: quadlet },
    { view: 'host', label: `${containerName || 'podbox'}-host.service`, title: 'host socket server', code: hostService },
    { view: 'proxy', label: 'proxy.service', title: 'D-Bus proxy', code: proxyService },
    { view: 'compositor', label: 'compositor.service', title: 'Wayland firewall', code: compositorService },
    { view: 'containerfile', label: 'Containerfile', title: 'Containerfile', code: showContainerfile ? (containerfile ?? '') : null },
  ];
  const visible = units.filter((u) => u.code != null);
  const view: UnitView = visible.some((u) => u.view === (activeView as UnitView)) ? (activeView as UnitView) : 'toml';
  const active = visible.find((u) => u.view === view) ?? visible[0];
  const code = active.code ?? '';
  const lang: PreviewLang = view === 'toml' ? 'toml' : view === 'containerfile' ? 'containerfile' : 'quadlet';
  const highlighted = highlightDocument(code, lang);
  const lines = code.split('\n');
  // Copy follows the visible tab: companion units copy their own text, not
  // the `.container` unit (useOutputActions copies `quadlet` for unit views).
  const copySource = view === 'toml' || view === 'containerfile' || view === 'quadlet' ? quadlet : (code || quadlet);
  const { handleCopyConfig } = useOutputActions(
    st as Pick<StudioState, 'activeView' | 'containerName' | 'copied' | 'setCopied' | 'setShowExportMenu'>, toml, copySource, containerfile);
  // Fullscreen is a workbench: tighter chrome so the code itself gets the pixels.
  const pad = isFullscreen ? 'px-2 py-1' : 'px-3 py-2';
  const scrollRef = useRef<HTMLDivElement>(null);
  const stripRef = useRef<HTMLDivElement>(null);
  useHorizontalWheel(stripRef);
  const [showDiff, setShowDiff] = useState(false);

  // Edit mode: the TOML tab becomes a real editor. The draft is the source
  // of truth while typing; the form only follows once the engine parses it,
  // so a half-typed line never overwrites a working configuration. The last
  // good state simply stays put until the draft parses again.
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState('');
  const [draftError, setDraftError] = useState<string | null>(null);

  useEffect(() => {
    if (!editing || draft === code) return;
    const timer = window.setTimeout(() => {
      if (onDraftValidate) {
        const error = onDraftValidate(draft);
        if (error) {
          setDraftError(error);
          return;
        }
      }
      const applied = onApplyToml?.(draft) ?? false;
      setDraftError(applied ? null : 'Could not apply this configuration.');
    }, 400);
    return () => window.clearTimeout(timer);
  }, [editing, draft, code, onApplyToml, onDraftValidate]);

  useEffect(() => {
    if (focusLine == null || view !== 'toml') return;
    const el = scrollRef.current?.querySelector(`[data-line="${focusLine}"]`);
    el?.scrollIntoView({ block: 'center', behavior: 'smooth' });
    onFocusLineConsumed?.();
  }, [focusLine, view, onFocusLineConsumed]);

  // "What changed" vs the preset/default baseline. TOML only: the units are
  // derived output, so diffing them would double-count every config edit.
  // Always computed (LCS on tens of lines is trivial) so the footer count is
  // live; only rendered when the toggle is on.
  const diffRows = useMemo(
    () => (baselineToml != null ? diffLines(baselineToml, toml) : null),
    [baselineToml, toml],
  );
  const diffCount = useMemo(
    () => diffRows?.filter((r) => r.kind !== 'same').length ?? 0,
    [diffRows],
  );

  return (
<div className={`lg:col-span-5 ${mobileHidden ? 'hidden lg:flex' : 'flex'} flex-col min-h-[60vh] lg:min-h-0 h-full overflow-hidden rounded-md border border-[var(--border)] bg-[var(--bg-mantle)]`}>
  {/* Header tabs for Output */}
  <div className={`shrink-0 flex items-center justify-between gap-2 bg-[var(--bg-crust)] border-b border-[var(--border)] ${pad}`}>
    <div ref={stripRef} className="flex items-center gap-1 min-w-0 overflow-x-auto scrollbar-none [mask-image:linear-gradient(to_right,black_calc(100%-28px),transparent)]">
      {visible.map((u, i) => (
        <button
          key={u.view}
          type="button"
          onClick={() => {
            setShowDiff(false);
            setActiveView(u.view);
          }}
          title={u.title}
          className={`px-2 py-1 rounded text-xs font-mono transition-colors cursor-pointer flex items-center gap-1.5 min-w-0 max-w-[160px] shrink-0 ${
            // The TOML tab pins to the left while unit tabs scroll under it.
            // Opaque cover only when inactive: the active accent fill is
            // already opaque, and a competing bg class kills it.
            i === 0 ? 'sticky left-0 z-10' : ''
          } ${
            view === u.view
              ? 'bg-[var(--bg-surface0)] text-[var(--text-primary)]'
              : 'bg-[var(--bg-crust)] text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-surface0)]'
          }`}
        >
          {u.view === 'toml' ? <FileCode2 className="w-3 h-3 shrink-0" /> : u.view === 'containerfile' ? <Box className="w-3 h-3 shrink-0" /> : <Terminal className="w-3 h-3 shrink-0" />}
          <span className="truncate">{u.label}</span>
        </button>
      ))}
    </div>

    <div className="flex items-center gap-1 shrink-0">
      {view === 'toml' && (
        <button
          onClick={() => {
            if (editing) {
              setEditing(false);
              setDraftError(null);
            } else {
              setDraft(code);
              setDraftError(null);
              setShowDiff(false);
              setEditing(true);
            }
          }}
          type="button"
          title={editing ? 'Back to preview (Esc)' : 'Edit this TOML directly'}
          className={`p-1.5 rounded-[2px] transition-colors cursor-pointer ${
            editing
              ? 'text-[var(--accent-mauve)] bg-[var(--accent-mauve)]/15'
              : 'text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-surface0)]'
          }`}
        >
          {editing ? <Eye className="w-3.5 h-3.5" /> : <Pencil className="w-3.5 h-3.5" />}
        </button>
      )}
      <button
        onClick={handleCopyConfig}
        type="button"
        className="p-1.5 rounded-[2px] text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-surface0)] transition-colors cursor-pointer"
        title="Copy to clipboard"
      >
        {copied ? (
          <Check className="w-3.5 h-3.5 text-[var(--accent-green)]" />
        ) : (
          <Copy className="w-3.5 h-3.5" />
        )}
      </button>
    </div>
  </div>

  {/* Code Body - flex-1 min-h-0 overflow-auto */}
  <div ref={scrollRef} className={`flex-1 min-h-0 overflow-auto overscroll-contain font-mono select-text scrollbar-none bg-[var(--bg-crust)]/50 ${isFullscreen ? 'p-2 text-[12px] leading-snug' : 'p-3.5 text-[12.5px] leading-relaxed'}`}>
    {diffRows && showDiff ? (
    <div className="table w-full">
      <div className="table-row">
        <span className="table-cell text-[10px] text-[var(--text-muted)]/60 pb-1">vs {baselineLabel ?? 'defaults'} — {diffCount} changed lines</span>
      </div>
      {diffRows.map((row, idx) => (
        <div
          key={idx}
          className={`table-row ${row.kind === 'del' ? 'bg-[var(--accent-red)]/10 text-[var(--accent-red)]' : row.kind === 'add' ? 'bg-[var(--accent-green)]/10 text-[var(--accent-green)]' : 'text-[var(--text-muted)]/50'}`}
        >
          <span className="table-cell text-right select-none text-[11px] pr-2 pl-0.5 w-7">
            {row.kind === 'del' ? '−' : row.kind === 'add' ? '+' : ' '}
          </span>
          <span className="table-cell whitespace-pre font-mono">{row.text || ' '}</span>
        </div>
      ))}
    </div>
    ) : editing ? (
    <div className="flex flex-col h-full min-h-0">
      <textarea
        value={draft}
        autoFocus
        spellCheck={false}
        onChange={(e) => setDraft(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Escape') {
            setEditing(false);
            setDraftError(null);
          } else if (e.key === 'Tab') {
            // TOML has no indent nesting, but Tab should still not leave the
            // editor: two spaces, like the rest of the generated files.
            e.preventDefault();
            const el = e.currentTarget;
            const start = el.selectionStart;
            const end = el.selectionEnd;
            const next = `${draft.slice(0, start)}  ${draft.slice(end)}`;
            setDraft(next);
            window.requestAnimationFrame(() => el.setSelectionRange(start + 2, start + 2));
          }
        }}
        className="flex-1 min-h-[8rem] w-full bg-transparent outline-none resize-none font-mono text-[var(--text-primary)] placeholder:text-[var(--text-muted)]"
        aria-label="Edit podbox.toml"
        placeholder="[container]
name = &quot;dev&quot;"
      />
      {draftError ? (
        <p className="shrink-0 mt-1 text-[11px] font-mono text-[var(--accent-red)]">
          {draftError} — the form keeps its last working values.
        </p>
      ) : (
        <p className="shrink-0 mt-1 text-[11px] font-mono text-[var(--text-muted)]/70">
          Esc to leave the editor. Changes apply once they parse.
        </p>
      )}
    </div>
    ) : (
    <div className="table w-full">
      {highlighted.map((nodes, idx) => {
        const lineNo = idx + 1;
        const errMsg = view === 'toml' ? errorLines?.[lineNo] : undefined;
        const field = view === 'toml' ? lineFields?.[lineNo] : undefined;
        const hot = (view === 'toml' && hoverLine === lineNo) || (view === 'toml' && focusLine === lineNo);
        return (
        <div
          key={idx}
          data-line={lineNo}
          onClick={field && onLineClick ? () => onLineClick(field) : undefined}
          title={errMsg ?? (field ? `Edit ${field}` : undefined)}
          className={`table-row transition-colors ${errMsg ? 'bg-[var(--accent-red)]/10' : ''} ${hot ? 'bg-[var(--accent-mauve)]/15' : 'hover:bg-[var(--bg-surface0)]/40'} ${field && onLineClick ? 'cursor-pointer' : ''}`}
        >
          <span className={`table-cell text-right select-none text-[var(--text-muted)]/40 text-[11px] ${isFullscreen ? 'pr-2 pl-0.5 w-7' : 'pr-4 pl-1 w-9'}`}>
            {idx + 1}
          </span>
          <span className={`table-cell whitespace-pre font-mono ${errMsg ? 'underline decoration-[var(--accent-red)] decoration-wavy underline-offset-4' : ''}`}>
            {nodes}
          </span>
        </div>
        );
      })}
    </div>
    )}
  </div>

  {/* Bottom Bar: line count + engine. The filename and kind are already in
      the tab directly above, so repeating them here was pure noise. */}
  <div className="shrink-0 px-3 py-1.5 bg-[var(--bg-crust)] border-t border-[var(--border)] flex items-center gap-2 text-[11px] font-mono text-[var(--text-muted)]">
    <span>{lines.length} lines</span>
    <span className="text-[var(--border)]">|</span>
    <span className="text-[10px] uppercase font-bold text-[var(--accent-mauve)]">
      {engine === 'rust' ? 'Rust Engine' : 'Live Synthesizer'}
    </span>
    {view === 'toml' && baselineToml != null ? (
      showDiff ? (
        <button
          onClick={() => setShowDiff(false)}
          type="button"
          className="ml-auto transition-colors cursor-pointer hover:text-[var(--text-primary)]"
        >
          show code
        </button>
      ) : diffCount > 0 ? (
        <button
          onClick={() => setShowDiff(true)}
          type="button"
          title={`${diffCount} changed lines vs ${baselineLabel ?? 'defaults'}`}
          className="ml-auto transition-colors cursor-pointer text-[var(--accent-green)] opacity-70 hover:opacity-100 hover:text-[var(--text-primary)]"
        >
          ±{diffCount} changes
        </button>
      ) : (
        <span title={`No changes vs ${baselineLabel ?? 'defaults'}`} className="ml-auto">
          no changes
        </span>
      )
    ) : (
      <span title="Derived from the TOML — edit the form or TOML, not this file" className="ml-auto">
        generated
      </span>
    )}
  </div>
</div>
  );
}
