//! Open → replace_color → save. Compiles as `pxl-ase` example (M9.0).
use std::env;
use std::path::PathBuf;
use pxl_core::parse_hex;

fn main() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let input = args.next().ok_or_else(|| "usage: apply_replace_color <in> <out> [from] [to]".to_string())?;
    let output = args.next().ok_or_else(|| "missing out".to_string())?;
    let from = args.next().unwrap_or_else(|| "#ff0000".into());
    let to = args.next().unwrap_or_else(|| "#00ff00".into());
    let mut doc = pxl_ase::open(&input, false).map_err(|e| e.to_string())?;
    let frames = doc.sprite.frame_count();
    let w = doc.sprite.width;
    let h = doc.sprite.height;
    let mut flats = Vec::new();
    for f in 0..frames {
        flats.push(pxl_ase::render::flatten_frame_rgba(&doc.sprite, f as u16).map_err(|e| e.to_string())?);
    }
    doc.sprite.layers.clear();
    doc.sprite.images.clear();
    doc.sprite.color_mode = pxl_ase::doc::ColorMode::Rgba;
    let layer = doc.sprite.add_image_layer("Flat");
    for (fi, px) in flats.into_iter().enumerate() {
        let img = pxl_ase::doc::RgbaImage::from_rgba(w, h, px);
        let id = doc.sprite.insert_image(pxl_ase::doc::ImageBuffer::Rgba(img));
        doc.sprite.set_cel(layer, fi as u16, pxl_ase::doc::Cel::image(id, 0, 0, 255));
    }
    let n = pxl_ase::filters::replace_color_rgba(
        &mut doc.sprite,
        parse_hex(&from).map_err(|e| e.message)?,
        parse_hex(&to).map_err(|e| e.message)?,
    )
    .map_err(|e| e.to_string())?;
    eprintln!("replaced {n} pixels");
    let out = PathBuf::from(output);
    if let Some(p) = out.parent() {
        std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    pxl_ase::io::write_aseprite(&out, &doc.sprite, &Default::default()).map_err(|e| e.to_string())?;
    Ok(())
}
