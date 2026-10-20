// React hook over the shared engine: everything dynamic (validation,
// Quadlet codegen, TOML parsing) plus the ready flag the page gates on.
//
// The engine functions live in engine.ts as plain wrappers that no-op until
// the module initializes; the hook only adds the ready flag, so identities
// stay stable and no stale closure can wrap a half-loaded module.

import { useEffect, useState } from 'react';
import {
  compileQuadletToml,
  ensureEngine,
  isEngineReady,
  parseTomlText,
  locateField,
  fieldLines,
  validateToml,
} from './engine';
import type { CompileResult, ValidationReport } from './engine';

export type { ValidationIssue, ValidationReport, CompileResult } from './engine';

/**
 * Called once per mount, but the shared `ensureEngine()` makes the effect a
 * no-op after the first: initialization is module-global.
 */
export function useWasm() {
  const [ready, setReady] = useState(isEngineReady());

  useEffect(() => {
    let live = true;
    ensureEngine()
      .then(() => {
        if (live) setReady(true);
      })
      .catch((e) => console.error('podbox wasm engine failed to load', e));
    return () => {
      live = false;
    };
  }, []);

  return {
    ready,
    validate: (toml: string): ValidationReport | null => validateToml(toml),
    compileQuadlet: (toml: string): CompileResult | null => compileQuadletToml(toml),
    parseToml: (toml: string): unknown => parseTomlText(toml),
    locateField: (toml: string, field: string): number | null => locateField(toml, field),
    fieldLines: (toml: string): Record<string, number> | null => fieldLines(toml),
  };
}
