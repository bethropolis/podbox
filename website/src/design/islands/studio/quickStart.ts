// Copy-paste blocks that take a Studio config to a running box.
//
// The point is the last mile: instead of "Download file → move it to
// ~/.config/podbox → run podbox", the user pastes one block. Two shapes are
// offered because the engine treats them differently:
//
// - prebuilt image: `podbox create` takes the reference directly, no file.
// - custom base:     the definition has to exist on disk, so the block
//                    writes it where `podbox create <name>` looks.
//
// The custom-base block comes in two native flavors — one per shell family —
// because fish has no heredocs:
//
// - sh (bash/zsh/sh): a quoted heredoc, fully literal, no escaping needed.
// - fish:             one `printf '%s'` with the TOML as a single multiline
//                     single-quoted string; `\` doubles to `\\` and `'` to
//                     `\'` (the only two characters special inside fish
//                     single quotes). No line-per-arg noise, no delimiter to
//                     collide with the body.

import type { StudioValues } from './useStudioState';

type Naming = Pick<StudioValues, 'containerName' | 'imagePrebuiltRef'>;

/**
 * Quote one shell word. Values made only of safe characters stay bare, so
 * the common `podbox create dev --edit` reads exactly as before; anything
 * else gets single-quoted. sh and fish need separate rules for an embedded
 * quote: sh closes and reopens around it (`'\''`), fish backslash-escapes
 * it (`\'`) — one dialect's escape is the other's literal.
 */
export function shellQuote(value: string, shell: 'sh' | 'fish'): string {
  if (value !== '' && /^[A-Za-z0-9_@%+=:,./-]+$/.test(value)) return value;
  if (shell === 'fish') {
    return `'${value.replaceAll('\\', '\\\\').replaceAll("'", "\\'")}'`;
  }
  return `'${value.replaceAll("'", "'\\''")}'`;
}

/** Container names double as a filename segment and an argv word. The UI
 * sanitizes them on entry, but a share link can carry anything, so clean
 * the path segment here: the `~` prefix must stay a literal tilde (quoting
 * it would stop expansion), which is why this is a sanitizer, not a quote.
 */
function fileName(name: string): string {
  return name.toLowerCase().replace(/[^a-z0-9_-]/g, '') || 'podbox';
}

/** `podbox create … --edit` for hand-finish: writes the config when missing,
 * opens it in the editor, then creates from what was saved. Every
 * interpolated value is quoted: the image ref rides in on share links and
 * must never break out of its word. */
export function cliCommand(s: Naming, shell: 'sh' | 'fish' = 'sh'): string {
  const name = shellQuote(s.containerName || 'podbox', shell);
  const image = s.imagePrebuiltRef?.trim();
  if (!image) return `podbox create ${name} --edit`;
  return `podbox create ${shellQuote(image, shell)} --name ${name} --edit`;
}

/** Heredoc delimiter that cannot collide with a line of the TOML body. */
function heredocDelimiter(body: string): string {
  const lines = new Set(body.split('\n'));
  let d = 'PODBOX_EOF';
  while (lines.has(d)) d += '_';
  return d;
}

/** Whole body as one fish single-quoted string (may span lines). */
function fishQuote(body: string): string {
  return `'${body.replaceAll('\\', '\\\\').replaceAll(`'`, `\\'`)}'`;
}

/** Block that writes the TOML to profiles/, then creates. */
export function quickStartCommand(s: Naming, toml: string, shell: 'sh' | 'fish' = 'sh'): string {
  const rawName = s.containerName || 'podbox';
  const name = shellQuote(rawName, shell);
  const image = s.imagePrebuiltRef?.trim();
  if (image) return cliCommand(s, shell);
  const body = toml.trimEnd();
  const target = `~/.config/podbox/profiles/${fileName(rawName)}.toml`;
  if (shell === 'fish') {
    // Trailing newline stays inside the quotes so the file ends with one.
    return [
      `mkdir -p ~/.config/podbox/profiles`,
      `printf '%s' ${fishQuote(`${body}\n`)} > ${target}`,
      `podbox create ${name}`,
    ].join('\n');
  }
  const eof = heredocDelimiter(body);
  return [
    `mkdir -p ~/.config/podbox/profiles`,
    `cat > ${target} <<'${eof}'`,
    body,
    eof,
    `podbox create ${name}`,
  ].join('\n');
}
