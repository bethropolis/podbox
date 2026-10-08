import React, { useRef, useState } from 'react';
import { Upload, X } from 'lucide-react';
import { parse } from 'smol-toml';
import { tomlToPatch } from './parseImport';
import type { StudioState } from './useStudioState';

interface StudioImportProps {
  st: Pick<StudioState, 'applyPatch'>;
  onClose: () => void;
}

export function StudioImport({ st, onClose }: StudioImportProps) {
  const [text, setText] = useState('');
  const [error, setError] = useState('');
  const fileRef = useRef<HTMLInputElement>(null);

  const onFile = async (f: File | undefined) => {
    if (!f) return;
    setText(await f.text());
    setError('');
  };

  const onApply = () => {
    try {
      const doc = parse(text);
      st.applyPatch(tomlToPatch(doc as Record<string, any>));
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Could not parse TOML.');
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
      <div className="fixed inset-0 bg-black/60" onClick={onClose} />
      <div className="relative w-full max-w-xl rounded-[3px] bg-[var(--bg-mantle)] border border-[var(--border)] shadow-2xl">
        <div className="flex items-center justify-between px-4 py-3 border-b border-[var(--border)]">
          <div className="text-sm font-bold text-[var(--text-primary)]">Import podbox.toml</div>
          <button
            onClick={onClose}
            type="button"
            className="p-1 text-[var(--text-muted)] hover:text-[var(--text-primary)] cursor-pointer"
            aria-label="Close import"
          >
            <X className="w-4 h-4" />
          </button>
        </div>
        <div className="p-4 space-y-3">
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
            <p className="text-xs text-[var(--accent-red)] font-mono">{error}</p>
          )}
          <input
            ref={fileRef}
            type="file"
            accept=".toml,text/plain"
            className="hidden"
            onChange={(e) => onFile(e.target.files?.[0])}
          />
          <div className="flex flex-wrap items-center justify-between gap-2">
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
          </div>
        </div>
      </div>
    </div>
  );
}
