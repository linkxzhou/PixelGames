//! wasm-bindgen surface for the web page.
//!
//! The logic is plain Rust over `pxl-engine` (`run_render`, `run_inspect`) so it
//! can be tested natively; the `#[wasm_bindgen]` functions only convert types.
//!
//! Options travel as a JSON string and results come back as a JS object, which
//! keeps this crate free of `serde-wasm-bindgen` and `js-sys`.

use std::collections::BTreeMap;

use pxl_core::{PxlError, PXL_VERSION};
use pxl_engine::{
    DocInfo, FormatChoice, InputKind, MemoryResolver, OutputFormat, OutputPurpose, RenderRequest,
};
use pxl_format::Charset;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

/// Options the page sends. Every field is optional; the defaults match
/// `RenderRequest::default()`.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Options {
    /// `auto`, `ascii`, `ascii-json`, `shapes`, `doc`.
    pub kind: Option<String>,
    pub palette: Option<String>,
    /// `digits` or `printable`; unset lets the engine guess.
    pub charset: Option<String>,
    pub size: Option<u16>,
    pub cell: Option<String>,
    pub anim: Option<String>,
    pub dir: Option<String>,
    pub scale: Option<u32>,
    pub seed: Option<u64>,
    /// `auto`, `png`, `gif`.
    pub format: Option<String>,
    pub grid: bool,
    /// `preview` (default; backdrop, optional grid) or `asset` (transparent, no grid).
    pub purpose: Option<String>,
    /// SVG only: `strict` (default), `nearest` or `extend`.
    pub color_mode: Option<String>,
    /// SVG only: samples per pixel and axis; unset picks by size.
    pub samples: Option<u8>,
    /// SVG only: run the size tier's finishing pass (default true).
    pub finish: Option<bool>,
    /// Extra files the spec refers to (`legend_ref`, palettes), by name.
    pub files: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
