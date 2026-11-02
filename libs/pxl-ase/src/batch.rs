//! Minimal left-to-right Aseprite-like argv batch parser (M7 subset).
//!
//! Covered grammar (not full official parity):
//! ```text
//! ase run -- [-b|--batch] <file.aseprite>
//!           [--tag NAME] [--play-subtags] [--frame-range SPEC]
//!           [--trim | --trim-by-grid]
//!           [--save-as PATH]
//!           [--sheet PATH [--data PATH] [--layout NAME]]
//!           [--filter NAME [--from-color HEX] [--to-color HEX]]
//!           [--op trim|flip ...]
//! ```
//! Tokens are applied left-to-right. Opening a `.aseprite`/`.ase` loads the
//! document. `--save-as` / `--sheet` export the current document with flags
//! collected so far. Unknown flags error (no silent swallow).

use std::path::{Path, PathBuf};

use pxl_ase_doc::AseDoc;

use crate::filters::replace_color_rgba;
use crate::ops::{flip_all_frames, trim_sprite, trim_sprite_by_grid};
use crate::{
    export_frame_png, export_gif, export_sheet_png_json_layout_opts, open, select_frames_opts,
    AseErr, Result, SheetExportOpts,
};
use pxl_core::{parse_hex, Rgba};

#[derive(Debug, Clone, Default)]
pub struct BatchState {
    pub doc: Option<AseDoc>,
    pub tag: Option<String>,
    pub frame_range: Option<String>,
    pub play_subtags: bool,
    pub trim: bool,
    pub trim_by_grid: bool,
    pub written: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum BatchEvent {
    Opened(PathBuf),
    SavedAs(PathBuf),
    Sheet { out: PathBuf, data: PathBuf },
    Filtered { name: String },
    Op { name: String },
}

/// Parse and execute a sequential argv slice (no program name).
pub fn run_batch_argv(args: &[String], workdir: Option<&Path>) -> Result<(BatchState, Vec<BatchEvent>)> {
    let mut st = BatchState::default();
    let mut events = Vec::new();
    let mut i = 0usize;
    while i < args.len() {
        let a = &args[i];
        if a == "--" {
            i += 1;
            continue;
        }
        if a == "-b" || a == "--batch" || a == "--preview" || a == "-p" {
            // Compatible no-ops / dry note: preview still executes for now only as ignore.
            i += 1;
            continue;
        }
        if a == "--tag" {
            i += 1;
            let v = args.get(i).ok_or_else(|| AseErr("--tag needs a value".into()))?;
            st.tag = Some(v.clone());
            i += 1;
            continue;
        }
        if a == "--frame-range" {
            i += 1;
            let v = args
                .get(i)
                .ok_or_else(|| AseErr("--frame-range needs a value".into()))?;
            st.frame_range = Some(v.clone());
            i += 1;
            continue;
        }
        if a == "--play-subtags" {
            st.play_subtags = true;
            i += 1;
            continue;
        }
        if a == "--trim" {
            st.trim = true;
            i += 1;
            continue;
        }
        if a == "--trim-by-grid" {
            st.trim_by_grid = true;
            i += 1;
            continue;
        }
        if a == "--save-as" {
            i += 1;
            let path = args
                .get(i)
                .ok_or_else(|| AseErr("--save-as needs a path".into()))?;
            let out = resolve_path(path, workdir);
            apply_prep(&mut st)?;
            save_current(&mut st, &out)?;
            events.push(BatchEvent::SavedAs(out.clone()));
            st.written.push(out.display().to_string());
            i += 1;
            continue;
        }
        if a == "--sheet" {
            i += 1;
            let path = args
                .get(i)
                .ok_or_else(|| AseErr("--sheet needs a path".into()))?;
            let out = resolve_path(path, workdir);
            i += 1;
            let mut data = out.clone();
            data.set_extension("json");
            let mut layout = "horizontal".to_string();
            while i < args.len() {
                if args[i] == "--data" {
                    i += 1;
                    let d = args
                        .get(i)
                        .ok_or_else(|| AseErr("--data needs a path".into()))?;
                    data = resolve_path(d, workdir);
                    i += 1;
                    continue;
                }
                if args[i] == "--layout" {
                    i += 1;
                    layout = args
                        .get(i)
                        .ok_or_else(|| AseErr("--layout needs a value".into()))?
                        .clone();
                    i += 1;
                    continue;
                }
                break;
            }
            apply_prep(&mut st)?;
            sheet_current(&mut st, &out, &data, &layout)?;
            events.push(BatchEvent::Sheet {
                out: out.clone(),
                data: data.clone(),
            });
            st.written.push(out.display().to_string());
            st.written.push(data.display().to_string());
            continue;
        }
        if a == "--filter" {
            i += 1;
            let name = args
                .get(i)
                .ok_or_else(|| AseErr("--filter needs a name".into()))?
                .clone();
            i += 1;
            let mut from_c: Option<Rgba> = None;
            let mut to_c: Option<Rgba> = None;
            while i < args.len() {
                if args[i] == "--from-color" {
                    i += 1;
                    let v = args
                        .get(i)
                        .ok_or_else(|| AseErr("--from-color needs a value".into()))?;
                    from_c = Some(parse_hex(v).map_err(|e| AseErr(e.message))?);
                    i += 1;
                    continue;
                }
                if args[i] == "--to-color" {
                    i += 1;
                    let v = args
                        .get(i)
                        .ok_or_else(|| AseErr("--to-color needs a value".into()))?;
                    to_c = Some(parse_hex(v).map_err(|e| AseErr(e.message))?);
                    i += 1;
                    continue;
                }
                break;
            }
            let doc = st
                .doc
                .as_mut()
                .ok_or_else(|| AseErr("--filter before opening a document".into()))?;
            match name.as_str() {
                "replace-color" => {
                    let from = from_c.ok_or_else(|| AseErr("--filter replace-color needs --from-color".into()))?;
                    let to = to_c.ok_or_else(|| AseErr("--filter replace-color needs --to-color".into()))?;
                    replace_color_rgba(&mut doc.sprite, from, to)?;
                }
                "invert" => {
                    crate::filters::invert(&mut doc.sprite)?;
                }
                other => {
                    return Err(AseErr(format!(
                        "batch --filter '{other}' not in minimal set (replace-color|invert)"
                    )));
                }
            }
            events.push(BatchEvent::Filtered { name });
            continue;
        }
        if a == "--op" {
            i += 1;
            let name = args
                .get(i)
                .ok_or_else(|| AseErr("--op needs a name".into()))?
                .clone();
            i += 1;
            let mut axis = "h".to_string();
            while i < args.len() {
                if args[i] == "--axis" {
                    i += 1;
                    axis = args
                        .get(i)
                        .ok_or_else(|| AseErr("--axis needs a value".into()))?
                        .clone();
                    i += 1;
                    continue;
                }
                break;
            }
            let doc = st
                .doc
                .as_mut()
                .ok_or_else(|| AseErr("--op before opening a document".into()))?;
            match name.as_str() {
                "trim" => {
                    trim_sprite(&mut doc.sprite)?;
                }
                "flip" => {
                    flip_all_frames(&mut doc.sprite, &axis)?;
                }
                other => {
                    return Err(AseErr(format!(
                        "batch --op '{other}' not in minimal set (trim|flip)"
                    )));
                }
            }
            events.push(BatchEvent::Op { name });
            continue;
        }
        if a.starts_with('-') {
            return Err(AseErr(format!(
                "unknown batch flag '{a}' (minimal AseBatchParser; see plan/v8.0/cli-flag-matrix.md)"
            )));
        }
        // Positional: open document
        let path = resolve_path(a, workdir);
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ext != "aseprite" && ext != "ase" {
            return Err(AseErr(format!(
                "expected .aseprite to open, got '{}'",
                path.display()
            )));
        }
        st.doc = Some(open(&path.display().to_string(), false)?);
        events.push(BatchEvent::Opened(path));
        i += 1;
    }
    Ok((st, events))
}

fn resolve_path(p: &str, workdir: Option<&Path>) -> PathBuf {
    let path = PathBuf::from(p);
    if path.is_absolute() {
        path
    } else if let Some(wd) = workdir {
        wd.join(path)
    } else {
        path
    }
}

fn apply_prep(st: &mut BatchState) -> Result<()> {
    let doc = st
        .doc
        .as_mut()
        .ok_or_else(|| AseErr("no document open before export".into()))?;
    if st.trim_by_grid {
        trim_sprite_by_grid(&mut doc.sprite)?;
    } else if st.trim {
        trim_sprite(&mut doc.sprite)?;
    }
    Ok(())
}

fn save_current(st: &mut BatchState, out: &Path) -> Result<()> {
    let doc = st
        .doc
        .as_ref()
        .ok_or_else(|| AseErr("no document open".into()))?;
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| AseErr(e.to_string()))?;
        }
    }
    let ext = out
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let frames = select_frames_opts(
        &doc.sprite,
        st.tag.as_deref(),
        st.frame_range.as_deref(),
        None,
        st.play_subtags,
    )?;
    match ext.as_str() {
        "ase" | "aseprite" => {
            crate::io::write_aseprite(out, &doc.sprite, &Default::default())?;
        }
        "gif" => {
            let bytes = export_gif(&doc.sprite, &frames, 1)?;
            std::fs::write(out, bytes).map_err(|e| AseErr(e.to_string()))?;
        }
        _ => {
            let f = frames.first().copied().unwrap_or(0);
            let png = export_frame_png(&doc.sprite, f, 1)?;
            std::fs::write(out, png).map_err(|e| AseErr(e.to_string()))?;
        }
    }
    Ok(())
}

