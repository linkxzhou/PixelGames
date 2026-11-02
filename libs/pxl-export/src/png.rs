use std::io::Cursor;

use pxl_core::{IndexFrame, Palette, PxlError, PxlResult, Rgba};

use crate::raster::scale_nearest;

pub fn encode_indexed_png(frame: &IndexFrame, palette: &Palette, scale: u32) -> PxlResult<Vec<u8>> {
    let image = if scale <= 1 {
        frame.clone()
    } else {
        scale_nearest(frame, scale)?
    };
    encode_indexed_bytes(image.w as u32, image.h as u32, &image.pixels, palette)
}

fn encode_indexed_bytes(w: u32, h: u32, pixels: &[u8], palette: &Palette) -> PxlResult<Vec<u8>> {
    let mut buf = Vec::new();
    let mut encoder = png::Encoder::new(&mut buf, w, h);
    encoder.set_color(png::ColorType::Indexed);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    encoder.set_filter(png::FilterType::NoFilter);
    let mut pal = Vec::with_capacity(palette.colors.len() * 3);
    let mut trns = Vec::with_capacity(palette.colors.len());
    let colors: Vec<Rgba> = if palette.colors.is_empty() {
        vec![Rgba::TRANSPARENT]
    } else {
        palette.colors.clone()
    };
    for c in &colors {
        pal.extend_from_slice(&[c.r, c.g, c.b]);
        trns.push(if c.a == 0 { 0 } else { 255 });
    }
    encoder.set_palette(pal);
    encoder.set_trns(trns);
    let mut writer = encoder
        .write_header()
        .map_err(|e| PxlError::io(format!("png header: {e}")))?;
    writer
        .write_image_data(pixels)
        .map_err(|e| PxlError::io(format!("png data: {e}")))?;
    drop(writer);
    Ok(buf)
}

pub fn encode_rgba_png(w: u32, h: u32, rgba: &[u8]) -> PxlResult<Vec<u8>> {
    if rgba.len() != (w as usize) * (h as usize) * 4 {
        return Err(PxlError::semantic("rgba buffer has the wrong length"));
    }
    let mut buf = Vec::new();
    let mut encoder = png::Encoder::new(&mut buf, w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    encoder.set_filter(png::FilterType::NoFilter);
    let mut writer = encoder
        .write_header()
        .map_err(|e| PxlError::io(format!("png header: {e}")))?;
    writer
        .write_image_data(rgba)
        .map_err(|e| PxlError::io(format!("png data: {e}")))?;
    drop(writer);
    Ok(buf)
}

/// Largest decoded image accepted, in pixels. Sheets and previews are tiny; this
/// only stops a hostile header from forcing a huge allocation.
const MAX_DECODE_PIXELS: u64 = 16 * 1024 * 1024;

fn read_png(
    bytes: &[u8],
    transformations: png::Transformations,
) -> PxlResult<(png::Reader<Cursor<&[u8]>>, png::OutputInfo, Vec<u8>)> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(transformations);
    let mut reader = decoder
        .read_info()
        .map_err(|e| PxlError::io(format!("png read: {e}")))?;
    let header = reader.info();
    let pixels = header.width as u64 * header.height as u64;
    if pixels > MAX_DECODE_PIXELS {
        return Err(PxlError::semantic(format!(
            "png is {}x{} ({pixels} pixels), over the {MAX_DECODE_PIXELS} pixel limit",
            header.width, header.height
        ))
        .with_hint("pxl reads sheets and previews, not photographs"));
    }
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| PxlError::io(format!("png frame: {e}")))?;
    buf.truncate(info.buffer_size());
    Ok((reader, info, buf))
}

