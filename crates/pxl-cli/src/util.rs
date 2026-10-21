use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use pxl_core::{sha256_hex, IndexFrame, Palette, PxlError, PxlResult, PXL_VERSION};
use pxl_format::{parse_palette, to_pretty, PxlDoc};
use pxl_gen::{builtin_palette, load_builtin_mask, RuntimeMask};
use serde_json::{Map, Value};

use crate::error::{CmdError, CmdResult};

#[derive(Clone, Debug)]
pub struct Ctx {
    pub json: bool,
    pub seed: Option<u64>,
    pub quiet: bool,
    pub strict: bool,
    pub lock: Option<String>,
    pub overwrite: bool,
}

pub fn emit_err(err: &CmdError, json: bool) {
    if json {
        let mut error = Map::new();
        error.insert("code".into(), Value::String(err.err.code.as_str().into()));
        error.insert("message".into(), Value::String(err.err.message.clone()));
        if let Some(path) = &err.err.path {
            error.insert("path".into(), Value::String(path.clone()));
        }
        if let Some(hint) = &err.err.hint {
            error.insert("hint".into(), Value::String(hint.clone()));
        }
        let mut root = Map::new();
        root.insert("ok".into(), Value::Bool(false));
        root.insert("error".into(), Value::Object(error));
        if let Some(extra) = &err.extra {
            if let Some(obj) = extra.as_object() {
                for (k, v) in obj {
                    root.insert(k.clone(), v.clone());
                }
            }
        }
        println!(
            "{}",
            serde_json::to_string(&Value::Object(root)).unwrap_or_else(|_| "{}".into())
        );
    } else {
        eprintln!("error [{}]: {}", err.err.code.as_str(), err.err.message);
        if let Some(path) = &err.err.path {
            eprintln!("path: {path}");
        }
        if let Some(hint) = &err.err.hint {
            eprintln!("hint: {hint}");
        }
    }
}

pub fn emit_ok(ctx: &Ctx, value: &Value) {
    if ctx.json {
        println!(
            "{}",
            serde_json::to_string(value).unwrap_or_else(|_| "{}".into())
        );
    } else if !ctx.quiet {
        let cmd = value.get("cmd").and_then(|v| v.as_str()).unwrap_or("pxl");
        let out = value.get("out").and_then(|v| v.as_str()).unwrap_or("");
        if out.is_empty() {
            println!("{cmd}");
        } else {
            println!("{cmd} {out}");
        }
    }
}

pub fn guard_warnings(ctx: &Ctx, warnings: &[String]) -> CmdResult<()> {
    if ctx.strict && !warnings.is_empty() {
        return Err(PxlError::semantic(format!(
            "{} warning(s) treated as errors because --strict is set",
            warnings.len()
        ))
        .with_hint(&warnings.join("; "))
        .into());
    }
    Ok(())
}

pub fn check_lock(ctx: &Ctx, version: Option<&str>) -> CmdResult<()> {
    let Some(lock) = &ctx.lock else {
        return Ok(());
    };
    if !version_compatible(lock, PXL_VERSION) {
        return Err(PxlError::semantic(format!(
            "pxl {PXL_VERSION} does not match --pxl-version-lock {lock}"
        ))
        .with_hint("use a lock that is a prefix of 0.1.0, or omit the flag")
        .into());
    }
    if let Some(v) = version {
        if !version_compatible(lock, v) {
            return Err(PxlError::semantic(format!(
                "document pxl_version {v} does not match --pxl-version-lock {lock}"
            ))
            .into());
        }
    }
    Ok(())
}

fn version_compatible(lock: &str, version: &str) -> bool {
    version == lock
        || version.starts_with(&format!("{lock}."))
        || lock.starts_with(&format!("{version}."))
}

pub fn read_text(path: &str) -> CmdResult<String> {
    if path == "-" {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| PxlError::io(format!("stdin: {e}")))?;
        return Ok(buf);
    }
    fs::read_to_string(path).map_err(|e| {
        PxlError::io(format!("cannot read {path}: {e}"))
            .with_hint("check the path, or pass - to read stdin")
            .into()
    })
}

pub fn hash_of(bytes: &[u8]) -> String {
    format!("sha256:{}", sha256_hex(bytes))
}

