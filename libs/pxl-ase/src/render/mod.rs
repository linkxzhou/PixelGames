//! Layer flatten / composite (merged `pxl-ase-render`).

mod blend;

use pxl_ase_doc::{BlendMode, CelPixels, ColorMode, Layer, LayerKind, Sprite};
use pxl_core::Rgba;

use crate::{AseErr, Result};

/// Composite visible layers for one frame into RGBA8.
///
/// Paint order: NOTE.5 z-index among siblings. When the sprite has group layers
/// (or header flag bit2), each group is composited into a temporary buffer then
/// blended onto the parent with the group's blend/opacity.
pub fn flatten_frame_rgba(sprite: &Sprite, frame: u16) -> Result<Vec<u8>> {
    let w = sprite.width as usize;
    let h = sprite.height as usize;
    let use_groups = sprite.header_flags & 2 != 0
        || sprite.layers.iter().any(|l| l.kind == LayerKind::Group);
    if use_groups {
        let mut out = vec![0u8; w * h * 4];
        composite_range(sprite, frame, 0, sprite.layers.len(), &mut out, w, h)?;
        return Ok(out);
    }
    flatten_flat(sprite, frame, w, h)
}

fn flatten_flat(sprite: &Sprite, frame: u16, w: usize, h: usize) -> Result<Vec<u8>> {
    let mut out = vec![0u8; w * h * 4];
    let mut plan: Vec<(i32, i16, usize)> = Vec::new();
    for (layer_index, layer) in sprite.layers.iter().enumerate() {
        if layer.kind != LayerKind::Image || !layer.flags.visible || layer.opacity == 0 {
            continue;
        }
        if sprite.cel_pixels(layer.id, frame).is_err() {
            continue;
        }
        let z = layer.cels.get(&frame).map(|c| c.z_index()).unwrap_or(0);
        plan.push((layer_index as i32 + z as i32, z, layer_index));
    }
    plan.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    for (_, _, layer_index) in plan {
        let layer = &sprite.layers[layer_index];
        if let Ok(cel) = sprite.cel_pixels(layer.id, frame) {
            blit_cel(&mut out, w, h, &cel, sprite, layer.opacity, layer.blend);
        }
    }
    Ok(out)
}

/// Composite layers in `layers[start..end)` (a sibling run) onto `out`.
fn composite_range(
    sprite: &Sprite,
    frame: u16,
    start: usize,
    end: usize,
    out: &mut [u8],
    w: usize,
    h: usize,
) -> Result<()> {
    let mut i = start;
    let mut items: Vec<(i32, i16, usize, usize)> = Vec::new(); // order,z,idx,child_end
    while i < end {
        let layer = &sprite.layers[i];
        let child_end = sibling_end(sprite, i, end);
        let z = layer.cels.get(&frame).map(|c| c.z_index()).unwrap_or(0);
        let order = i as i32 + z as i32;
        items.push((order, z, i, child_end));
        i = child_end;
    }
    items.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    for (_, _, idx, child_end) in items {
        paint_node(sprite, frame, idx, child_end, out, w, h)?;
    }
    Ok(())
}

fn sibling_end(sprite: &Sprite, idx: usize, limit: usize) -> usize {
    let base = sprite.layers[idx].child_level;
    let mut j = idx + 1;
    while j < limit && sprite.layers[j].child_level > base {
        j += 1;
    }
    j
}

fn paint_node(
    sprite: &Sprite,
    frame: u16,
    idx: usize,
    child_end: usize,
    out: &mut [u8],
    w: usize,
    h: usize,
) -> Result<()> {
    let layer = &sprite.layers[idx];
    if !layer.flags.visible || layer.opacity == 0 {
        return Ok(());
    }
    match layer.kind {
        LayerKind::Image => {
            if let Ok(cel) = sprite.cel_pixels(layer.id, frame) {
                blit_cel(out, w, h, &cel, sprite, layer.opacity, layer.blend);
            }
        }
        LayerKind::Group => {
            if child_end <= idx + 1 {
                return Ok(());
            }
            let mut tmp = vec![0u8; w * h * 4];
            composite_range(sprite, frame, idx + 1, child_end, &mut tmp, w, h)?;
            blit_buffer(out, &tmp, layer.opacity, layer.blend);
        }
        LayerKind::Tilemap => {}
    }
    Ok(())
}

fn blit_buffer(dst: &mut [u8], src: &[u8], opacity: u8, blend: BlendMode) {
    for di in (0..dst.len()).step_by(4) {
        let a = scale_alpha(src[di + 3], opacity, 255);
        if a == 0 {
            continue;
        }
        blend::blend_pixel(blend, dst, di, src[di], src[di + 1], src[di + 2], a);
    }
}

