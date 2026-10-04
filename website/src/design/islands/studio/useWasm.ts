import { useState, useEffect } from 'react';
import init, { validate_toml, compile_quadlet, parse_toml_to_json } from '../../../wasm/podbox_wasm';

export interface ValidationIssue {
  field?: string;
  message: string;
}

export interface ValidationReport {
  valid: boolean;
  errors: ValidationIssue[];
  warnings: string[];
  /** field path -> first message, for input highlighting and tab badges. */
  errorMap: Record<string, string>;
}

export interface CompileResult {
  container: string;
  socket: string;
  build: string | null;
  containerfile: string | null;
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
    validate: (toml: string): ValidationReport | null => {
      if (!ready) return null;
      try {
        const raw = validate_toml(toml) as {
          valid: boolean;
          errors: ValidationIssue[];
          warnings: string[];
        };
        const errorMap: Record<string, string> = {};
        for (const issue of raw.errors) {
          if (issue.field && !(issue.field in errorMap)) {
            errorMap[issue.field] = issue.message;
          }
        }
        return { valid: raw.valid, errors: raw.errors, warnings: raw.warnings, errorMap };
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
