//! Field-to-source mapping for TOML definition files.
//!
//! Maps a dotted schema path (`container.memory`, `provision.2.run`) to the
//! byte span and (line, column) it occupies in the document, using
//! `toml_edit` spans. Every consumer of field positions — the JSON contract
//! error spans and the Studio's field-to-line highlighting — goes through
//! this one implementation, so quoted keys, comments in table headers,
//! inline tables and array-of-tables indices are all handled identically.
//!
//! Path notation: dot-separated segments. Keys that are not bare words may
//! be quoted (`container."my key"`), and array-of-tables positions are
//! referenced either as dedicated index segments (`provision.2.run`) or
//! bracketed (`provision[2].run`).
//!
//! Pure module: no host dependencies, compiles for wasm.

use std::collections::BTreeMap;
use std::ops::Range;

use toml_edit::{ImDocument, Item, Key, TableLike};

/// Where a field path's value sits in the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldSpan {
    /// 1-based line of the field's first byte.
    pub line: usize,
    /// 1-based column (Unicode scalar units, like editors render) of the
    /// field's first byte.
    pub column: usize,
    /// Inclusive byte range (start, end) in the document.
    pub span: (usize, usize),
}

/// Locate a dotted path in a TOML document.
///
/// Returns `None` for anything the document does not contain: derived and
/// defaulted values have no source position.
pub fn field_span(text: &str, path: &str) -> Option<FieldSpan> {
    let doc = ImDocument::parse(text).ok()?;
    let segments = fold_indices(split_path(path));
    let root = doc.as_table();
    let range = walk_root(root, &segments)?;
    let (line, column) = offset_to_line_col(text, range.start);
    Some(FieldSpan {
        line,
        column,
        span: (range.start, range.end),
    })
}

/// Locate a dotted path and return only its 1-based line number.
pub fn field_line(text: &str, path: &str) -> Option<usize> {
    field_span(text, path).map(|s| s.line)
}

/// Scan a document into a `dotted.path -> 1-based line` map.
///
/// Covers `[table]` and `[[array]]` headers (trailing comments do not
/// shift the line), `key =` assignments qualified by their table,
/// inline-table members, and indexed array-of-tables entries
/// (`provision.0`). Comment-only and blank lines contribute nothing. The
/// batch form backs the Studio's field-to-line highlighting; see
/// [`field_span`] for single lookups.
pub fn field_lines(text: &str) -> BTreeMap<String, usize> {
    let doc = ImDocument::parse(text);
    let mut map = BTreeMap::new();
    if let Ok(doc) = doc.as_ref() {
        collect(doc.as_table(), String::new(), text, &mut map);
    }
    map
}

// ── path parsing ──────────────────────────────────────────────────────────

/// One path element: a bare or quoted key, plus an optional
/// array-of-tables index (`[[provision]]` position).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Segment {
    key: String,
    index: Option<usize>,
}

/// Split a dotted path into segments.
///
/// Quotes win over separators: `container."a.b"` is two segments, and
/// `provision[2].run` is a key-with-index followed by a key. A whole
/// element made of digits becomes an index into the previous segment's
/// array (`provision.2`).
fn split_path(path: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut element = String::new();
    let mut index_buffer: Option<String> = None;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for ch in path.chars() {
        if let Some(q) = quote {
            element.push(ch);
            if escaped {
                escaped = false;
            } else if ch == q {
                quote = None;
            } else if ch == '\\' {
                escaped = true;
            }
            continue;
        }
        match ch {
            '"' | '\'' => {
                quote = Some(ch);
                element.push(ch);
            }
            '.' => push_element(&mut segments, &mut element, &mut index_buffer),
            '[' if index_buffer.is_none() => {
                index_buffer = Some(String::new());
            }
            ']' => push_element(&mut segments, &mut element, &mut index_buffer),
            _ => match index_buffer.as_mut() {
                Some(buffer) => buffer.push(ch),
                None => element.push(ch),
            },
        }
    }
    push_element(&mut segments, &mut element, &mut index_buffer);
    segments
}

