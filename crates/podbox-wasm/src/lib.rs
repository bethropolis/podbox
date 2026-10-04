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
use podbox::env::HostEnv;
use podbox::error::PodboxError;
use podbox::xdg::ResolvedXdgDirs;

#[derive(Serialize)]
pub struct ValidationError {
    /// Dotted schema path (`container.memory`), when the issue is a schema
    /// violation of the form `<field>: <reason>`. `None` for TOML syntax
    /// errors, which carry no field target.
    pub field: Option<String>,
    pub message: String,
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
    /// Rendered Containerfile, or `None` when it cannot be produced here.
    /// The engine always emits the short overlay for prebuilt images; custom
    /// `base` images need the guest binary baked in at build time, which the
    /// wasm build does not embed (`PODBOX_GUEST` is `None`), so those yield
    /// `None` and the Studio renders an explanatory comment instead.
    pub containerfile: Option<String>,
    pub warnings: Vec<String>,
}

/// Split an `anyhow` parse error into Studio-ready issues.
///
/// Schema violations arrive as `ConfigValidationFailed { details }` with
/// items joined by `"\n  - "`, each shaped `<field>: <reason>`; the field
/// target is split off for input highlighting. Everything else (TOML syntax
/// errors, missing fields) is a single field-less message.
fn validation_errors(err: &anyhow::Error) -> Vec<ValidationError> {
    if let Some(PodboxError::ConfigValidationFailed { details }) = err.downcast_ref::<PodboxError>()
    {
        details
            .split("\n  - ")
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|line| match line.split_once(':') {
                Some((field, message)) => ValidationError {
                    field: Some(field.trim().to_string()),
                    message: message.trim().to_string(),
                },
                None => ValidationError {
                    field: None,
                    message: line.to_string(),
                },
            })
            .collect()
    } else {
        vec![ValidationError {
            field: None,
            // `{:#}` renders the full anyhow chain ("failed to parse definition
            // file: TOML parse error at line 1, column 9"); `{}` would show only
            // the outermost context.
            message: format!("{err:#}"),
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
            errors: validation_errors(&err),
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

    let response = CompileResponse {
        container,
        socket,
        build,
        containerfile: podbox::codegen::containerfile::generate(&config, "podbox-guest").ok(),
        warnings,
    };
    Ok(serde_wasm_bindgen::to_value(&response).unwrap_or(JsValue::NULL))
}

/// Parse a TOML definition into a plain JS object for Studio state import.
#[wasm_bindgen]
pub fn parse_toml_to_json(toml_str: &str) -> Result<JsValue, JsError> {
    let config = Config::parse(toml_str).map_err(|e| JsError::new(&format!("{e:#}")))?;
    Ok(serde_wasm_bindgen::to_value(&config).unwrap_or(JsValue::NULL))
}
