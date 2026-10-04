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
