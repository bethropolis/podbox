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

/** `podbox create … --edit` for hand-finish: writes the config when missing,
 * opens it in the editor, then creates from what was saved. */
export function cliCommand(s: Naming): string {
  const name = s.containerName || 'podbox';
  const image = s.imagePrebuiltRef?.trim();
  if (!image) return `podbox create ${name} --edit`;
  return `podbox create ${image} --name ${name} --edit`;
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
  const name = s.containerName || 'podbox';
  const image = s.imagePrebuiltRef?.trim();
  if (image) return cliCommand(s);
  const body = toml.trimEnd();
  const target = `~/.config/podbox/profiles/${name}.toml`;
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
