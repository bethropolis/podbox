import { useState, useEffect } from 'react';
import init, { validate_toml, compile_quadlet, parse_toml_to_json, locate_field_in_toml, field_lines_in_toml } from '../../../wasm/podbox_wasm';

export interface ValidationIssue {
  field?: string;
  message: string;
  /** 1-based source line in the submitted TOML, when locatable. */
  line?: number;
  /** 1-based source column, when locatable (syntax errors). */
  column?: number;
  /** Byte offsets (start, end) in the submitted TOML, when locatable. */
  span?: [number, number];
}

export interface ValidationReport {
  valid: boolean;
  errors: ValidationIssue[];
  advisories: ValidationIssue[];
  warnings: string[];
  /** field path -> first message, for input highlighting and tab badges. */
  errorMap: Record<string, string>;
  /** Advisories deliberately stay out of `errorMap`; they badge their tab only. */
  advisoryMap: Record<string, string>;
}

/**
 * Validation issues the Studio reports as guidance rather than as errors.
 *
 * podbox genuinely rejects these configs — the rule is a guardrail, not a
 * wording problem — but the owning panel already renders an actionable hint
 * for them (what is missing plus one-click fixes), so repeating the raw
 * engine string in a red banner only adds noise.
 */
const ADVISORY_FIELDS = new Set(['integration.host_exec']);

export interface CompileResult {
  container: string;
  socket: string;
  build: string | null;
  /** Companion host socket server unit (`{name}-host.service`). */
  host_service: string;
  /** D-Bus proxy unit; null when the proxy is disabled in config. */
  dbus_proxy_service: string | null;
  /** Wayland firewall unit; null when the proxy is disabled in config. */
  compositor_service: string | null;
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
        const advisoryMap: Record<string, string> = {};
        const errors: ValidationIssue[] = [];
        const advisories: ValidationIssue[] = [];
        for (const issue of raw.errors) {
          const advisory = !!issue.field && ADVISORY_FIELDS.has(issue.field);
          if (advisory) {
            advisories.push(issue);
            if (issue.field && !(issue.field in advisoryMap)) {
              advisoryMap[issue.field] = issue.message;
            }
          } else {
            errors.push(issue);
            if (issue.field && !(issue.field in errorMap)) {
              errorMap[issue.field] = issue.message;
            }
          }
        }
        return { valid: raw.valid, errors, advisories, warnings: raw.warnings, errorMap, advisoryMap };
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
    locateField: (toml: string, field: string): number | null => {
      if (!ready) return null;
      try {
        const line = locate_field_in_toml(toml, field) as number | null | undefined;
        return typeof line === 'number' ? line : null;
      } catch {
        return null;
      }
    },
    fieldLines: (toml: string): Record<string, number> | null => {
      if (!ready) return null;
      try {
        const raw = field_lines_in_toml(toml) as unknown;
        // serde-wasm-bindgen serializes HashMap as a JS Map.
        if (raw instanceof Map) return Object.fromEntries(raw) as Record<string, number>;
        return raw as Record<string, number>;
      } catch {
        return null;
      }
    },
  };
}