/// Turn the buffered text into a segment and reset the buffers.
///
/// `provision[2]` carries both parts; a lone `2` between dots is an index;
/// anything else is a key.
fn push_element(
    segments: &mut Vec<Segment>,
    element: &mut String,
    index_buffer: &mut Option<String>,
) {
    let index = index_buffer.take().and_then(|i| i.parse::<usize>().ok());
    let raw = std::mem::take(element);
    let trimmed = raw.trim();
    if index.is_none()
        && !trimmed.starts_with('"')
        && !trimmed.starts_with('\'')
        && !trimmed.is_empty()
        && trimmed.bytes().all(|b| b.is_ascii_digit())
    {
        if let Ok(i) = trimmed.parse::<usize>() {
            segments.push(Segment {
                key: String::new(),
                index: Some(i),
            });
            return;
        }
    }
    if trimmed.is_empty() {
        if let Some(i) = index {
            segments.push(Segment {
                key: String::new(),
                index: Some(i),
            });
        }
        return;
    }
    segments.push(Segment {
        key: unquote_key(trimmed),
        index,
    });
}

/// Fold a bare index element into the segment before it, so walkers only
/// ever see `key` and `key+index` pairs.
fn fold_indices(segments: Vec<Segment>) -> Vec<Segment> {
    let mut folded: Vec<Segment> = Vec::with_capacity(segments.len());
    for segment in segments {
        let is_index_only = segment.key.is_empty();
        if is_index_only {
            match folded.last_mut() {
                Some(previous) if previous.index.is_none() => {
                    previous.index = segment.index;
                    continue;
                }
                _ => continue, // No owner to fold into: the path is broken.
            }
        }
        folded.push(segment);
    }
    folded
}

/// Decode a quoted key (`"my key"`) into its TOML value; bare keys pass
/// through unchanged.
fn unquote_key(key: &str) -> String {
    let bytes = key.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        if (first == b'"' || first == b'\'') && first == bytes[bytes.len() - 1] {
            if let Some(inner) = key.get(1..key.len() - 1) {
                return unescape_basic(inner);
            }
        }
    }
    key.to_string()
}

