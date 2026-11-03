//! Open → flatten frame 0 → save PNG. Compiles as `pxl-ase` example (M9.0).
use std::env;
use std::path::PathBuf;

fn main() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let input = args.next().ok_or_else(|| "usage: open_save_flatten <in.aseprite> <out.png>".to_string())?;
    let output = args.next().ok_or_else(|| "usage: open_save_flatten <in.aseprite> <out.png>".to_string())?;
    let doc = pxl_ase::open(&input, false).map_err(|e| e.to_string())?;
    let png = pxl_ase::export_frame_png(&doc.sprite, 0, 1).map_err(|e| e.to_string())?;
    let out = PathBuf::from(output);
    if let Some(p) = out.parent() {
        std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    std::fs::write(&out, png).map_err(|e| e.to_string())?;
    let _flat = pxl_ase::render::flatten_frame_rgba(&doc.sprite, 0).map_err(|e| e.to_string())?;
    Ok(())
}