pub struct AnimOut {
    pub name: String,
    pub fps: u32,
    pub frames: usize,
    #[serde(rename = "loop")]
    pub looped: bool,
    pub dirs: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct InfoOut {
    pub kind: String,
    pub width: u16,
    pub height: u16,
    pub frames: Vec<String>,
    pub anims: Vec<AnimOut>,
    pub palette: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ErrorOut {
    pub code: String,
    pub message: String,
    pub path: Option<String>,
    pub hint: Option<String>,
}

/// Result of `run_render`: metadata as JSON-ready fields plus the encoded bytes.
#[derive(Debug)]
pub struct RenderOut {
    pub format: &'static str,
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub frame_count: usize,
    pub native_width: u32,
    pub native_height: u32,
    pub scale: u32,
    pub grid: bool,
    pub purpose: &'static str,
    pub anim: Option<String>,
    pub hash: String,
    pub info: InfoOut,
}

impl From<&DocInfo> for InfoOut {
    fn from(info: &DocInfo) -> Self {
        InfoOut {
            kind: info.kind.name().to_string(),
            width: info.width,
            height: info.height,
            frames: info.frames.clone(),
            anims: info
                .anims
                .iter()
                .map(|a| AnimOut {
                    name: a.name.clone(),
                    fps: a.fps,
                    frames: a.frames,
                    looped: a.looped,
                    dirs: a.dirs.clone(),
                })
                .collect(),
            palette: info.palette.clone(),
            warnings: info.warnings.clone(),
        }
    }
}

impl From<PxlError> for ErrorOut {
    fn from(err: PxlError) -> Self {
        ErrorOut {
            code: err.code.as_str().to_string(),
            message: err.message,
            path: err.path,
            hint: err.hint,
        }
    }
}

fn usage(message: impl Into<String>) -> ErrorOut {
    PxlError::usage(message).into()
}

fn parse_options(json: &str) -> Result<Options, ErrorOut> {
    if json.trim().is_empty() {
        return Ok(Options::default());
    }
    serde_json::from_str(json).map_err(|e| usage(format!("options are not valid: {e}")))
}

fn charset(value: &Option<String>) -> Result<Option<Charset>, ErrorOut> {
    match value.as_deref() {
        None | Some("") | Some("auto") => Ok(None),
        Some(text) => Charset::parse(text).map(Some).map_err(ErrorOut::from),
    }
}

fn build_request<'a>(input: &'a str, opts: &'a Options) -> Result<RenderRequest<'a>, ErrorOut> {
    let kind = match opts.kind.as_deref() {
        None | Some("") => InputKind::Auto,
        Some(text) => InputKind::parse(text)?,
    };
    let format = match opts.format.as_deref() {
        None | Some("") | Some("auto") => FormatChoice::Auto,
        Some("png") => FormatChoice::Png,
        Some("gif") => FormatChoice::Gif,
        Some(other) => {
            return Err(usage(format!("unknown format '{other}'")));
        }
    };
    let purpose = match opts.purpose.as_deref() {
        None | Some("") | Some("preview") => OutputPurpose::Preview,
        Some("asset") => OutputPurpose::Asset,
        Some(other) => {
            return Err(PxlError::usage(format!("unknown purpose '{other}'"))
                .with_hint("purpose is preview or asset")
                .into());
        }
    };
    let defaults = RenderRequest::default();
    Ok(RenderRequest {
        input,
        kind,
        palette: opts.palette.as_deref().filter(|p| !p.is_empty()),
        charset: charset(&opts.charset)?,
        size: opts.size.unwrap_or(defaults.size),
        cell: opts.cell.as_deref().filter(|c| !c.is_empty()),
        anim: opts.anim.as_deref().filter(|a| !a.is_empty()),
        dir: opts.dir.as_deref().filter(|d| !d.is_empty()),
        scale: opts.scale.unwrap_or(defaults.scale),
        seed: opts.seed.unwrap_or(defaults.seed),
        format,
        grid: opts.grid,
        color_mode: opts.color_mode.as_deref().filter(|m| !m.is_empty()),
        samples: opts.samples,
        finish: opts.finish.unwrap_or(defaults.finish),
        purpose,
    })
}

fn resolver(opts: &Options) -> MemoryResolver {
    MemoryResolver::with_files(opts.files.clone())
}

/// Render `input` with options given as JSON. Pure Rust; the wasm export wraps it.
pub fn run_render(input: &str, options_json: &str) -> Result<RenderOut, ErrorOut> {
    let opts = parse_options(options_json)?;
    let req = build_request(input, &opts)?;
    let out = pxl_engine::render(&req, &resolver(&opts))?;
    Ok(RenderOut {
        format: match out.format {
            OutputFormat::Png => "png",
            OutputFormat::Gif => "gif",
        },
        bytes: out.bytes,
        width: out.width,
        height: out.height,
        frame_count: out.frame_count,
        native_width: out.native_width,
        native_height: out.native_height,
        scale: out.scale,
        grid: out.grid,
        purpose: out.purpose.name(),
        anim: out.anim,
        hash: out.hash,
        info: InfoOut::from(&out.info),
    })
}

/// Parse and assemble without encoding; used to fill the animation drop-down.
pub fn run_inspect(input: &str, options_json: &str) -> Result<InfoOut, ErrorOut> {
    let opts = parse_options(options_json)?;
    let req = build_request(input, &opts)?;
    let info = pxl_engine::inspect(&req, &resolver(&opts))?;
    Ok(InfoOut::from(&info))
}

// ---- wasm exports -------------------------------------------------------

fn to_js<T: Serialize>(value: &T) -> JsValue {
    // Serialising these plain structs cannot fail; fall back to `null` rather
    // than panic across the wasm boundary.
    match serde_json::to_string(value) {
        Ok(text) => js_json_parse(&text),
        Err(_) => JsValue::NULL,
    }
}

#[wasm_bindgen(inline_js = "export function parse_json(text) { return JSON.parse(text); }")]
extern "C" {
    #[wasm_bindgen(js_name = parse_json)]
    fn js_json_parse(text: &str) -> JsValue;
}

fn throw(err: ErrorOut) -> JsValue {
    to_js(&err)
}

/// Result object handed to JS.
#[wasm_bindgen]
pub struct RenderResult {
    format: String,
    bytes: Vec<u8>,
    meta_json: String,
}

#[wasm_bindgen]
impl RenderResult {
    /// `"png"` or `"gif"`.
    #[wasm_bindgen(getter)]
    pub fn format(&self) -> String {
        self.format.clone()
    }

    /// Encoded image bytes.
    #[wasm_bindgen(getter)]
    pub fn bytes(&self) -> Vec<u8> {
        self.bytes.clone()
    }

    /// `{ width, height, frameCount, nativeWidth, nativeHeight, scale, sampling, grid,
    /// purpose, anim, hash, info }` as JSON text.
    #[wasm_bindgen(getter, js_name = metaJson)]
    pub fn meta_json(&self) -> String {
        self.meta_json.clone()
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Meta<'a> {
    width: u32,
    height: u32,
    frame_count: usize,
    native_width: u32,
    native_height: u32,
    scale: u32,
    sampling: &'static str,
    grid: bool,
    purpose: &'static str,
    anim: &'a Option<String>,
    hash: &'a str,
    info: &'a InfoOut,
}

/// Render. On failure throws an object `{ code, message, path, hint }`.
#[wasm_bindgen]
pub fn render(input: &str, options_json: &str) -> Result<RenderResult, JsValue> {
    let out = run_render(input, options_json).map_err(throw)?;
    let meta = Meta {
        width: out.width,
        height: out.height,
        frame_count: out.frame_count,
        native_width: out.native_width,
        native_height: out.native_height,
        scale: out.scale,
        sampling: "nearest",
        grid: out.grid,
        purpose: out.purpose,
        anim: &out.anim,
        hash: &out.hash,
        info: &out.info,
    };
    let meta_json = serde_json::to_string(&meta)
        .map_err(|e| throw(usage(format!("cannot encode result: {e}"))))?;
    Ok(RenderResult {
        format: out.format.to_string(),
        bytes: out.bytes,
        meta_json,
    })
}

/// Parse only. Returns `{ kind, width, height, frames, anims, palette, warnings }`.
#[wasm_bindgen]
pub fn inspect(input: &str, options_json: &str) -> Result<JsValue, JsValue> {
    let info = run_inspect(input, options_json).map_err(throw)?;
    Ok(to_js(&info))
}

#[wasm_bindgen]
pub fn version() -> String {
    PXL_VERSION.to_string()
}

/// Largest accepted input in bytes, so the page can warn before sending.
#[wasm_bindgen(js_name = maxInputBytes)]
pub fn max_input_bytes() -> usize {
    pxl_engine::MAX_INPUT_BYTES
}

#[cfg(test)]
mod tests {
    use super::*;

    const GEM: &str = "..##..\n.#$$#.\n#$$$$#\n#$$$$#\n.#$$#.\n..##..\n";

    #[test]
    fn renders_png_from_ascii_with_json_options() {
        let out = run_render(GEM, r#"{"palette":"p16","scale":4}"#).unwrap();
        assert_eq!(out.format, "png");
        assert!(out.bytes.starts_with(b"\x89PNG"));
        assert_eq!((out.width, out.height), (128, 128));
        assert_eq!(out.info.kind, "ascii");
    }

    #[test]
    fn empty_options_use_defaults() {
        let out = run_render(GEM, "").unwrap();
        assert_eq!(out.format, "png");
        assert_eq!(out.info.palette, "p16");
        assert_eq!((out.width, out.height), (32 * 8, 32 * 8));
    }

    #[test]
    fn errors_carry_code_message_and_hint() {
        let err = run_render("", r#"{"palette":"p16"}"#).err().unwrap();
        assert_eq!(err.code, "E_USAGE");
        assert!(err.message.contains("empty"));
        let err = run_render(GEM, r#"{"palette":"p16","scale":99}"#)
            .err()
            .unwrap();
        assert!(err.message.contains("scale"));
    }

    #[test]
    fn unknown_option_keys_are_rejected() {
        let err = run_render(GEM, r#"{"palete":"p16"}"#).err().unwrap();
        assert_eq!(err.code, "E_USAGE");
        assert!(err.message.contains("palete"), "{}", err.message);
    }

    #[test]
    fn bad_format_and_kind_are_usage_errors() {
        let err = run_render(GEM, r#"{"palette":"p16","format":"webp"}"#)
            .err()
            .unwrap();
        assert!(err.message.contains("webp"));
        let err = run_render(GEM, r#"{"palette":"p16","kind":"zip"}"#)
            .err()
            .unwrap();
        assert!(err.message.contains("zip"));
    }

    #[test]
    fn files_table_feeds_legend_ref() {
        let spec = include_str!("../../../examples/ascii/slime/slime.ascii.json");
        let legend = include_str!("../../../examples/ascii/shared/p16.legend.json");
        let opts = serde_json::json!({
            "scale": 4,
            "files": { "p16.legend.json": legend }
        })
        .to_string();
        let out = run_render(spec, &opts).unwrap();
        assert_eq!(out.format, "gif");
        assert_eq!(out.anim.as_deref(), Some("idle"));
        assert_eq!(out.info.anims.len(), 2);
        let info = run_inspect(spec, &opts).unwrap();
        assert_eq!(
            info.anims
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>(),
            ["hop", "idle"]
        );
    }

    #[test]
    fn version_is_the_pxl_version() {
        assert_eq!(version(), PXL_VERSION);
    }

    #[test]
    fn render_reports_native_size_and_purpose() {
        let out = run_render(GEM, r#"{"palette":"p16","scale":4}"#).unwrap();
        assert_eq!((out.native_width, out.native_height), (32, 32));
        assert_eq!((out.scale, out.purpose, out.grid), (4, "preview", false));
        let out = run_render(GEM, r#"{"palette":"p16","scale":4,"purpose":"asset"}"#).unwrap();
        assert_eq!(out.purpose, "asset");
        let err = run_render(GEM, r#"{"palette":"p16","purpose":"poster"}"#)
            .err()
            .unwrap();
        assert!(err.message.contains("poster"), "{}", err.message);
        let err = run_render(GEM, r#"{"palette":"p16","purpose":"asset","grid":true}"#)
            .err()
            .unwrap();
        assert!(err.message.contains("grid"), "{}", err.message);
    }
}
