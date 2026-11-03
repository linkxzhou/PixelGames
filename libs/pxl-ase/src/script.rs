//! Rhai script host for `pxl-ase` (M9 prototype).
//!
//! Sandbox policy (also in `SCRIPTING.md`):
//! - Paths must stay under the configured workdir (canonical prefix check).
//! - No network modules registered.
//! - Engine `max_operations` step limit.
//! - Canvas width/height capped by `MAX_CANVAS`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use pxl_ase_doc::{AseDoc, ColorMode, ImageBuffer, RgbaImage, Sprite};
use pxl_core::Rgba;
use rhai::{Dynamic, Engine, EvalAltResult, Map, Scope, AST};

use crate::filters::replace_color_rgba;
use crate::render::flatten_frame_rgba;
use crate::{open, AseErr, Result};

/// Default max pixels on one side for script-created / opened docs.
pub const MAX_CANVAS: u32 = 4096;
/// Default Rhai operation budget.
pub const DEFAULT_MAX_OPS: u64 = 200_000;

#[derive(Clone)]
struct Host {
    workdir: PathBuf,
    allow_fs_escape: bool,
    doc: Arc<Mutex<Option<AseDoc>>>,
    max_canvas: u32,
}

impl Host {
    fn resolve_in(&self, path: &str) -> std::result::Result<PathBuf, Box<EvalAltResult>> {
        let p = PathBuf::from(path);
        let joined = if p.is_absolute() {
            p
        } else {
            self.workdir.join(p)
        };
        let canon = if joined.exists() {
            joined.canonicalize().map_err(|e| e.to_string())?
        } else {
            // For save targets: canonicalize parent + join name
            let parent = joined.parent().unwrap_or(Path::new("."));
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            let parent_c = parent
                .canonicalize()
                .map_err(|e| e.to_string())?;
            parent_c.join(joined.file_name().unwrap_or_default())
        };
        if !self.allow_fs_escape {
            let wd = self
                .workdir
                .canonicalize()
                .map_err(|e| e.to_string())?;
            if !canon.starts_with(&wd) {
                return Err(format!(
                    "script path escapes workdir: {} (workdir {})",
                    canon.display(),
                    wd.display()
                )
                .into());
            }
        }
        Ok(canon)
    }
}

#[derive(Debug, Clone)]
pub struct ScriptOptions {
    pub workdir: PathBuf,
    pub allow_fs_escape: bool,
    pub max_operations: u64,
    pub max_canvas: u32,
    pub params: Vec<(String, String)>,
}

impl Default for ScriptOptions {
    fn default() -> Self {
        Self {
            workdir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            allow_fs_escape: false,
            max_operations: DEFAULT_MAX_OPS,
            max_canvas: MAX_CANVAS,
            params: Vec::new(),
        }
    }
}

pub fn run_script_file(path: &Path, opts: &ScriptOptions) -> Result<Dynamic> {
    let src = std::fs::read_to_string(path).map_err(|e| AseErr(e.to_string()))?;
    run_script_src(&src, opts)
}

pub fn run_script_src(src: &str, opts: &ScriptOptions) -> Result<Dynamic> {
    let host = Host {
        workdir: opts.workdir.clone(),
        allow_fs_escape: opts.allow_fs_escape,
        doc: Arc::new(Mutex::new(None)),
        max_canvas: opts.max_canvas,
    };
    let mut engine = Engine::new();
    engine.set_max_operations(opts.max_operations);
    // No file/net packages beyond what we register.
    register_api(&mut engine, host.clone());

    let mut scope = Scope::new();
    let mut params = Map::new();
    for (k, v) in &opts.params {
        params.insert(k.clone().into(), Dynamic::from(v.clone()));
    }
    scope.push("params", params);

    let ast: AST = engine.compile(src).map_err(|e| AseErr(e.to_string()))?;
    engine
        .eval_ast_with_scope(&mut scope, &ast)
        .map_err(|e| AseErr(e.to_string()))
}

