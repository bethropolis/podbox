use std::path::Path;

use anyhow::{Context, Result};

use podbox::cli::OutputFormat;
use podbox::config::Config;
use podbox::error::PodboxError;

/// Split `table.key.path` into segments. Empty paths and empty segments
/// (`a..b`) are rejected; array indices (`provision[0]`) are not supported.
fn split_key(key: &str) -> Result<Vec<&str>> {
    let segments: Vec<&str> = key.split('.').collect();
    if key.is_empty() || segments.iter().any(|s| s.is_empty()) {
        anyhow::bail!("invalid key '{key}': expected a dotted table.key path");
    }
    Ok(segments)
}

/// Walk a TOML value by path segments.
fn lookup<'v>(value: &'v toml::Value, segments: &[&str]) -> Option<&'v toml::Value> {
    let mut current = value;
    for seg in segments {
        current = current.get(seg)?;
    }
    Some(current)
}

/// Print one effective configuration value.
///
/// Reads the loaded config, so defaults and `extends` are applied: what
/// `get` prints is what podbox runs with. Scalars print raw (script
/// friendly); tables and arrays print as pretty JSON.
pub fn run_get(config: &Config, key: &str, output: OutputFormat) -> Result<()> {
    let segments = split_key(key)?;
    let value = toml::Value::try_from(config)?;
    let found = lookup(&value, &segments).ok_or_else(|| anyhow::anyhow!("unknown key '{key}'"))?;
    match output {
        OutputFormat::Ndjson => return Err(podbox::contract::bail_ndjson("config get")),
        OutputFormat::Json => {
            podbox::contract::print_json(serde_json::to_value(found)?)?;
        }
        OutputFormat::Text => {
            println!("{}", render_text(found));
        }
    }
    Ok(())
}

fn render_text(value: &toml::Value) -> String {
    match value {
        toml::Value::String(s) => s.clone(),
        toml::Value::Integer(i) => i.to_string(),
        toml::Value::Float(f) => f.to_string(),
        toml::Value::Boolean(b) => b.to_string(),
        toml::Value::Datetime(d) => d.to_string(),
        other => serde_json::to_string_pretty(other).unwrap_or_else(|_| format!("{other:?}")),
    }
}

/// Parse a `set` value: valid TOML parses as TOML (`4`, `true`, `["a"]`,
/// `"quoted"`); anything else (`4G`, `bash`) becomes a string.
fn parse_set_value(raw: &str) -> toml_edit::Value {
    format!("v = {raw}")
        .parse::<toml_edit::DocumentMut>()
        .ok()
        .and_then(|doc| doc["v"].as_value().cloned())
        .unwrap_or_else(|| toml_edit::Value::from(raw))
}

/// Set one configuration value, preserving comments and formatting.
///
/// The edit is verified before it lands: the new text must parse (with
/// `extends` resolved) and the key must exist in the resulting config, so
/// typos fail instead of writing dead keys. Nothing is written on failure.
pub fn run_set(
    path: &Path,
    key: &str,
    raw_value: &str,
    dry_run: bool,
    output: OutputFormat,
) -> Result<()> {
    let segments = split_key(key)?;
    let original = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read '{}'", path.display()))?;
    let mut doc: toml_edit::DocumentMut = original
        .parse()
        .with_context(|| format!("failed to parse '{}'", path.display()))?;

    let new_value = parse_set_value(raw_value);
    let (leaf, parents) = segments.split_last().expect("split_key rejects empty");
    let mut table = doc.as_table_mut();
    for seg in parents {
        match table.get(seg) {
            None => {
                table.insert(seg, toml_edit::Item::Table(toml_edit::Table::new()));
            }
            Some(item) if !item.is_table() => {
                anyhow::bail!("'{seg}' in '{key}' is not a table");
            }
            Some(_) => {}
        }
        table = table
            .get_mut(seg)
            .and_then(|item| item.as_table_mut())
            .ok_or_else(|| anyhow::anyhow!("'{seg}' in '{key}' is not a table"))?;
    }
    table.insert(leaf, toml_edit::Item::Value(new_value.clone()));

    let new_text = doc.to_string();
    let checked = Config::parse_with_source(path, &new_text)?;
    // Unknown keys deserialize into the void: confirm the key survived the
    // round trip through the schema. (Empty collections serialize away, so
    // they skip this check.)
    let round_tripped = toml::Value::try_from(&checked)?;
    let empty_collection = matches!(
        new_value,
        toml_edit::Value::Array(ref a) if a.is_empty()
    ) || matches!(
        new_value,
        toml_edit::Value::InlineTable(ref t) if t.is_empty()
    );
    if !empty_collection && lookup(&round_tripped, &segments).is_none() {
        anyhow::bail!("unknown key '{key}'");
    }

    if dry_run {
        podbox::ui::would(&format!("set {key} = {raw_value} in {}", path.display()));
        return Ok(());
    }
    podbox::fsutil::write_atomic(path, new_text.as_bytes())?;
    let rebuild_required = segments[0] == "image";
    match output {
        OutputFormat::Ndjson => return Err(podbox::contract::bail_ndjson("config set")),
        OutputFormat::Json => {
            podbox::contract::print_json(serde_json::json!({
                "ok": true,
                "key": key,
                "rebuild_required": rebuild_required,
            }))?;
        }
        OutputFormat::Text => {
            println!("Set {key}.");
        }
    }
    if rebuild_required {
        podbox::ui::warn("image configuration changed — run `podbox build` to rebuild.");
    }
    Ok(())
}

