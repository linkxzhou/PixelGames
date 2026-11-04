use super::grid::reduce;

use super::*;

/// A 6x6 native picture blown up by `k` with an offset and a soft fringe.
fn fake(k: usize, offset: usize) -> (Rgba8, [[u8; 4]; 36]) {
    let mut native = [[0u8; 4]; 36];
    for y in 0..6 {
        for x in 0..6 {
            native[y * 6 + x] = match (x + y) % 3 {
                0 => [200, 40, 40, 255],
                1 => [40, 160, 60, 255],
                _ => [30, 30, 120, 255],
            };
        }
    }
    let n = 6 * k + offset;
    let mut data = vec![0u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let (nx, ny) = (
                (x.saturating_sub(offset)) / k,
                (y.saturating_sub(offset)) / k,
            );
            let mut c = native[ny.min(5) * 6 + nx.min(5)];
            // A one-pixel blurred seam, like JPEG or AI fringing.
            if (x + k - offset % k) % k == 0 {
                c[0] = c[0].saturating_add(12);
            }
            data[(y * n + x) * 4..(y * n + x) * 4 + 4].copy_from_slice(&c);
        }
    }
    (Rgba8 { w: n, h: n, data }, native)
}

#[test]
fn the_grid_of_an_upscaled_image_is_found() {
    let (img, _) = fake(5, 0);

    assert_eq!(detect_grid(&img, 32).cell, 5);
    let (img, _) = fake(8, 3);
    let g = detect_grid(&img, 32);
    assert_eq!((g.cell, g.phase_x), (8, 3));
}

#[test]
fn a_native_image_has_no_grid() {
    let mut data = Vec::new();
    for i in 0..32 * 32u32 {
        let v = (i.wrapping_mul(2654435761) >> 24) as u8;
        data.extend_from_slice(&[v, v / 2, 255 - v, 255]);
    }
    let img = Rgba8 { w: 32, h: 32, data };
    assert_eq!(detect_grid(&img, 16).cell, 1);
}

#[test]
fn recovering_a_fake_image_gives_back_the_native_pixels() {
    let (img, native) = fake(6, 0);
    let opts = PixelizeOptions {
        colors: 3,
        ..Default::default()
    };
    let out = pixelize(&img, &opts).unwrap();
    assert_eq!((out.frame.w, out.frame.h), (6, 6));
    // Same colour classes as the source: equal native pixels share an index.
    for i in 0..36 {
        for j in 0..36 {
            let same = native[i] == native[j];
            assert_eq!(same, out.frame.pixels[i] == out.frame.pixels[j], "{i} {j}");
        }
    }
}

#[test]
fn a_thin_dark_line_survives_downscaling_with_expansion() {
    let n = 32;
    let mut data = vec![0u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let c = if x == 13 {
                [10, 10, 10, 255]
            } else {
                [230, 220, 200, 255]
            };
            data[(y * n + x) * 4..(y * n + x) * 4 + 4].copy_from_slice(&c);
        }
    }
    let img = Rgba8 { w: n, h: n, data };
    let plain = downscale(&img, 4, 4, Downscale::KCentroid);
    assert!(
        plain.data.chunks(4).all(|p| p[0] > 100),
        "the line is lost without expansion"
    );
    let opts = PixelizeOptions {
        size: Some((4, 4)),
        method: Downscale::KCentroid,
        outline_radius: Some(2),
        colors: 2,
        ..Default::default()
    };
    let out = pixelize(&img, &opts).unwrap();
    let dark = out.palette.special.outline;
    assert!(
        (0..4).all(|y| out.frame.get(1, y) == dark),
        "{:?}",
        out.frame.pixels
    );
}

#[test]
fn contrast_keeps_the_minority_extreme() {
    let mut cell = vec![[200u8, 200, 200, 255]; 15];
    cell.push([0, 0, 0, 255]);
    assert_eq!(reduce(&cell, Downscale::Contrast, cell[0]), [0, 0, 0, 255]);
    assert_eq!(
        reduce(&cell, Downscale::Mode, cell[0]),
        [200, 200, 200, 255]
    );
    let flat = vec![[120u8, 120, 120, 255]; 16];
    assert_eq!(
        reduce(&flat, Downscale::Contrast, flat[0]),
        [120, 120, 120, 255]
    );
}