fn blit_cel(
    out: &mut [u8],
    canvas_w: usize,
    canvas_h: usize,
    cel: &CelPixels,
    sprite: &Sprite,
    layer_opacity: u8,
    layer_blend: BlendMode,
) {
    match cel {
        CelPixels::Rgba { image, x, y, opacity } => {
            blit_rgba(out, canvas_w, canvas_h, image.pixels.as_ref(), image.width, image.height, *x, *y, *opacity, layer_opacity, layer_blend);
        }
        CelPixels::Indexed { image, x, y, opacity } => {
            let iw = image.width as i32;
            let ih = image.height as i32;
            for py in 0..ih {
                for px in 0..iw {
                    let dx = *x as i32 + px;
                    let dy = *y as i32 + py;
                    if dx < 0 || dy < 0 || dx >= canvas_w as i32 || dy >= canvas_h as i32 {
                        continue;
                    }
                    let idx = image.pixels[py as usize * image.width as usize + px as usize];
                    if idx == sprite.transparent_index {
                        continue;
                    }
                    let color = sprite.palette.get(idx as usize).copied().unwrap_or(Rgba::TRANSPARENT);
                    let a = scale_alpha(color.a, *opacity, layer_opacity);
                    if a == 0 {
                        continue;
                    }
                    let di = (dy as usize * canvas_w + dx as usize) * 4;
                    blend::blend_pixel(layer_blend, out, di, color.r, color.g, color.b, a);
                }
            }
        }
        CelPixels::Gray { image, x, y, opacity } => {
            let iw = image.width as i32;
            let ih = image.height as i32;
            for py in 0..ih {
                for px in 0..iw {
                    let dx = *x as i32 + px;
                    let dy = *y as i32 + py;
                    if dx < 0 || dy < 0 || dx >= canvas_w as i32 || dy >= canvas_h as i32 {
                        continue;
                    }
                    let si = (py as usize * image.width as usize + px as usize) * 2;
                    let v = image.pixels[si];
                    let a = scale_alpha(image.pixels[si + 1], *opacity, layer_opacity);
                    if a == 0 {
                        continue;
                    }
                    let di = (dy as usize * canvas_w + dx as usize) * 4;
                    blend::blend_pixel(layer_blend, out, di, v, v, v, a);
                }
            }
        }
    }
    let _ = sprite.color_mode;
}

fn blit_rgba(
    out: &mut [u8],
    canvas_w: usize,
    canvas_h: usize,
    pixels: &[u8],
    iw_u: u16,
    ih_u: u16,
    x: i16,
    y: i16,
    opacity: u8,
    layer_opacity: u8,
    layer_blend: BlendMode,
) {
    let iw = iw_u as i32;
    let ih = ih_u as i32;
    for py in 0..ih {
        for px in 0..iw {
            let dx = x as i32 + px;
            let dy = y as i32 + py;
            if dx < 0 || dy < 0 || dx >= canvas_w as i32 || dy >= canvas_h as i32 {
                continue;
            }
            let si = (py as usize * iw_u as usize + px as usize) * 4;
            let a = scale_alpha(pixels[si + 3], opacity, layer_opacity);
            if a == 0 {
                continue;
            }
            let di = (dy as usize * canvas_w + dx as usize) * 4;
            blend::blend_pixel(layer_blend, out, di, pixels[si], pixels[si + 1], pixels[si + 2], a);
        }
    }
}

fn scale_alpha(a: u8, cel: u8, layer: u8) -> u8 {
    let v = (a as u32) * (cel as u32) * (layer as u32) / (255 * 255);
    v.min(255) as u8
}

pub fn flatten_all_frames(sprite: &Sprite) -> Result<Vec<Vec<u8>>> {
    let mut frames = Vec::new();
    for f in 0..sprite.frame_count() {
        frames.push(flatten_frame_rgba(sprite, f as u16)?);
    }
    if frames.is_empty() {
        return Err(AseErr("sprite has no frames".into()));
    }
    let _ = ColorMode::Rgba;
    Ok(frames)
}

#[cfg(test)]
mod group_tests {
    use pxl_ase_doc::{
        BlendMode, Cel, ColorMode, ImageBuffer, Layer, LayerFlags, LayerId, LayerKind, RgbaImage,
        Sprite,
    };

    use super::flatten_frame_rgba;

    fn flags() -> LayerFlags {
        LayerFlags {
            visible: true,
            editable: true,
            ..Default::default()
        }
    }

    #[test]
    fn group_composites_children_then_blends() {
        let mut s = Sprite::new(1, 1, ColorMode::Rgba);
        s.header_flags |= 2;
        s.layers.clear();
        let white_img = s.insert_image(ImageBuffer::Rgba(RgbaImage::from_rgba(
            1,
            1,
            vec![255, 255, 255, 255],
        )));
        let red_img = s.insert_image(ImageBuffer::Rgba(RgbaImage::from_rgba(
            1,
            1,
            vec![128, 0, 0, 255],
        )));
        s.layers.push(Layer {
            id: LayerId(0),
            name: "white".into(),
            kind: LayerKind::Image,
            flags: flags(),
            child_level: 0,
            blend: BlendMode::Normal,
            opacity: 255,
            cels: Default::default(),
            tileset_index: None,
            uuid: None,
            user_data: Default::default(),
        });
        s.set_cel(LayerId(0), 0, Cel::image(white_img, 0, 0, 255));
        s.layers.push(Layer {
            id: LayerId(1),
            name: "G".into(),
            kind: LayerKind::Group,
            flags: flags(),
            child_level: 0,
            blend: BlendMode::Multiply,
            opacity: 255,
            cels: Default::default(),
            tileset_index: None,
            uuid: None,
            user_data: Default::default(),
        });
        s.layers.push(Layer {
            id: LayerId(2),
            name: "red".into(),
            kind: LayerKind::Image,
            flags: flags(),
            child_level: 1,
            blend: BlendMode::Normal,
            opacity: 255,
            cels: Default::default(),
            tileset_index: None,
            uuid: None,
            user_data: Default::default(),
        });
        s.set_cel(LayerId(2), 0, Cel::image(red_img, 0, 0, 255));
        let flat = flatten_frame_rgba(&s, 0).unwrap();
        assert!(flat[0] < 200, "{flat:?}");
        assert_eq!(flat[1], 0);
    }
}
