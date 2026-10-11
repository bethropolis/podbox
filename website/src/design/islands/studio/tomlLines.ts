// TOML source-map scanner and line diff for Studio.
//
// `tomlFieldLines` mirrors the Rust `field_lines_in_toml` wasm export so
// hover/click linkage works before wasm initializes (and without a wasm
// round-trip per render). `diffLines` powers the "what changed" view: a
// dependency-free LCS line diff, sized for config files, not megabytes.

/** Scan a TOML document into `dotted.path -> 1-based line`. */
export function tomlFieldLines(text: string): Record<string, number> {
  const map: Record<string, number> = {};
  let currentTable = '';
  const lines = text.split('\n');
  for (let i = 0; i < lines.length; i++) {
    const lineNo = i + 1;
    const trimmed = lines[i].trim();
    if (!trimmed || trimmed.startsWith('#')) continue;
    if (trimmed.startsWith('[[')) {
      const end = trimmed.indexOf(']]');
      if (end > 0) {
        const table = trimmed.slice(2, end).trim();
        currentTable = table;
        map[table] ??= lineNo;
      }
      continue;
    }
    if (trimmed.startsWith('[')) {
      const end = trimmed.indexOf(']');
      if (end > 0) {
        const table = trimmed.slice(1, end).trim().replace(/^["']|["']$/g, '');
        currentTable = table;
        map[table] ??= lineNo;
      }
      continue;
    }
    const eq = trimmed.indexOf('=');
    if (eq > 0) {
      const key = trimmed.slice(0, eq).trim().replace(/^["']|["']$/g, '');
      if (!key) continue;
      const dotted = currentTable ? `${currentTable}.${key}` : key;
      map[dotted] ??= lineNo;
    }
  }
  return map;
}

export type DiffRow =
  | { kind: 'same'; text: string }
  | { kind: 'del'; text: string }
  | { kind: 'add'; text: string };

/** LCS line diff (old -> new). O(n*m) table; configs are tens of lines. */
export function diffLines(oldText: string, newText: string): DiffRow[] {
  const a = oldText.split('\n');
  const b = newText.split('\n');
  const n = a.length;
  const m = b.length;
  const dp: Uint16Array[] = Array.from({ length: n + 1 }, () => new Uint16Array(m + 1));
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      dp[i][j] = a[i] === b[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
    }
  }
  const rows: DiffRow[] = [];
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (a[i] === b[j]) {
      rows.push({ kind: 'same', text: a[i] });
      i++;
      j++;
    } else if (dp[i + 1][j] >= dp[i][j + 1]) {
      rows.push({ kind: 'del', text: a[i] });
      i++;
    } else {
      rows.push({ kind: 'add', text: b[j] });
      j++;
    }
  }
  while (i < n) rows.push({ kind: 'del', text: a[i++] });
  while (j < m) rows.push({ kind: 'add', text: b[j++] });
  return rows;
}