fn sheet_current(st: &mut BatchState, out: &Path, data: &Path, layout: &str) -> Result<()> {
    let doc = st
        .doc
        .as_ref()
        .ok_or_else(|| AseErr("no document open".into()))?;
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| AseErr(e.to_string()))?;
        }
    }
    let frames = select_frames_opts(
        &doc.sprite,
        st.tag.as_deref(),
        st.frame_range.as_deref(),
        None,
        st.play_subtags,
    )?;
    let image_name = out
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("sheet.png");
    let opts = SheetExportOpts {
        trim: false, // already applied via apply_prep on document when --trim
        ..Default::default()
    };
    let (png, js) =
        export_sheet_png_json_layout_opts(&doc.sprite, &frames, image_name, layout, &opts)?;
    std::fs::write(out, png).map_err(|e| AseErr(e.to_string()))?;
    if let Some(parent) = data.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| AseErr(e.to_string()))?;
        }
    }
    std::fs::write(data, js).map_err(|e| AseErr(e.to_string()))?;
    Ok(())
}

/// Thin alias used by tests / docs.
pub struct AseBatchParser;

impl AseBatchParser {
    pub fn run(args: &[String]) -> Result<(BatchState, Vec<BatchEvent>)> {
        run_batch_argv(args, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pxl_ase_doc::{Cel, ColorMode, ImageBuffer, IndexedImage, Sprite, Tag, TagDirection};
    use pxl_ase_io::{write_aseprite, WriteOptions};
    use pxl_core::Rgba;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn tmp_dir() -> PathBuf {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("pxl-ase-batch-{n}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn write_mini(path: &Path) {
        let mut s = Sprite::new(4, 4, ColorMode::Indexed);
        s.palette = vec![Rgba::TRANSPARENT, Rgba::rgb(255, 0, 0), Rgba::rgb(0, 0, 255)];
        s.frame_durations_ms = vec![100, 100];
        let layer = s.add_image_layer("a");
        let img = IndexedImage::from_indices(4, 4, vec![0, 1, 1, 0, 1, 2, 2, 1, 1, 2, 2, 1, 0, 1, 1, 0]);
        let id = s.insert_image(ImageBuffer::Indexed(img));
        s.set_cel(layer, 0, Cel::image(id, 0, 0, 255));
        s.set_cel(layer, 1, Cel::linked(0));
        s.tags.push(Tag {
            name: "idle".into(),
            from: 0,
            to: 1,
            direction: TagDirection::Forward,
            repeat: 0,
        });
        write_aseprite(path, &s, &WriteOptions::default()).unwrap();
    }

    #[test]
    fn batch_open_trim_save_as() {
        let dir = tmp_dir();
        let ase = dir.join("mini.aseprite");
        write_mini(&ase);
        let out = dir.join("out.png");
        let args = vec![
            ase.display().to_string(),
            "--tag".into(),
            "idle".into(),
            "--trim".into(),
            "--save-as".into(),
            out.display().to_string(),
        ];
        let (st, events) = AseBatchParser::run(&args).unwrap();
        assert!(out.is_file());
        assert!(st.written.iter().any(|w| w.contains("out.png")));
        assert!(events.iter().any(|e| matches!(e, BatchEvent::SavedAs(_))));
    }

    #[test]
    fn batch_sheet_and_unknown_flag() {
        let dir = tmp_dir();
        let ase = dir.join("mini.aseprite");
        write_mini(&ase);
        let sheet = dir.join("s.png");
        let args = vec![
            "--batch".into(),
            ase.display().to_string(),
            "--sheet".into(),
            sheet.display().to_string(),
            "--layout".into(),
            "horizontal".into(),
        ];
        let (_st, events) = AseBatchParser::run(&args).unwrap();
        assert!(sheet.is_file());
        assert!(events.iter().any(|e| matches!(e, BatchEvent::Sheet { .. })));
        let err = AseBatchParser::run(&[ase.display().to_string(), "--nope".into()]).unwrap_err();
        assert!(err.0.contains("unknown batch flag"));
    }
}
