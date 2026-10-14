import React from 'react';

// Tiny line-based syntax highlighter shared by the Studio preview and every
// code block on the site (docs, landing).
//
// A hand-rolled tokenizer instead of a library: the grammars are small (TOML,
// systemd ini, Containerfile, shell, JSON), it re-renders on every Studio
// keystroke, and it costs nothing to load. Colors come straight from the
// `.syntax-*` theme classes in global.css.

export type PreviewLang = 'toml' | 'quadlet' | 'containerfile' | 'shell' | 'json' | 'text';

/** Map a markdown fence label to a grammar. Unknown labels render plain. */
export function langFromFence(fence: string | undefined): PreviewLang {
  switch ((fence || '').toLowerCase()) {
    case 'toml':
      return 'toml';
    case 'ini':
    case 'systemd':
    case 'quadlet':
    case 'container':
      return 'quadlet';
    case 'dockerfile':
    case 'containerfile':
      return 'containerfile';
    case 'bash':
    case 'sh':
    case 'shell':
    case 'zsh':
    case 'fish':
    case 'console':
      return 'shell';
    case 'json':
    case 'jsonc':
    case 'ndjson':
      return 'json';
    default:
      return 'text';
  }
}

let key = 0;
const span = (cls: string, text: string) => (
  <span key={key++} className={cls}>
    {text}
  </span>
);
const plain = (text: string) => <React.Fragment key={key++}>{text}</React.Fragment>;

