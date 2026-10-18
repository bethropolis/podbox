//! `podbox-wasm` — WebAssembly bindings for the website Studio.
//!
//! Exposes the exact CLI validation rules (`Config::parse`) and Quadlet
//! codegen (`codegen::quadlet`) to JavaScript, evaluated against a
//! deterministic mock host ([`HostEnv::mock_preview`]) so Studio previews
//! match `podbox enable` output for the same TOML.
//!
//! Nothing on this path may print (`println!`/`eprintln!` panic on
//! `wasm32-unknown-unknown`); diagnostics travel through return values.

use serde::Serialize;
use wasm_bindgen::prelude::*;

use podbox::config::Config;
use podbox::contract::{find_field_line, offset_to_line_col};
use podbox::env::HostEnv;
use podbox::error::PodboxError;
use podbox::tomlscan;
use podbox::xdg::ResolvedXdgDirs;

#[derive(Serialize)]
pub struct ValidationError {
    /// Dotted schema path (`container.memory`), when the issue is a schema
    /// violation of the form `<field>: <reason>`. `None` for TOML syntax
    /// errors, which carry no field target.
    pub field: Option<String>,
    pub message: String,
    /// 1-based line of the issue in the submitted TOML, when locatable.
    pub line: Option<usize>,
    /// 1-based column of the issue in the submitted TOML, when locatable.
    pub column: Option<usize>,
    /// Byte offsets (start, end) in the submitted TOML, when locatable.
    pub span: Option<(usize, usize)>,
}

#[derive(Serialize)]
pub struct ValidationResponse {
    pub valid: bool,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<String>,
}

#[derive(Serialize)]
pub struct CompileResponse {
    pub container: String,
    pub socket: String,
    pub build: Option<String>,
    /// Companion host socket server unit (`{name}-host.service`). Always
    /// present: the socket unit references it.
    pub host_service: String,
    /// D-Bus proxy unit. `None` when the proxy is disabled in config.
    pub dbus_proxy_service: Option<String>,
    /// Wayland firewall unit. `None` when the proxy is disabled in config.
    pub compositor_service: Option<String>,
    /// Rendered Containerfile. Always present: prebuilt images get the short
    /// overlay, and a custom `base` gets the full recipe with the `podbox-guest`
    /// layer marked as a placeholder (this wasm build embeds no guest binary).
    /// The placeholder lives in the text rather than in `warnings` — it describes
    /// the preview environment, not a problem with the submitted config.
    pub containerfile: Option<String>,
    pub warnings: Vec<String>,
}

/// Split an `anyhow` parse error into Studio-ready issues.
///
/// Schema violations arrive as `ConfigValidationFailed { details }` with
/// items joined by `"\n  - "`, each shaped `<field>: <reason>`; the field
/// target is split off for input highlighting and resolved to a source line
/// in the submitted TOML. TOML syntax errors carry the parser's byte span,
/// converted to line/column here instead of regexing the message.
fn validation_errors(err: &anyhow::Error, toml_str: &str) -> Vec<ValidationError> {
    if let Some(PodboxError::ConfigValidationFailed { details }) = err.downcast_ref::<PodboxError>()
    {
        details
            .split("\n  - ")
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|line| match line.split_once(':') {
                Some((field, message)) => {
                    let field = field.trim().to_string();
                    let line_no = find_field_line(toml_str, &field);
                    ValidationError {
                        field: Some(field),
                        message: message.trim().to_string(),
                        line: line_no,
                        column: None,
                        span: None,
                    }
                }
                None => ValidationError {
                    field: None,
                    message: line.to_string(),
                    line: None,
                    column: None,
                    span: None,
                },
            })
            .collect()
    } else if let Some(de) = err.downcast_ref::<toml::de::Error>() {
        let (line, column, span) = match de.span() {
            Some(range) => {
                let (l, c) = offset_to_line_col(toml_str, range.start);
                (Some(l), Some(c), Some((range.start, range.end)))
            }
            None => (None, None, None),
        };
        vec![ValidationError {
            field: None,
            // `{:#}` renders the full anyhow chain ("failed to parse definition
            // file: TOML parse error at line 1, column 9"); `{}` would show only
            // the outermost context.
            message: format!("{err:#}"),
            line,
            column,
            span,
        }]
    } else {
        vec![ValidationError {
            field: None,
            message: format!("{err:#}"),
            line: None,
            column: None,
            span: None,
        }]
    }
}

/// Validate a TOML definition string with the exact CLI rules.
#[wasm_bindgen]
pub fn validate_toml(toml_str: &str) -> JsValue {
    let response = match Config::parse_with_warnings(toml_str) {
        Ok((_, warnings)) => ValidationResponse {
            valid: true,
            errors: Vec::new(),
            warnings,
        },
        Err(err) => ValidationResponse {
            valid: false,
            errors: validation_errors(&err, toml_str),
            warnings: Vec::new(),
        },
    };
    serde_wasm_bindgen::to_value(&response).unwrap_or(JsValue::NULL)
}

