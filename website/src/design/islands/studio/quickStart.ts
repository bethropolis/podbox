// One-line shell commands that take a Studio config to a running box.
//
// The point is the last mile: instead of "Download file → move it to
// ~/.config/podbox → run podbox", the user pastes one line. Two shapes are
// offered because the engine treats them differently:
//
// - prebuilt image: `podbox create` takes the reference directly, no file.
// - custom base:     the definition has to exist on disk, so the heredoc
//                    writes it where `podbox create <name>` looks.

import type { StudioValues } from './useStudioState';

/** `podbox create <image-ref> --name <name>` for prebuilt images. */
export function cliCommand(s: StudioValues): string {
  const name = s.containerName || 'podbox';
  const image = s.imagePrebuiltRef?.trim();
  if (!image) return `podbox create ${name}`;
  return `podbox create ${image} --name ${name}`;
}

/** Heredoc that writes the TOML where podbox reads it, then creates. */
export function quickStartCommand(s: StudioValues, toml: string): string {
  const name = s.containerName || 'podbox';
  const image = s.imagePrebuiltRef?.trim();
  if (image) return cliCommand(s);
  const body = toml.trimEnd();
  return [
    `mkdir -p ~/.config/podbox`,
    `cat <<'EOF' > ~/.config/podbox/${name}.toml`,
    body,
    `EOF`,
    `podbox create ${name}`,
  ].join('\n');
}