fn register_api(engine: &mut Engine, host: Host) {
    let h_open = host.clone();
    engine.register_fn("open", move |path: &str| -> std::result::Result<(), Box<EvalAltResult>> {
        let p = h_open.resolve_in(path)?;
        let doc = open(&p.display().to_string(), false).map_err(|e| e.0)?;
        if doc.sprite.width as u32 > h_open.max_canvas || doc.sprite.height as u32 > h_open.max_canvas
        {
            return Err(format!(
                "canvas {}x{} exceeds MAX_CANVAS {}",
                doc.sprite.width, doc.sprite.height, h_open.max_canvas
            )
            .into());
        }
        *h_open.doc.lock().unwrap() = Some(doc);
        Ok(())
    });

    let h_save = host.clone();
    engine.register_fn("save", move |path: &str| -> std::result::Result<(), Box<EvalAltResult>> {
        let p = h_save.resolve_in(path)?;
        let guard = h_save.doc.lock().unwrap();
        let doc = guard.as_ref().ok_or_else(|| "no document open".to_string())?;
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let ext = p
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        match ext.as_str() {
            "ase" | "aseprite" => {
                crate::io::write_aseprite(&p, &doc.sprite, &Default::default())
                    .map_err(|e| e.to_string())?;
            }
            "png" => {
                let png = crate::export_frame_png(&doc.sprite, 0, 1).map_err(|e| e.0)?;
                std::fs::write(&p, png).map_err(|e| e.to_string())?;
            }
            other => {
                return Err(format!("save: unsupported extension '{other}'").into());
            }
        }
        Ok(())
    });

    let h_rep = host.clone();
    engine.register_fn(
        "replace_color",
        move |from_hex: &str, to_hex: &str| -> std::result::Result<i64, Box<EvalAltResult>> {
            let from = parse_hex_rhai(from_hex)?;
            let to = parse_hex_rhai(to_hex)?;
            let mut guard = h_rep.doc.lock().unwrap();
            let doc = guard.as_mut().ok_or_else(|| "no document open".to_string())?;
            // Ensure RGBA buffers for replace_color_rgba; flatten-convert if indexed.
            ensure_rgba_images(&mut doc.sprite).map_err(|e| e)?;
            let n = replace_color_rgba(&mut doc.sprite, from, to).map_err(|e| e.0)?;
            Ok(n as i64)
        },
    );

    let h_flat = host.clone();
    engine.register_fn(
        "flatten",
        move |frame: i64| -> std::result::Result<i64, Box<EvalAltResult>> {
            let guard = h_flat.doc.lock().unwrap();
            let doc = guard.as_ref().ok_or_else(|| "no document open".to_string())?;
            let rgba = flatten_frame_rgba(&doc.sprite, frame as u16).map_err(|e| e.0)?;
            Ok((rgba.len() / 4) as i64)
        },
    );

    let h_set = host.clone();
    engine.register_fn(
        "set_pixel",
        move |frame: i64, x: i64, y: i64, hex: &str| -> std::result::Result<(), Box<EvalAltResult>> {
            let color = parse_hex_rhai(hex)?;
            let mut guard = h_set.doc.lock().unwrap();
            let doc = guard.as_mut().ok_or_else(|| "no document open".to_string())?;
            ensure_rgba_images(&mut doc.sprite).map_err(|e| e)?;
            set_pixel_rgba(&mut doc.sprite, frame as u16, x as i32, y as i32, color)
                .map_err(|e| e)?;
            Ok(())
        },
    );

    let h_info = host.clone();
    engine.register_fn("width", move || -> std::result::Result<i64, Box<EvalAltResult>> {
        let guard = h_info.doc.lock().unwrap();
        let doc = guard.as_ref().ok_or_else(|| "no document open".to_string())?;
        Ok(doc.sprite.width as i64)
    });
    let h_info2 = host.clone();
    engine.register_fn("height", move || -> std::result::Result<i64, Box<EvalAltResult>> {
        let guard = h_info2.doc.lock().unwrap();
        let doc = guard.as_ref().ok_or_else(|| "no document open".to_string())?;
        Ok(doc.sprite.height as i64)
    });
}

fn parse_hex_rhai(s: &str) -> std::result::Result<Rgba, Box<EvalAltResult>> {
    pxl_core::parse_hex(s).map_err(|e| e.message.into())
}

fn ensure_rgba_images(sprite: &mut Sprite) -> std::result::Result<(), String> {
    if sprite.color_mode == ColorMode::Rgba {
        return Ok(());
    }
    // Flatten each frame into a single RGBA layer.
    let frames = sprite.frame_count();
    let w = sprite.width;
    let h = sprite.height;
    let mut flats = Vec::with_capacity(frames);
    for f in 0..frames {
        let rgba = flatten_frame_rgba(sprite, f as u16).map_err(|e| e.0)?;
        flats.push(rgba);
    }
    sprite.layers.clear();
    sprite.images.clear();
    sprite.color_mode = ColorMode::Rgba;
    let layer = sprite.add_image_layer("Script");
    for (fi, px) in flats.into_iter().enumerate() {
        let img = RgbaImage::from_rgba(w, h, px);
        let id = sprite.insert_image(ImageBuffer::Rgba(img));
        sprite.set_cel(
            layer,
            fi as u16,
            pxl_ase_doc::Cel::image(id, 0, 0, 255),
        );
    }
    Ok(())
}