/// Best-effort TOML basic-string unescape for key names.
fn unescape_basic(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('u') => {
                let code: String = chars.by_ref().take(4).collect();
                if let Ok(n) = u32::from_str_radix(&code, 16)
                    && let Some(decoded) = char::from_u32(n)
                {
                    out.push(decoded);
                }
            }
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// Inverse of [`unescape_basic`] plus quoting, used for map keys so a key
/// with a dot stays one path element.
fn encode_key(key: &str) -> String {
    if !key.is_empty()
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return key.to_string();
    }
    let mut out = String::with_capacity(key.len() + 2);
    out.push('"');
    for c in key.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Convert a byte offset into 1-based (line, column). Columns count Unicode
/// scalar values, matching how editors render the line.
pub fn offset_to_line_col(text: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(text.len());
    let before = &text[..offset];
    let line = before.bytes().filter(|&b| b == b'\n').count() + 1;
    let column = before.rsplit('\n').next().unwrap_or("").chars().count() + 1;
    (line, column)
}

// ── lookup ────────────────────────────────────────────────────────────────

/// Resolve `segments` from the root table. What comes back is the widest
/// span the path names: from the key's first byte through the value's last
/// byte (or a table's header through its contents, which puts the header
/// line in `line`). Array-of-tables indices select one entry's header.
fn walk_root(root: &toml_edit::Table, segments: &[Segment]) -> Option<Range<usize>> {
    let (first, rest) = segments.split_first()?;
    if first.key.is_empty() {
        return None;
    }
    let (key, item) = root.get_key_value(&first.key)?;
    span_into(key, item, first, rest)
}

/// Continue resolving from an already-selected `key`/`item` pair.
///
/// `first`'s index, when present, selects a `[[table]]` entry; `rest` then
/// descends inside whatever lands there.
fn span_into(key: &Key, item: &Item, first: &Segment, rest: &[Segment]) -> Option<Range<usize>> {
    if let Some(i) = first.index {
        let entry = item.as_array_of_tables()?.iter().nth(i)?;
        if rest.is_empty() {
            return entry.span();
        }
        return walk_table(entry, rest);
    }
    if rest.is_empty() {
        // A dotted key's intermediate table carries no span of its own;
        // the union of the leaves it nests stands in for it.
        if let Item::Table(t) = item {
            return union(key, item).or_else(|| dotted_union(t));
        }
        return union(key, item);
    }
    // More path is left underneath: nested `[table]`s and inline tables
    // both expose their children through `TableLike`.
    let (next, deeper) = rest.split_first()?;
    let (next_key, next_item) = table_body(item)?.get_key_value(&next.key)?;
    span_into(next_key, next_item, next, deeper)
}

/// Union of the leaf spans of a dotted-key table (`a.b = 1`): from the
/// first leaf key's first byte to the last leaf value's last byte.
fn dotted_union(table: &toml_edit::Table) -> Option<Range<usize>> {
    let mut union: Option<Range<usize>> = None;
    for (keys, value) in table.get_values() {
        let Some(span) = value.span() else {
            continue;
        };
        let head = keys
            .iter()
            .filter_map(|k| k.span().map(|k| k.start))
            .min()
            .unwrap_or(span.start);
        let range = head.min(span.start)..span.end.max(head);
        union = Some(match union {
            Some(existing) => existing.start.min(range.start)..existing.end.max(range.end),
            None => range,
        });
    }
    union
}

/// The byte span a reached key/value pair covers, from the start of its
/// key (`"a.b"` quotes included) through the end of its value, missing
/// when either side carries no span representation.
fn union(key: &Key, item: &Item) -> Option<Range<usize>> {
    let value_span = item.span()?;
    let head = key.span().map(|k| k.start).unwrap_or(value_span.start);
    Some(head.min(value_span.start)..value_span.end.max(head))
}

/// Interpret an item as a table body. Arrays and scalars are leaves and
/// yield nothing further.
fn table_body(item: &Item) -> Option<&dyn TableLike> {
    match item {
        Item::Table(t) => Some(t),
        Item::Value(v) => v.as_inline_table().map(|i| i as &dyn TableLike),
        _ => None,
    }
}

// ── array-of-tables sentinel for a value held in `Item::Table` position ──
fn walk_table(table: &toml_edit::Table, segments: &[Segment]) -> Option<Range<usize>> {
    let (first, rest) = segments.split_first()?;
    if first.key.is_empty() {
        return None;
    }
    let (key, item) = table.get_key_value(&first.key)?;
    span_into(key, item, first, rest)
}

// ── batch scan ────────────────────────────────────────────────────────────

/// Recursively record every key and sub-table header of `table`, keyed by
/// dotted path with array-of-tables positions folded in (`provision.0`).
fn collect(table: &dyn TableLike, prefix: String, text: &str, out: &mut BTreeMap<String, usize>) {
    for (key, item) in table.iter() {
        let path = if prefix.is_empty() {
            encode_key(key)
        } else {
            format!("{prefix}.{}", encode_key(key))
        };
        // Dotted-key intermediate tables carry no span of their own; the
        // union of the leaves they nest stands in for them.
        let at = if let Item::Table(child) = item {
            child.span().or_else(|| dotted_union(child))
        } else {
            item.span()
        };
        record(out, &path, at, text);
        match item {
            Item::Table(child) => collect(child, path, text, out),
            Item::Value(value) => {
                if let Some(inline) = value.as_inline_table() {
                    collect(inline, path, text, out);
                }
            }
            Item::ArrayOfTables(entries) => {
                for (i, entry) in entries.iter().enumerate() {
                    let indexed = format!("{path}.{i}");
                    record(out, &indexed, entry.span(), text);
                    collect(entry, indexed, text, out);
                }
            }
            _ => {}
        }
    }
}

/// Record the line a location's first byte sits on. A key carries its own
/// span when it has a quoted representation; the value's span (or a table
/// header's) is the fallback so header lines still resolve.
fn record(out: &mut BTreeMap<String, usize>, path: &str, at: Option<Range<usize>>, text: &str) {
    let Some(span) = at else {
        return;
    };
    let (line, _) = offset_to_line_col(text, span.start);
    out.entry(path.to_string()).or_insert(line);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(toml: &str, path: &str) -> String {
        let s = field_span(toml, path).expect("span");
        format!("{}:{} {}", s.line, s.column, &toml[s.span.0..s.span.1])
    }

    // A header with a trailing comment keeps both the header and its keys
    // on their own lines (the old line-scan matched keys after `[t] # c`).
    #[test]
    fn header_with_comment() {
        let toml = "[container] # the box\nname = \"x\"\n";
        let lines = field_lines(toml);
        assert_eq!(lines.get("container"), Some(&1));
        assert_eq!(lines.get("container.name"), Some(&2));
        assert_eq!(field_line(toml, "container.name"), Some(2));
        assert_eq!(field_line(toml, "container"), Some(1));
        assert_eq!(span(toml, "container.name"), "2:1 name = \"x\"");
    }

    #[test]
    fn inline_tables() {
        let toml = "[container]\nenv = { FOO = \"bar\", forward = [\"A\"] }\n";
        assert_eq!(field_line(toml, "container.env"), Some(2));
        assert_eq!(field_line(toml, "container.env.FOO"), Some(2));
        let lines = field_lines(toml);
        assert_eq!(lines.get("container.env.FOO"), Some(&2));
        assert_eq!(lines.get("container.env.forward"), Some(&2));
    }

    #[test]
    fn array_of_tables_index_one() {
        let toml = "[[provision]]\nname = \"a\"\nrun = \"x\"\n\n[[provision]]\nname = \"b\"\n";
        assert_eq!(field_line(toml, "provision.1.name"), Some(6));
        assert_eq!(field_line(toml, "provision[1].name"), Some(6));
        assert_eq!(field_line(toml, "provision.0.name"), Some(2));
        assert_eq!(field_line(toml, "provision.1"), Some(5));
        assert_eq!(field_line(toml, "provision.2.name"), None);
        let lines = field_lines(toml);
        assert_eq!(lines.get("provision"), Some(&1));
        assert_eq!(lines.get("provision.1.name"), Some(&6));
        assert_eq!(lines.get("provision.0.run"), Some(&3));
    }

    #[test]
    fn dotted_keys() {
        let toml = "image.packages.install = [\"git\"]\n";
        assert_eq!(field_line(toml, "image.packages.install"), Some(1));
        let lines = field_lines(toml);
        assert_eq!(lines.get("image.packages.install"), Some(&1));
        assert_eq!(lines.get("image.packages"), Some(&1));
    }

    #[test]
    fn quoted_keys() {
        let toml = "[container]\n\"my key\" = \"v\"\n[container.env]\na.b = \"c\"\n";
        assert_eq!(field_line(toml, "container.\"my key\""), Some(2));
        assert_eq!(field_line(toml, "container.\"my.key\""), None);
        // A dotted key inside a header table is nested by the parser, so
        // the path names its parts.
        assert_eq!(field_line(toml, "container.env.a.b"), Some(4));
        let lines = field_lines(toml);
        assert_eq!(lines.get("container.\"my key\""), Some(&2));
        assert_eq!(lines.get("container.env.a.b"), Some(&4));
    }

    #[test]
    fn missing_paths_and_ancestors() {
        let toml = "[container]\nname = \"x\"\n";
        assert_eq!(field_line(toml, "container.missing"), None);
        assert_eq!(field_line(toml, "nosuch.key"), None);
        assert_eq!(field_line(toml, ""), None);
    }
}
