// Shareable Studio links: the config travels in the URL hash, so no server
// and no paste are involved.
//
// The payload is the generated TOML (the config, not the UI state), deflated
// with the platform `CompressionStream` and base64url-encoded. TOML is
// already terse; sharing UI keys would be larger and would drift as panels
// change. Browsers without CompressionStream fall back to plain base64url,
// which is bigger but always decodes.
//
// Hash shape: `#c=<payload>` (`c` for config). Restoring goes through the
// same parse the Import modal uses, so a shared link lands in Studio exactly
// like an imported file: migrations and defaults included.

const PREFIX = '#c=';
const CODEC = 'deflate-raw';

function toBase64Url(bytes: Uint8Array): string {
  let binary = '';
  for (const b of bytes) binary += String.fromCharCode(b);
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

function fromBase64Url(text: string): Uint8Array {
  const padded = text.replace(/-/g, '+').replace(/_/g, '/');
  const binary = atob(padded + '='.repeat((4 - (padded.length % 4)) % 4));
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

async function run(bytes: Uint8Array, transform: TransformStream): Promise<Uint8Array> {
  const writer = transform.writable.getWriter();
  const read = new Response(transform.readable).arrayBuffer();
  await writer.write(bytes);
  await writer.close();
  return new Uint8Array(await read);
}

/** Encode a TOML string into a URL-safe hash payload. */
export async function encodeShare(toml: string): Promise<string> {
  const bytes = new TextEncoder().encode(toml);
  if (typeof CompressionStream === 'undefined') return toBase64Url(bytes);
  const compressed = await run(bytes, new CompressionStream(CODEC));
  return toBase64Url(compressed);
}

/** Decode a hash payload back into TOML. `null` when unreadable. */
export async function decodeShare(payload: string): Promise<string | null> {
  try {
    const bytes = fromBase64Url(payload);
    if (typeof DecompressionStream === 'undefined') {
      return new TextDecoder().decode(bytes);
    }
    return new TextDecoder().decode(await run(bytes, new DecompressionStream(CODEC)));
  } catch {
    return null;
  }
}

/** Read `#c=<payload>` from a hash string, when present. */
export function shareFromHash(hash: string): string | null {
  const idx = hash.indexOf(PREFIX);
  if (idx === -1) return null;
  const payload = hash.slice(idx + PREFIX.length).split('&')[0];
  return payload.length > 0 ? payload : null;
}

/*
 * Whether a shared link is still loading.
 *
 * A visit with `#c=…` skips the localStorage restore, so for the moment
 * between mount and the shared config landing, state holds defaults. Without
 * this counter the debounced session save would write those defaults over the
 * user's saved session — the exact failure a shared link is meant to avoid.
 * Balanced by `markSharedLoadPending` in the Studio page.
 */
let pendingSharedLoads = 0;

export function markSharedLoadPending(pending: boolean): void {
  pendingSharedLoads += pending ? 1 : -1;
}

export function hasPendingSharedLoad(): boolean {
  return pendingSharedLoads > 0;
}