/// Decode any 8/16-bit or packed-depth PNG to 8-bit RGBA. Palette, tRNS,
/// grayscale and 16-bit samples are all expanded, so the result is always
/// exactly `w * h * 4` bytes.
pub fn decode_png_rgba(bytes: &[u8]) -> PxlResult<(u32, u32, Vec<u8>)> {
    let (_reader, info, buf) = read_png(bytes, png::Transformations::normalize_to_color8())?;
    let (w, h) = (info.width, info.height);
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|g| [*g, *g, *g, 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf
            .chunks_exact(2)
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Indexed => {
            return Err(PxlError::semantic("png palette was not expanded")
                .with_hint("this is a pxl bug; please report it"));
        }
    };
    if rgba.len() as u64 != w as u64 * h as u64 * 4 {
        return Err(PxlError::io(format!(
            "png decoded to {} bytes, expected {}",
            rgba.len(),
            w as u64 * h as u64 * 4
        )));
    }
    Ok((w, h, rgba))
}

/// Indexed pixels (one palette index per byte, whatever the file's bit depth)
/// plus the palette stored in the file.
pub fn decode_indexed(bytes: &[u8]) -> PxlResult<(u32, u32, Vec<u8>, Vec<Rgba>)> {
    let (reader, info, packed) = read_png(bytes, png::Transformations::IDENTITY)?;
    if info.color_type != png::ColorType::Indexed {
        return Err(PxlError::semantic("sheet png is not indexed")
            .with_hint("pxl sheet writes an indexed png; re-export with pxl sheet"));
    }
    let (w, h) = (info.width as usize, info.height as usize);
    let depth = match info.bit_depth {
        png::BitDepth::One => 1usize,
        png::BitDepth::Two => 2,
        png::BitDepth::Four => 4,
        png::BitDepth::Eight => 8,
        png::BitDepth::Sixteen => {
            return Err(PxlError::semantic("indexed png cannot be 16 bit"));
        }
    };
    let row_bytes = (w * depth).div_ceil(8);
    if packed.len() < row_bytes * h {
        return Err(PxlError::io(format!(
            "indexed png has {} bytes, expected {}",
            packed.len(),
            row_bytes * h
        )));
    }
    let mut pixels = Vec::with_capacity(w * h);
    let mask: u16 = (1u16 << depth) - 1;
    for row in packed.chunks(row_bytes).take(h) {
        for x in 0..w {
            let bit = x * depth;
            let shift = 8 - depth - (bit % 8);
            pixels.push(((row[bit / 8] as u16 >> shift) & mask) as u8);
        }
    }
    let pal = reader.info().palette.clone().unwrap_or_default();
    let trns = reader.info().trns.clone().unwrap_or_default();
    let colors = pal
        .chunks_exact(3)
        .enumerate()
        .map(|(i, rgb)| Rgba {
            r: rgb[0],
            g: rgb[1],
            b: rgb[2],
            a: trns.get(i).copied().unwrap_or(255),
        })
        .collect();
    Ok((info.width, info.height, pixels, colors))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raster::scale_rgba;
    use crate::tests::support::palette;
    use pxl_core::Pcg32;

    /// Raw PNG with a caller-chosen colour type, depth and sample bytes.
    fn raw_png(
        w: u32,
        h: u32,
        color: png::ColorType,
        depth: png::BitDepth,
        palette: Option<Vec<u8>>,
        trns: Option<Vec<u8>>,
        data: &[u8],
    ) -> Vec<u8> {
        let mut buf = Vec::new();
        let mut enc = png::Encoder::new(&mut buf, w, h);
        enc.set_color(color);
        enc.set_depth(depth);
        if let Some(p) = palette {
            enc.set_palette(p);
        }
        if let Some(t) = trns {
            enc.set_trns(t);
        }
        let mut writer = enc.write_header().unwrap();
        writer.write_image_data(data).unwrap();
        drop(writer);
        buf
    }

    /// 8x1, 1 bit per pixel: indices 0 1 0 1 1 0 0 1 packed into one byte.
    fn one_bit_png() -> Vec<u8> {
        raw_png(
            8,
            1,
            png::ColorType::Indexed,
            png::BitDepth::One,
            Some(vec![0, 0, 0, 255, 255, 255]),
            Some(vec![0, 255]),
            &[0b0101_1001],
        )
    }

    #[test]
    fn one_bit_indexed_png_decodes_one_index_per_pixel() {
        let (w, h, px, colors) = decode_indexed(&one_bit_png()).unwrap();
        assert_eq!((w, h), (8, 1));
        assert_eq!(px, vec![0, 1, 0, 1, 1, 0, 0, 1]);
        assert_eq!(colors.len(), 2);
        assert_eq!(colors[0].a, 0);
    }

    #[test]
    fn one_bit_indexed_png_decodes_to_a_full_rgba_buffer() {
        let (w, h, rgba) = decode_png_rgba(&one_bit_png()).unwrap();
        assert_eq!((w, h), (8, 1));
        assert_eq!(rgba.len(), 8 * 4);
        assert_eq!(&rgba[0..4], &[0, 0, 0, 0]);
        assert_eq!(&rgba[4..8], &[255, 255, 255, 255]);
        let (ow, oh, out) = scale_rgba(w, h, &rgba, 3, false);
        assert_eq!((ow, oh), (24, 3));
        assert_eq!(out.len(), 24 * 3 * 4);
    }

    #[test]
    fn sixteen_bit_rgba_is_reduced_to_eight_bit_samples() {
        let data: Vec<u8> = vec![0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xff, 0xff];
        let png = raw_png(
            1,
            1,
            png::ColorType::Rgba,
            png::BitDepth::Sixteen,
            None,
            None,
            &data,
        );
        let (w, h, rgba) = decode_png_rgba(&png).unwrap();
        assert_eq!((w, h), (1, 1));
        assert_eq!(rgba, vec![0x12, 0x56, 0x9a, 0xff]);
    }

    #[test]
    fn grayscale_alpha_and_low_depth_gray_expand_correctly() {
        let ga = raw_png(
            1,
            1,
            png::ColorType::GrayscaleAlpha,
            png::BitDepth::Eight,
            None,
            None,
            &[40, 200],
        );
        assert_eq!(decode_png_rgba(&ga).unwrap().2, vec![40, 40, 40, 200]);
        let gray = raw_png(
            4,
            1,
            png::ColorType::Grayscale,
            png::BitDepth::Two,
            None,
            None,
            &[0b00_01_10_11],
        );
        let rgba = decode_png_rgba(&gray).unwrap().2;
        assert_eq!(rgba.len(), 16);
        assert_eq!(&rgba[0..4], &[0, 0, 0, 255]);
        assert_eq!(&rgba[12..16], &[255, 255, 255, 255]);
    }

    #[test]
    fn truncated_or_oversized_png_is_an_error_not_a_panic() {
        let good = one_bit_png();
        assert!(decode_png_rgba(&good[..good.len() / 2]).is_err());
        assert!(decode_indexed(&good[..good.len() / 2]).is_err());
        assert!(decode_png_rgba(b"not a png").is_err());
        let mut huge = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut huge, 30000, 30000);
            enc.set_color(png::ColorType::Grayscale);
            enc.set_depth(png::BitDepth::One);
            let mut w = enc.write_header().unwrap();
            let _ = w.write_image_data(&vec![0u8; 30000 / 8 * 30000]);
        }
        assert!(
            decode_png_rgba(&huge).is_err(),
            "a 900 MP image must be refused before it is expanded"
        );
    }

    #[test]
    fn png_roundtrip_is_stable() {
        let pal = palette();
        let mut frame = IndexFrame::new(4, 2).unwrap();
        frame.set(1, 0, 3);
        frame.set(2, 1, 6);
        let a = encode_indexed_png(&frame, &pal, 1).unwrap();
        let b = encode_indexed_png(&frame, &pal, 1).unwrap();
        assert_eq!(a, b);
        let (w, h, px, colors) = decode_indexed(&a).unwrap();
        assert_eq!((w, h), (4, 2));
        assert_eq!(px[1], 3);
        assert_eq!(colors[0].a, 0);
        assert!(colors[3].a == 255);
        let _ = Pcg32::from_seed(1);
    }
}