/// Validate the configuration file, reporting errors with line numbers.
///
/// Syntax is checked against the raw file (file coordinates); semantic
/// errors come from the `extends`-aware load, with best-effort line lookup
/// (inherited fields have no source line). Prints the verdict, then fails
/// with a report-only error when invalid so JSON mode stays one document.
pub fn run_validate(path: &Path, output: OutputFormat) -> Result<()> {
    use podbox::contract::{find_field_line, offset_to_line_col};

    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read '{}'", path.display()))?;
    let mut errors: Vec<serde_json::Value> = Vec::new();

    let syntax_ok = match text.parse::<toml::Value>() {
        Ok(_) => true,
        Err(e) => {
            let (line, col) = e
                .span()
                .map(|range| offset_to_line_col(&text, range.start))
                .unzip();
            errors.push(serde_json::json!({
                "field": null,
                "line": line,
                "col": col,
                "message": e.to_string(),
            }));
            false
        }
    };

    if syntax_ok {
        match Config::load(path) {
            Ok(_) => {}
            Err(e) => match e.downcast_ref::<PodboxError>() {
                Some(PodboxError::ConfigValidationFailed { details }) => {
                    for entry in details.split("\n  - ") {
                        let (field, message) = match entry.split_once(": ") {
                            Some((f, m)) => (Some(f.to_string()), m.to_string()),
                            None => (None, entry.to_string()),
                        };
                        let line = field.as_deref().and_then(|f| find_field_line(&text, f));
                        errors.push(serde_json::json!({
                            "field": field,
                            "line": line,
                            "col": null,
                            "message": message,
                        }));
                    }
                }
                _ => errors.push(serde_json::json!({
                    "field": null,
                    "line": null,
                    "col": null,
                    "message": format!("{e:#}"),
                })),
            },
        }
    }

    let valid = errors.is_empty();
    match output {
        OutputFormat::Ndjson => return Err(podbox::contract::bail_ndjson("config validate")),
        OutputFormat::Json => {
            podbox::contract::print_json(serde_json::json!({
                "valid": valid,
                "errors": errors,
            }))?;
        }
        OutputFormat::Text => {
            if valid {
                println!("Configuration valid.");
            } else {
                for e in &errors {
                    let field = e["field"].as_str().unwrap_or("");
                    let line = e["line"]
                        .as_u64()
                        .map(|n| format!("line {n}"))
                        .unwrap_or_default();
                    let col = e["col"]
                        .as_u64()
                        .map(|n| format!(", col {n}"))
                        .unwrap_or_default();
                    let message = e["message"].as_str().unwrap_or("");
                    let at = if field.is_empty() && line.is_empty() {
                        String::new()
                    } else {
                        format!("{field} ({line}{col}): ")
                    };
                    println!("  - {at}{message}");
                }
            }
        }
    }
    if valid {
        Ok(())
    } else {
        Err(PodboxError::ConfigInvalid(errors.len()).into())
    }
}
