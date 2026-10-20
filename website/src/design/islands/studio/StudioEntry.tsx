// Mount point for the Studio. Rendering is gated on the full wasm engine,
// because the Rust schema (defaults, enum variants, TOML emission) backs
// everything the page shows — a TS replica that drifts is exactly the class
// of bug this whole pipeline exists to prevent.
import React, { useEffect, useState } from 'react';
import { ensureEngine, isEngineReady } from './engine';
import { StudioPage } from '../StudioPage';

export function StudioEntry() {
  const [ready, setReady] = useState(isEngineReady());
  const [failed, setFailed] = useState('');

  useEffect(() => {
    let live = true;
    ensureEngine()
      .then(() => {
        if (live) setReady(true);
      })
      .catch((e) => {
        if (live) setFailed(e instanceof Error ? e.message : String(e));
      });
    return () => {
      live = false;
    };
  }, []);

  if (ready) return <StudioPage />;
  if (failed) {
    return (
      <div className="max-w-[1600px] mx-auto px-4 py-16 text-center font-mono text-sm">
        <p className="text-[var(--accent-red)]">podbox engine failed to load</p>
        <p className="mt-2 text-[var(--text-muted)] break-all">{failed}</p>
        <p className="mt-4 text-[var(--text-muted)]">Reload the page to try again.</p>
      </div>
    );
  }
  return (
    <div
      className="max-w-[1600px] mx-auto px-4 py-16 text-center font-mono text-sm text-[var(--text-muted)]"
      role="status"
      aria-live="polite"
    >
      loading podbox engine…
    </div>
  );
}
