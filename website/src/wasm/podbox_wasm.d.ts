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
 * Scan a TOML document into a `dotted.path -> 1-based line` source map for
 * field-to-line highlighting in Studio. One call per render; hover and
 * click-to-edit resolve locally from the result.
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
 * Validate a TOML definition string with the exact CLI rules.
 */
export function validate_toml(toml_str: string): any;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly compile_quadlet: (a: number, b: number, c: number) => void;
    readonly field_lines_in_toml: (a: number, b: number) => number;
    readonly locate_field_in_toml: (a: number, b: number, c: number, d: number) => number;
    readonly parse_toml_to_json: (a: number, b: number, c: number) => void;
    readonly validate_toml: (a: number, b: number) => number;
    readonly __wbindgen_add_to_stack_pointer: (a: number) => number;
    readonly __wbindgen_export: (a: number, b: number) => number;
    readonly __wbindgen_export2: (a: number, b: number, c: number, d: number) => number;
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
