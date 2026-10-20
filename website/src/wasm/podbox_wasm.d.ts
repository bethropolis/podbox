/* tslint:disable */
/* eslint-disable */

/**
 * Generate the systemd Quadlet units for a TOML definition string.
 *
 * Uses the deterministic mock host, so output matches what
 * `podbox enable` would emit on a fully-integrated desktop. Returns an
 * `Err` when the TOML is invalid — call `validate_toml` first for
 * structured issues.
 */
export function compile_quadlet(toml_str: string): any;

/**
 * Emit a `Config`-shaped JSON object as canonical TOML.
 *
 * Same serializer `podbox init` writes through, so the Studio's preview and
 * the engine's own files cannot disagree. Shaped like `serde` defaults:
 * absent optional keys and default values stay out of the output, and
 * invalid input (unknown fields, missing `image.base` / `container.name`)
 * raises with the schema error rather than re-typing TS-side validation.
 */
export function config_to_toml(config: any): string;

/**
 * Scan a TOML document into a `dotted.path -> 1-based line` source map for
 * field-to-line highlighting in Studio. One call per render; hover and
 * click-to-edit resolve locally from the result. Uses the same span-based
 * scanner as the error lookup, so quoted keys, inline tables and `[[array]]`
 * indices all resolve.
 */
export function field_lines_in_toml(toml_str: string): any;

/**
 * Locate a dotted schema path (`container.memory`) in a TOML document.
 *
 * Returns the 1-based line number, or `null` when the key has no source
 * line (derived values, defaults, absent keys). Thin wrapper over the same
 * lookup backing error spans; Studio prefers the batch `field_lines_map`
 * equivalent per render and calls this for ad-hoc jumps.
 */
export function locate_field_in_toml(toml_str: string, field_path: string): any;

/**
 * Parse a TOML definition into a plain JS object for Studio state import.
 */
export function parse_toml_to_json(toml_str: string): any;

/**
 * Per-field schema defaults for Studio prefill, keyed by dotted config path.
 *
 * Hand-built in the config crate from the same defaults serde and the
 * schema use; an absent path means "no engine default" (required keys,
 * engine-derived values), so the Studio never invents a placeholder where
 * Rust has none.
 */
export function schema_defaults(): any;

/**
 * Valid enum values per config path, straight from the strum derives.
 *
 * The Studio builds its select options from this list and attaches labels
 * in TypeScript; a variant added on the Rust side appears here and the
 * parity check forces it a label instead of it being silently missing.
 */
export function schema_enums(): any;

/**
 * Validate a TOML definition string with the exact CLI rules.
 */
export function validate_toml(toml_str: string): any;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly compile_quadlet: (a: number, b: number, c: number) => void;
    readonly config_to_toml: (a: number, b: number) => void;
    readonly field_lines_in_toml: (a: number, b: number) => number;
    readonly locate_field_in_toml: (a: number, b: number, c: number, d: number) => number;
    readonly parse_toml_to_json: (a: number, b: number, c: number) => void;
    readonly schema_defaults: () => number;
    readonly schema_enums: () => number;
    readonly validate_toml: (a: number, b: number) => number;
    readonly __wbindgen_export: (a: number, b: number) => number;
    readonly __wbindgen_export2: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_export3: (a: number) => void;
    readonly __wbindgen_add_to_stack_pointer: (a: number) => number;
    readonly __wbindgen_export4: (a: number, b: number, c: number) => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