pub fn write_out(
    path: &str,
    bytes: &[u8],
    overwrite: bool,
    json_mode: bool,
) -> CmdResult<Option<String>> {
    if path == "-" {
        if json_mode {
            return Err(PxlError::usage(
                "refusing to write binary or document bytes to stdout together with --json",
            )
            .with_hint("pass a file path to -o when using --json")
            .into());
        }
        let mut out = std::io::stdout().lock();
        out.write_all(bytes)
            .map_err(|e| PxlError::io(format!("stdout: {e}")))?;
        return Ok(None);
    }
    let p = Path::new(path);
    if p.exists() && !overwrite {
        return Err(PxlError::exists(path).into());
    }
    if let Some(parent) = p.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| {
                CmdError::from(PxlError::io(format!(
                    "cannot create {}: {e}",
                    parent.display()
                )))
            })?;
        }
    }
    fs::write(p, bytes)
        .map_err(|e| CmdError::from(PxlError::io(format!("cannot write {path}: {e}"))))?;
    Ok(Some(path.to_string()))
}

pub fn load_palette(path_or_id: &str, base: &Path) -> CmdResult<Palette> {
    if !path_or_id.contains('/') && !path_or_id.ends_with(".json") {
        if let Ok(p) = builtin_palette(path_or_id) {
            return Ok(p);
        }
    }
    let candidates = [
        base.join(path_or_id),
        PathBuf::from(path_or_id),
        base.join(Path::new(path_or_id).file_name().unwrap_or_default()),
    ];
    for c in &candidates {
        if c.is_file() {
            let text =
                fs::read_to_string(c).map_err(|e| PxlError::io(format!("{}: {e}", c.display())))?;
            let id = c.file_stem().and_then(|s| s.to_str()).unwrap_or("palette");
            return parse_palette(&text, id).map_err(Into::into);
        }
    }
    if let Ok(p) = builtin_palette(path_or_id) {
        return Ok(p);
    }
    Err(
        PxlError::semantic(format!("palette '{path_or_id}' was not found"))
            .with_hint("pass a .palette.json path or a built-in id such as tank or p16")
            .into(),
    )
}

pub fn load_doc(path: &str) -> CmdResult<(PxlDoc, PathBuf)> {
    let text = read_text(path)?;
    let doc: PxlDoc = pxl_format::parse_json(&text, "pxl document")?;
    let full = if path == "-" {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    } else {
        PathBuf::from(path)
    };
    Ok((doc, full))
}

pub fn doc_palette(doc: &PxlDoc, doc_path: &Path) -> CmdResult<Palette> {
    let base = doc_path.parent().unwrap_or_else(|| Path::new("."));
    load_palette(&doc.palette, base)
}

pub fn save_doc(doc: &PxlDoc, path: &str, overwrite: bool, json_mode: bool) -> CmdResult<String> {
    let text = to_pretty(doc)?;
    let bytes = text.into_bytes();
    let hash = hash_of(&bytes);
    write_out(path, &bytes, overwrite, json_mode)?;
    Ok(hash)
}

pub fn load_mask(name: &str, base: &Path) -> PxlResult<RuntimeMask> {
    let candidates = [base.join(name), PathBuf::from(name)];
    for c in &candidates {
        if c.is_file() {
            let text = fs::read_to_string(c)
                .map_err(|e| PxlError::io(format!("cannot read template {}: {e}", c.display())))?;
            let file = pxl_format::parse_template(&text)?;
            return RuntimeMask::from_file(&file, None);
        }
    }
    load_builtin_mask(name)
}

pub fn frames_of(doc: &PxlDoc) -> CmdResult<Vec<(String, IndexFrame)>> {
    let mut out = Vec::new();
    for name in doc.frame_names() {
        out.push((name.clone(), doc.frame(&name)?));
    }
    Ok(out)
}

pub fn colors_used(frames: &[IndexFrame]) -> usize {
    let mut seen = [false; 256];
    for frame in frames {
        for px in &frame.pixels {
            seen[*px as usize] = true;
        }
    }
    seen.iter().filter(|s| **s).count()
}

pub fn ok_base(cmd: &str) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("ok".into(), Value::Bool(true));
    m.insert("cmd".into(), Value::String(cmd.into()));
    m.insert("warnings".into(), Value::Array(Vec::new()));
    m
}

pub fn stem_id(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("asset")
        .trim_end_matches(".pxl")
        .trim_end_matches(".sheet")
        .to_string()
}
