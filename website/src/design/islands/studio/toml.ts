// TOML string and key quoting for the Studio's TypeScript emitters.
//
// Every value the user can type (env values, run commands, install hooks,
// service commands, package names, mount paths) flows through here. Only
// `[[provision]]` escaped before, so a `"` anywhere else produced TOML the
// engine rejects, and a `\` could silently become an escape (`~/a\b` reads
// as `~/a<backspace>`). The long-term fix emits TOML in Rust; until then
// this module is the single quoting point for `generateToml.ts`,
// `schema.ts` (`formatScalar`) and the landing-page playground.

/**
 * Escape content for a TOML basic string. Covers `"`, `\` and every
 * control character (newline and tab get their short forms, the rest
 * `\uXXXX`); anything else — including Unicode — passes through, and a
 * literal tab stays literal (TOML allows it unescaped in basic strings).
 */
export function escapeTomlBasic(s: string): string {
  return s.replace(/["\\\b\f\n\r\t\x00-\x1f\x7f]/g, (ch) => {
    switch (ch) {
      case '"':
        return '\\"';
      case '\\':
        return '\\\\';
      case '\b':
        return '\\b';
      case '\f':
        return '\\f';
      case '\n':
        return '\\n';
      case '\r':
        return '\\r';
      case '\t':
        return '\\t';
      default:
        return `\\u${ch.charCodeAt(0).toString(16).padStart(4, '0')}`;
    }
  });
}

/** Quote a value as a single-line TOML basic string. */
export function tomlString(s: string): string {
  return `"${escapeTomlBasic(s)}"`;
}

/**
 * Quote a TOML key. Bare words stay bare (`memory`, `my-key`); anything
 * else — `my.var`, `my key`, `FOO"BAR` — is quoted, so an env key with a
 * dot stays one key instead of becoming a nested table.
 */
export function tomlKey(k: string): string {
  if (/^[A-Za-z0-9_-]+$/.test(k)) return k;
  return tomlString(k);
}