fn set_pixel_rgba(
    sprite: &mut Sprite,
    frame: u16,
    x: i32,
    y: i32,
    color: Rgba,
) -> std::result::Result<(), String> {
    use pxl_ase_doc::Cel;
    if x < 0 || y < 0 || x >= sprite.width as i32 || y >= sprite.height as i32 {
        return Err("set_pixel out of bounds".into());
    }
    let mut img_id = None;
    for layer in &sprite.layers {
        if let Some(cel) = layer.cels.get(&frame) {
            match cel {
                Cel::Image { image, .. } => {
                    img_id = Some(*image);
                    break;
                }
                Cel::Linked { frame: src, .. } => {
                    if let Some(Cel::Image { image, .. }) = layer.cels.get(src) {
                        img_id = Some(*image);
                        break;
                    }
                }
            }
        }
    }
    let Some(img_id) = img_id else {
        return Err("set_pixel: no RGBA cel for frame".into());
    };
    if let Some(ImageBuffer::Rgba(buf)) = sprite.images.get_mut(&img_id) {
        let mut px = buf.pixels.as_ref().clone();
        let i = ((y * sprite.width as i32 + x) * 4) as usize;
        if i + 3 >= px.len() {
            return Err("set_pixel: buffer short".into());
        }
        px[i] = color.r;
        px[i + 1] = color.g;
        px[i + 2] = color.b;
        px[i + 3] = color.a;
        buf.pixels = std::sync::Arc::new(px);
        return Ok(());
    }
    Err("set_pixel: no RGBA cel for frame".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pxl_ase_doc::{Cel, ColorMode, ImageBuffer, RgbaImage, Sprite};
    use pxl_ase_io::{write_aseprite, WriteOptions};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn tmp() -> PathBuf {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("pxl-ase-script-{n}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn write_rgba_fixture(path: &Path) {
        let mut s = Sprite::new(2, 2, ColorMode::Rgba);
        let layer = s.add_image_layer("a");
        let px = vec![
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
        ];
        let id = s.insert_image(ImageBuffer::Rgba(RgbaImage::from_rgba(2, 2, px)));
        s.set_cel(layer, 0, Cel::image(id, 0, 0, 255));
        write_aseprite(path, &s, &WriteOptions::default()).unwrap();
    }

    #[test]
    fn script_replace_color_and_save() {
        let dir = tmp();
        let ase = dir.join("in.aseprite");
        write_rgba_fixture(&ase);
        let out = dir.join("out.aseprite");
        let src = format!(
            r##"
            open("{}");
            replace_color("#ff0000", "#00ff00");
            save("{}");
            "##,
            ase.display(),
            out.display()
        );
        // Use forward slashes escaped for rhai strings - on unix display is fine.
        let src = src.replace('\\', "/");
        let opts = ScriptOptions {
            workdir: dir.clone(),
            ..Default::default()
        };
        run_script_src(&src, &opts).unwrap();
        assert!(out.is_file());
        let doc = open(&out.display().to_string(), false).unwrap();
        let rgba = flatten_frame_rgba(&doc.sprite, 0).unwrap();
        assert_eq!(&rgba[0..4], &[0, 255, 0, 255]);
    }

    #[test]
    fn script_rejects_path_escape() {
        let dir = tmp();
        let opts = ScriptOptions {
            workdir: dir.clone(),
            allow_fs_escape: false,
            ..Default::default()
        };
        let src = r#"
            open("/etc/passwd");
        "#;
        let err = run_script_src(src, &opts).unwrap_err();
        assert!(
            err.0.contains("escapes workdir") || err.0.contains("No such file") || err.0.contains("failed"),
            "err={}",
            err.0
        );
        // Explicit write outside
        let _src2 = r#"
            open("missing.aseprite");
        "#;
        // create a doc via failing open - instead test save escape with a valid open first
        let ase = dir.join("in.aseprite");
        write_rgba_fixture(&ase);
        let src3 = format!(
            r#"
            open("{}");
            save("/tmp/pxl-ase-should-not-write-{}.aseprite");
            "#,
            ase.display(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        );
        let src3 = src3.replace('\\', "/");
        let err = run_script_src(&src3, &opts).unwrap_err();
        assert!(err.0.contains("escapes workdir"), "err={}", err.0);
    }
}
