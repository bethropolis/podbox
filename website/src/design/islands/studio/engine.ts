// The podbox wasm engine behind one lazy initialisation.
//
// Everything the Studio computes with podbox logic — schema defaults, enum
// variants, TOML emission, validation, Quadlet codegen — happens here, once
// the module loads. Schema data is collected right after init so panels read
// it synchronously; there is no TS replica to fall back on, so callers gate
// rendering on `ensureEngine()` (StudioEntry) instead.

import init, {
  compile_quadlet,
  config_to_toml,
  field_lines_in_toml,
  locate_field_in_toml,
  parse_toml_to_json,
  schema_defaults,
  schema_enums,
  validate_toml,
} from '../../../wasm/podbox_wasm';

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
    x11_service: string | null;
  containerfile: string | null;
  warnings: string[];
}

/** Per-field schema data served by the config crate. */
export interface SchemaData {
  /** `dotted.path -> engine default`; absent means the engine has none. */
  defaults: Record<string, unknown>;
  /** `dotted.path -> enum variant order`, straight from the Rust derives. */
  enums: Record<string, string[]>;
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

// Init is module-global: every Studio mount reuses the same instance.
let initPromise: Promise<SchemaData> | null = null;
let engine: SchemaData | null = null;

/**
 * Idempotent engine start. Resolves to the schema data and leaves it
 * installed for the synchronous accessors below; a rejected load is
 * restartable (the next call tries again).
 */
export function ensureEngine(): Promise<SchemaData> {
  if (!initPromise) {
    initPromise = init()
      .then(collectSchemaData)
      .then((data) => {
        engine = data;
        return data;
      })
      .catch((e) => {
        initPromise = null;
        throw e;
      });
  }
  return initPromise;
}

/** True once `ensureEngine()` has resolved (before it does, nothing served). */
export function isEngineReady(): boolean {
  return engine != null;
}

function collectSchemaData(): SchemaData {
  // serde-wasm-bindgen hands back Maps for the Rust maps; plain records are
  // what the rest of the Studio reads.
  const toRecord = (m: unknown): any =>
    m instanceof Map ? Object.fromEntries(m) : m;
  return {
    defaults: toRecord(schema_defaults()),
    enums: toRecord(schema_enums()),
  };
}

/**
 * The schema data. Render paths are gated on `ensureEngine()`, so a miss
 * here is always an ordering bug, not a recoverable case.
 */
export function engineSchema(): SchemaData {
  if (!engine) throw new Error('podbox engine not initialized');
  return engine;
}

/** Enum variant order for a dotted path; emptiness is schema drift. */
export function enumVariants(path: string): string[] {
  const variants = engineSchema().enums[path];
  if (!Array.isArray(variants) || variants.length === 0) {
    throw new Error(`no enum variants served for ${path}`);
  }
  return variants;
}

/** Engine default for a dotted path; absence is schema drift, not null. */
export function engineDefault(path: string): unknown {
  if (!(path in engineSchema().defaults)) {
    throw new Error(`no engine default served for ${path}`);
  }
  return engineSchema().defaults[path];
}

/**
 * Emit a Config-shaped object as canonical TOML through the shared Rust
 * serializer. Returns null when the object does not fit the schema — a
 * Studio shape bug, surfaced in the log rather than by crashing the page.
 */
export function configToToml(config: Record<string, unknown>): string | null {
  if (!isEngineReady()) return null;
  try {
    return config_to_toml(config);
  } catch (e) {
    console.error('wasm config_to_toml failed', config, e);
    return null;
  }
}

export function validateToml(toml: string): ValidationReport | null {
  if (!isEngineReady()) return null;
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
}

export function compileQuadletToml(toml: string): CompileResult | null {
  if (!isEngineReady()) return null;
  try {
    return compile_quadlet(toml) as CompileResult;
  } catch {
    // Invalid TOML throws; the caller falls back to the TS template and
    // the validation banner carries the actual issues.
    return null;
  }
}

export function parseTomlText(toml: string): unknown {
  if (!isEngineReady()) return null;
  try {
    return parse_toml_to_json(toml);
  } catch {
    return null;
  }
}

export function locateField(toml: string, field: string): number | null {
  if (!isEngineReady()) return null;
  try {
    const line = locate_field_in_toml(toml, field) as number | null | undefined;
    return typeof line === 'number' ? line : null;
  } catch {
    return null;
  }
}

export function fieldLines(toml: string): Record<string, number> | null {
  if (!isEngineReady()) return null;
  try {
    const raw = field_lines_in_toml(toml) as unknown;
    // serde-wasm-bindgen serializes HashMap as a JS Map.
    if (raw instanceof Map) return Object.fromEntries(raw) as Record<string, number>;
    return raw as Record<string, number>;
  } catch {
    return null;
  }
}
