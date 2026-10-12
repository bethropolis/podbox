import React, { useRef, useState } from 'react';
import { Upload } from 'lucide-react';
import { parse } from 'smol-toml';
import { Dialog } from '../../components/Dialog';
import { tomlToPatch } from './parseImport';
import { useWasm } from './useWasm';
import type { StudioState } from './useStudioState';

interface StudioImportProps {
  st: Pick<StudioState, 'applyPatch'>;
  onClose: () => void;
}

export function StudioImport({ st, onClose }: StudioImportProps) {
  const [text, setText] = useState('');
  const [error, setError] = useState('');
  const fileRef = useRef<HTMLInputElement>(null);
  const { ready: wasmReady, parseToml, validate } = useWasm();

  const onFile = async (f: File | undefined) => {
    if (!f) return;
    setText(await f.text());
    setError('');
  };

  const onApply = () => {
    // Native path first: the Rust parser applies migrations and defaults, so
    // older schemas land in current state with one semantics everywhere.
    // (The canonical shape drops the legacy Studio-only `image.preset` key;
    // its base ref survives, so imports land in custom-image mode.)
    if (wasmReady) {
      const parsed = parseToml(text);
      if (parsed && typeof parsed === 'object') {
        st.applyPatch(tomlToPatch(parsed as Record<string, any>));
        onClose();
        return;
      }
      const report = validate(text);
      if (report && !report.valid) {
        setError(report.errors.map((e) => e.message).join('\n') || 'Invalid TOML.');
        return;
      }
    }
    try {
      const doc = parse(text);
      st.applyPatch(tomlToPatch(doc as Record<string, any>));
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Could not parse TOML.');
    }
  };

  // Mounted only while open (see StudioHeader), so `open` is constant here:
  // Esc, backdrop click, and ✕ all funnel into `onClose`, which unmounts.
  return (
    <Dialog
      open
      onClose={onClose}
      title="Import podbox.toml"
      size="md"
      actions={
        <>
          <button
            onClick={() => fileRef.current?.click()}
            type="button"
            className="px-3 py-1.5 rounded-[2px] bg-[var(--bg-crust)] border border-[var(--border)] text-xs text-[var(--text-subtext)] hover:text-[var(--text-primary)] transition-colors cursor-pointer flex items-center gap-1.5"
          >
            <Upload className="w-3.5 h-3.5" />
            <span>Choose .toml file</span>
          </button>
          <button
            onClick={onApply}
            type="button"
            disabled={!text.trim()}
            className="px-4 py-1.5 rounded-[2px] bg-[var(--accent-mauve)] text-[var(--bg-crust)] font-semibold text-xs transition-colors cursor-pointer disabled:opacity-40 hover:opacity-95"
          >
            Import into Studio
          </button>
        </>
      }
    >
      <div className="space-y-3">
        <textarea
          value={text}
          onChange={(e) => {
            setText(e.target.value);
            setError('');
          }}
          spellCheck={false}
          placeholder={'[image]\nbase = "fedora:44"\n\n[container]\nname = "dev"'}
          rows={12}
          className="w-full rounded-[2px] bg-[var(--bg-crust)] border border-[var(--border)] p-3 font-mono text-xs text-[var(--text-primary)] placeholder:text-[var(--text-muted)] focus:outline-none focus:border-[var(--accent-mauve)] resize-y"
        />
        {error && (
          <p className="text-xs text-[var(--accent-red)] font-mono whitespace-pre-line">{error}</p>
        )}
        <input
          ref={fileRef}
          type="file"
          accept=".toml,text/plain"
          className="hidden"
          onChange={(e) => onFile(e.target.files?.[0])}
        />
      </div>
    </Dialog>
  );
}
