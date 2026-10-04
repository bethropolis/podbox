import { useState, useEffect } from 'react';
import init, { validate_toml, compile_quadlet, parse_toml_to_json } from '../../../wasm/podbox_wasm';

export interface ValidationResult {
  valid: boolean;
  errors: string[];
  warnings: string[];
}

export interface CompileResult {
  container: string;
  socket: string;
  build: string | null;
  warnings: string[];
}

// Init is module-global: every StudioPage mount reuses the same instance.
let initPromise: Promise<unknown> | null = null;
function ensureInit(): Promise<unknown> {
  if (!initPromise) {
    initPromise = init().catch((e) => {
      initPromise = null;
      throw e;
    });
  }
  return initPromise;
}

export function useWasm() {
  const [ready, setReady] = useState(false);

  useEffect(() => {
    let live = true;
    ensureInit()
      .then(() => {
        if (live) setReady(true);
      })
      .catch((e) => console.error('podbox wasm engine failed to load, using TS fallback', e));
    return () => {
      live = false;
    };
  }, []);

  return {
    ready,
    validate: (toml: string): ValidationResult | null => {
      if (!ready) return null;
      try {
        return validate_toml(toml) as ValidationResult;
      } catch (e) {
        console.error('wasm validate failed', e);
        return null;
      }
    },
    compileQuadlet: (toml: string): CompileResult | null => {
      if (!ready) return null;
      try {
        return compile_quadlet(toml) as CompileResult;
      } catch {
        // Invalid TOML throws; the caller falls back to the TS template and
        // the validation banner carries the actual issues.
        return null;
      }
    },
    parseToml: (toml: string): unknown => {
      if (!ready) return null;
      try {
        return parse_toml_to_json(toml);
      } catch {
        return null;
      }
    },
  };
}