/** Split a value into strings, dates, booleans, numbers, flags, comments. */
function valueParts(value: string): React.ReactNode[] {
  const out: React.ReactNode[] = [];
  // Order is load-bearing: triple-quoted strings before single-quoted ones,
  // datetimes before bare numbers (a date starts with digits), inf/nan and
  // hex/oct/bin before decimals, and quoted strings before everything so
  // `--`, `#` or `=` inside quotes never matches a later alternative.
  const re =
    /("""(?:[^"\\]|\\.|"(?!""))*"""|'''(?:[^'\\]|\\.|'(?!''))*'''|"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'|\d{4}-\d{2}-\d{2}(?:[T ]\d{2}:\d{2}(?::\d{2}(?:\.\d+)?)?(?:Z|[+-]\d{2}:?\d{2})?)?|\d{2}:\d{2}(?::\d{2}(?:\.\d+)?)?|[+-]?(?:inf|nan)|0x[0-9A-Fa-f_]+|0o[0-7_]+|0b[01_]+|-?\d[\d_]*(?:\.\d[\d_]*)?(?:[eE][+-]?\d[\d_]*)?|\b(?:true|false|yes|no|on|off)\b|--[A-Za-z0-9_-]+(?:=\S*)?|#.*$|[\[\]{},]|=)/g;
  let last = 0;
  for (let m = re.exec(value); m; m = re.exec(value)) {
    if (m.index > last) out.push(plain(value.slice(last, m.index)));
    const tok = m[0];
    if (tok.startsWith('#')) out.push(span('syntax-comment', tok));
    else if (tok.startsWith('"') || tok.startsWith("'")) out.push(span('syntax-string', tok));
    else if (tok.startsWith('--')) out.push(span('syntax-func', tok));
    else if (/^(true|false|yes|no|on|off)$/.test(tok)) out.push(span('syntax-keyword', tok));
    else if (/^[+-]?(inf|nan)$/.test(tok)) out.push(span('syntax-keyword', tok));
    else if (/^[\[\]{},]$/.test(tok)) out.push(span('syntax-punct', tok));
    else if (tok === '=') out.push(span('syntax-operator', tok));
    else out.push(span('syntax-number', tok));
    last = m.index + tok.length;
  }
  if (last < value.length) out.push(plain(value.slice(last)));
  return out;
}

/** Split a key into quoted segments (strings) and bare/dotted parts. */
function keyParts(key: string): React.ReactNode[] {
  const out: React.ReactNode[] = [];
  const re = /("(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*')/g;
  let last = 0;
  for (let m = re.exec(key); m; m = re.exec(key)) {
    if (m.index > last) out.push(span('syntax-func', key.slice(last, m.index)));
    out.push(span('syntax-string', m[0]));
    last = m.index + m[0].length;
  }
  if (last < key.length) out.push(span('syntax-func', key.slice(last)));
  return out;
}

function tomlLine(line: string): React.ReactNode[] {
  key = 0;
  const comment = line.match(/^(\s*)(#.*)$/);
  if (comment) return [plain(comment[1]), span('syntax-comment', comment[2])];
  const section = line.match(/^(\s*)(\[{1,2}[^\]]+\]{1,2})(.*)$/);
  if (section) {
    return [plain(section[1]), span('syntax-section', section[2]), ...valueParts(section[3])];
  }
  // Bare, "basic", and 'literal' keys, dotted or alone. Quoted segments
  // highlight as strings so `"weird.key".plain` reads correctly.
  const kv = line.match(/^(\s*)((?:"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'|[A-Za-z0-9_-]+)(?:\s*\.\s*(?:"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'|[A-Za-z0-9_-]+))*)(\s*=\s*)(.*)$/);
  if (kv) {
    return [plain(kv[1]), ...keyParts(kv[2]), span('syntax-operator', kv[3]), ...valueParts(kv[4])];
  }
  // No `key =` on this line: multi-line array items, continuation lines,
  // bare `],` closers. Still scan for values so strings inside them theme.
  return valueParts(line);
}

function iniLine(line: string): React.ReactNode[] {
  key = 0;
  const comment = line.match(/^(\s*)([#;].*)$/);
  if (comment) return [plain(comment[1]), span('syntax-comment', comment[2])];
  const section = line.match(/^(\s*)(\[[^\]]+\])(.*)$/);
  if (section) {
    return [plain(section[1]), span('syntax-section', section[2]), ...valueParts(section[3])];
  }
  // Systemd directives: `Key=value…` — the key is the stable vocabulary.
  const kv = line.match(/^(\s*)([A-Za-z][A-Za-z0-9]*)(=)(.*)$/);
  if (kv) {
    return [
      plain(kv[1]),
      span('syntax-func', kv[2]),
      span('syntax-operator', kv[3]),
      ...valueParts(kv[4]),
    ];
  }
  // Continuation lines (long values wrapped by the generator).
  return valueParts(line);
}

function containerfileLine(line: string): React.ReactNode[] {
  key = 0;
  const comment = line.match(/^(\s*)(#.*)$/);
  if (comment) return [plain(comment[1]), span('syntax-comment', comment[2])];
  const instr = line.match(/^(\s*)([A-Z]+)\b(.*)$/);
  if (!instr) return valueParts(line);
  const out: React.ReactNode[] = [plain(instr[1]), span('syntax-keyword', instr[2])];
  // Flags, shell operators, and the rest of the arguments.
  const rest = instr[3];
  const re = /(\s+|--[A-Za-z0-9-]+(?:=\S*)?|&&|\|\||[|&;\\]|\b(?:true|false)\b)/g;
  let last = 0;
  for (let m = re.exec(rest); m; m = re.exec(rest)) {
    if (m.index > last) out.push(...valueParts(rest.slice(last, m.index)));
    const tok = m[0];
    if (/^\s+$/.test(tok)) out.push(plain(tok));
    else if (tok.startsWith('--')) out.push(span('syntax-func', tok));
    else if (/^(true|false)$/.test(tok)) out.push(span('syntax-keyword', tok));
    else out.push(span('syntax-operator', tok));
    last = m.index + tok.length;
  }
  if (last < rest.length) out.push(...valueParts(rest.slice(last)));
  return out;
}

const SHELL_COMMAND_POS = /(^|[|;&]\s*|&&\s*|\|\|\s*|\$\(\s*)$/;

function shellLine(line: string): React.ReactNode[] {
  key = 0;
  const out: React.ReactNode[] = [];
  // Strings first so `#`, `|` or `--x` inside quotes stay part of the string;
  // a `#` only starts a comment at line start or after whitespace.
  const re =
    /("(?:[^"\\]|\\.)*"|'[^']*'|(?:^|(?<=\s))#.*$|\$\{[^}]*\}|\$[A-Za-z_][A-Za-z0-9_]*|\$\(|\s--?[A-Za-z0-9][A-Za-z0-9_-]*(?:=\S*)?|&&|\|\||[|;&<>]|\\$|[^\s"'|;&<>$]+)/g;
  let last = 0;
  for (let m = re.exec(line); m; m = re.exec(line)) {
    if (m.index > last) out.push(plain(line.slice(last, m.index)));
    const tok = m[0];
    const before = line.slice(0, m.index);
    if (tok.startsWith('#')) out.push(span('syntax-comment', tok));
    else if (tok.startsWith('"') || tok.startsWith("'")) out.push(span('syntax-string', tok));
    else if (tok.startsWith('$')) out.push(span('syntax-variable', tok));
    else if (/^\s--?/.test(tok)) {
      out.push(plain(tok[0]));
      out.push(span('syntax-operator', tok.slice(1)));
    } else if (/^(&&|\|\||[|;&<>]|\\)$/.test(tok)) out.push(span('syntax-punct', tok));
    else if (SHELL_COMMAND_POS.test(before.trim().replace(/(^|[|;&]\s*)(sudo|time|exec)$/, '$1'))) out.push(span('syntax-func', tok));
    else out.push(plain(tok));
    last = m.index + tok.length;
  }
  if (last < line.length) out.push(plain(line.slice(last)));
  return out;
}

function jsonLine(line: string): React.ReactNode[] {
  key = 0;
  const out: React.ReactNode[] = [];
  const re = /("(?:[^"\\]|\\.)*")(\s*:)?|-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?|\b(?:true|false|null)\b|\/\/.*$|[{}\[\],]/g;
  let last = 0;
  for (let m = re.exec(line); m; m = re.exec(line)) {
    if (m.index > last) out.push(plain(line.slice(last, m.index)));
    const tok = m[0];
    if (m[1] && m[2]) {
      out.push(span('syntax-func', m[1]));
      out.push(span('syntax-punct', m[2]));
    } else if (m[1]) out.push(span('syntax-string', tok));
    else if (tok.startsWith('//')) out.push(span('syntax-comment', tok));
    else if (/^(true|false|null)$/.test(tok)) out.push(span('syntax-keyword', tok));
    else if (/^[{}\[\],]$/.test(tok)) out.push(span('syntax-punct', tok));
    else out.push(span('syntax-number', tok));
    last = m.index + tok.length;
  }
  if (last < line.length) out.push(plain(line.slice(last)));
  return out;
}

/** Highlight one line for the given preview language. */
export function highlightLine(line: string, lang: PreviewLang): React.ReactNode[] {
  switch (lang) {
    case 'toml':
      return tomlLine(line);
    case 'quadlet':
      return iniLine(line);
    case 'containerfile':
      return containerfileLine(line);
    case 'shell':
      return shellLine(line);
    case 'json':
      return jsonLine(line);
    default:
      return [line];
  }
}

type TripleDelim = '"""' | "'''";

/**
 * Escape-aware closer search. In basic (`"""`) strings `\"` is an escape, so
 * a delimiter preceded by an odd run of backslashes doesn't terminate;
 * literal (`'''`) strings have no escapes, so the first occurrence wins.
 */
function findCloser(text: string, delim: TripleDelim): number {
  if (delim === "'''") return text.indexOf(delim);
  let i = text.indexOf(delim);
  while (i !== -1) {
    let bs = 0;
    for (let j = i - 1; j >= 0 && text[j] === '\\'; j--) bs++;
    if (bs % 2 === 0) return i;
    i = text.indexOf(delim, i + 1);
  }
  return -1;
}

/**
 * Find a multi-line string opener on a fresh line: the first `"""`/`'''`
 * that is neither inside a single-line string or comment nor closed again
 * on the same line.
 */
function findMultilineOpener(line: string): { delim: TripleDelim; index: number } | null {
  const re = /("""|'''|"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'|#.*$)/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(line))) {
    const tok = m[0];
    if (tok !== '"""' && tok !== "'''") continue; // string/comment consumed
    if (findCloser(line.slice(m.index + 3), tok as TripleDelim) !== -1) continue; // complete
    return { delim: tok as TripleDelim, index: m.index };
  }
  return null;
}

/** Tokenize a line fragment (the `key = ` prefix before a string opener). */
function fragmentNodes(fragment: string): React.ReactNode[] {
  const kv = fragment.match(
    /^(\s*)((?:"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'|[A-Za-z0-9_-]+)(?:\s*\.\s*(?:"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'|[A-Za-z0-9_-]+))*)(\s*=\s*)(.*)$/
  );
  if (kv) {
    return [plain(kv[1]), ...keyParts(kv[2]), span('syntax-operator', kv[3]), ...valueParts(kv[4])];
  }
  return valueParts(fragment);
}

function tomlDocument(lines: string[]): React.ReactNode[][] {
  const out: React.ReactNode[][] = [];
  let multi: TripleDelim | null = null;
  for (const line of lines) {
    key = 0;
    if (!multi) {
      const opener = findMultilineOpener(line);
      if (!opener) {
        out.push(tomlLine(line));
        continue;
      }
      out.push([
        ...fragmentNodes(line.slice(0, opener.index)),
        span('syntax-string', line.slice(opener.index)),
      ]);
      multi = opener.delim;
      continue;
    }
    const c = findCloser(line, multi);
    if (c === -1) {
      out.push([span('syntax-string', line)]);
    } else {
      const end = c + 3;
      out.push([span('syntax-string', line.slice(0, end)), ...valueParts(line.slice(end))]);
      multi = null;
    }
  }
  return out;
}

/**
 * Highlight a whole document, tracking multi-line `"""`/`'''` strings
 * across lines so their contents theme as strings instead of plain text.
 * Single-line languages map per line; per-line spans reset keys each row.
 */
export function highlightDocument(code: string, lang: PreviewLang): React.ReactNode[][] {
  const lines = code.split('\n');
  if (lang !== 'toml') return lines.map(highlightLineWithLang(lang));
  return tomlDocument(lines);
}

const highlightLineWithLang =
  (lang: PreviewLang) =>
  (line: string): React.ReactNode[] =>
    highlightLine(line, lang);