/// Generate the systemd Quadlet units for a TOML definition string.
///
/// Uses the deterministic mock host, so output matches what
/// `podbox enable` would emit on a fully-integrated desktop. Returns an
/// `Err` when the TOML is invalid — call `validate_toml` first for
/// structured issues.
#[wasm_bindgen]
pub fn compile_quadlet(toml_str: &str) -> Result<JsValue, JsError> {
    let (config, mut warnings) =
        Config::parse_with_warnings(toml_str).map_err(|e| JsError::new(&format!("{e:#}")))?;
    let env = HostEnv::mock_preview();
    let xdg = ResolvedXdgDirs::mock_preview();

    let (container, codegen_warnings) =
        podbox::codegen::quadlet::generate_container_with_warnings(&config, &env, &xdg);
    warnings.extend(codegen_warnings);
    let socket = podbox::codegen::quadlet::generate_socket(&config);
    let build = if config.image.image_ref.is_none() {
        // Mirror the CLI `File=` path for the mock home (/home/user).
        let containerfile = env
            .home_dir
            .join(".local/share/podbox")
            .join(&config.container.name)
            .join("Containerfile");
        Some(podbox::codegen::quadlet::generate_build(
            &config,
            &containerfile,
        ))
    } else {
        None
    };

    let containerfile =
        podbox::codegen::containerfile::generate_preview(&config, "podbox-guest", &env);

    // Companion units mirror `podbox enable`. Service binaries resolve via
    // `current_exe`, which has no meaningful value on wasm — the preview
    // falls back to the install path, matching a packaged host.
    let name = &config.container.name;
    let host_service = podbox::codegen::quadlet::generate_host_service(name);
    let dbus_proxy_service = podbox::codegen::quadlet::generate_dbus_proxy_service(name, &config);
    let compositor_service = podbox::codegen::quadlet::generate_compositor_service(name, &config);

    let response = CompileResponse {
        container,
        socket,
        build,
        host_service,
        dbus_proxy_service,
        compositor_service,
        containerfile: Some(containerfile),
        warnings,
    };
    Ok(serde_wasm_bindgen::to_value(&response).unwrap_or(JsValue::NULL))
}

/// Locate a dotted schema path (`container.memory`) in a TOML document.
///
/// Returns the 1-based line number, or `null` when the key has no source
/// line (derived values, defaults, absent keys). Thin wrapper over the same
/// lookup backing error spans; Studio prefers the batch `field_lines_map`
/// equivalent per render and calls this for ad-hoc jumps.
#[wasm_bindgen]
pub fn locate_field_in_toml(toml_str: &str, field_path: &str) -> JsValue {
    serde_wasm_bindgen::to_value(&find_field_line(toml_str, field_path)).unwrap_or(JsValue::NULL)
}

/// Scan a TOML document into a `dotted.path -> 1-based line` source map for
/// field-to-line highlighting in Studio. One call per render; hover and
/// click-to-edit resolve locally from the result. Uses the same span-based
/// scanner as the error lookup, so quoted keys, inline tables and `[[array]]`
/// indices all resolve.
#[wasm_bindgen]
pub fn field_lines_in_toml(toml_str: &str) -> JsValue {
    serde_wasm_bindgen::to_value(&tomlscan::field_lines(toml_str)).unwrap_or(JsValue::NULL)
}

/// Parse a TOML definition into a plain JS object for Studio state import.
#[wasm_bindgen]
pub fn parse_toml_to_json(toml_str: &str) -> Result<JsValue, JsError> {
    let config = Config::parse(toml_str).map_err(|e| JsError::new(&format!("{e:#}")))?;
    Ok(serde_wasm_bindgen::to_value(&config).unwrap_or(JsValue::NULL))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offset_to_line_col_counts_from_one() {
        assert_eq!(offset_to_line_col("ab\ncd", 0), (1, 1));
        assert_eq!(offset_to_line_col("ab\ncd", 3), (2, 1));
        assert_eq!(offset_to_line_col("ab\ncd", 4), (2, 2));
    }

    #[test]
    fn syntax_error_carries_span_and_line() {
        let bad = "[container\nname = 1\n";
        let err = Config::parse_with_warnings(bad).unwrap_err();
        let issues = validation_errors(&err, bad);
        assert_eq!(issues.len(), 1);
        assert!(issues[0].field.is_none());
        assert!(issues[0].line.is_some());
        assert!(issues[0].span.is_some());
    }

    #[test]
    fn semantic_error_resolves_source_line() {
        let toml = "[image]\nbase = \"fedora:44\"\nname = \"x\"\n\n[container]\nname = \"x\"\nhome = \"~/c/x\"\nmemory = \"bogus\"\n";
        let err = Config::parse_with_warnings(toml).unwrap_err();
        let issues = validation_errors(&err, toml);
        let mem = issues
            .iter()
            .find(|i| i.field.as_deref() == Some("container.memory"))
            .expect("memory issue present");
        assert_eq!(mem.line, Some(8));
    }

    #[test]
    fn field_map_covers_tables_and_keys() {
        let toml = "[container]\nname = \"x\"\n\n[network]\nmode = \"pasta\"\n";
        let map = tomlscan::field_lines(toml);
        assert_eq!(map.get("container"), Some(&1));
        assert_eq!(map.get("container.name"), Some(&2));
        assert_eq!(map.get("network.mode"), Some(&5));
    }

    #[test]
    fn compile_response_includes_companions() {
        let toml = "[image]\nbase = \"fedora:44\"\nname = \"x\"\n\n[container]\nname = \"x\"\nhome = \"~/c/x\"\n\n[integration]\ndbus = true\nwayland = true\n\n[wayland]\nfirewall = true\n\n[dbus]\nown = [\"com.example.Test\"]\n";
        let (config, _) = Config::parse_with_warnings(toml).unwrap();
        let name = &config.container.name;
        assert!(podbox::codegen::quadlet::generate_host_service(name).contains("serve"));
        assert!(podbox::codegen::quadlet::generate_dbus_proxy_service(name, &config).is_some());
        assert!(podbox::codegen::quadlet::generate_compositor_service(name, &config).is_some());
    }
}
