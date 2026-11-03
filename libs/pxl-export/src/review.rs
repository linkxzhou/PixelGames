use pxl_core::{scale_copy, IndexFrame, Palette, PxlResult, Rgba};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewMode {
    Native,
    Silhouette,
    Grayscale,
    Posterize3,
}

pub fn review_image(
    frame: &IndexFrame,
    palette: &Palette,
    mode: ReviewMode,
    scale: u16,
    background: Option<Rgba>,
) -> PxlResult<(IndexFrame, Palette)> {
    if scale == 0 {
        return Err(pxl_core::PxlError::usage("review scale must be at least 1"));
    }
    let mut output_palette = palette.clone();
    match mode {
        ReviewMode::Native => {}
        ReviewMode::Silhouette => {
            let foreground = palette.special.flash;
            for index in 0..palette.colors.len() {
                output_palette.colors[index] = if index == 0 {
                    Rgba::TRANSPARENT
                } else {
                    palette.colors[foreground as usize]
                };
            }
        }
        ReviewMode::Grayscale | ReviewMode::Posterize3 => {
            for (index, color) in palette.colors.iter().enumerate() {
                if index == 0 {
                    continue;
                }
                let luma = (u32::from(color.r) * 299
                    + u32::from(color.g) * 587
                    + u32::from(color.b) * 114
                    + 500)
                    / 1000;
                let value = if mode == ReviewMode::Posterize3 {
                    match luma * 3 / 256 {
                        0 => 32,
                        1 => 128,
                        _ => 240,
                    }
                } else {
                    luma
                };
                output_palette.colors[index] = Rgba {
                    r: value as u8,
                    g: value as u8,
                    b: value as u8,
                    a: color.a,
                };
            }
        }
    }
    let mut scaled = scale_copy(frame, scale)?;
    if mode == ReviewMode::Silhouette {
        for pixel in &mut scaled.pixels {
            if *pixel != 0 {
                *pixel = palette.special.flash;
            }
        }
    }
    if let Some(background) = background {
        let index = output_palette.colors.len();
        if index >= 256 {
            return Err(pxl_core::PxlError::semantic(
                "review background needs one free palette index",
            ));
        }
        output_palette.colors.push(background);
        for pixel in &mut scaled.pixels {
            if *pixel == 0 {
                *pixel = index as u8;
            }
        }
    }
    Ok((scaled, output_palette))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_modes_are_deterministic_and_do_not_change_production_frames() {
        let source = pxl_core::decode_matrix(&["010".into(), "002".into()]).unwrap();
        let palette = Palette {
            id: "test".into(),
            colors: vec![Rgba::TRANSPARENT, Rgba::rgb(200, 40, 20)],
            ramps: Default::default(),
            cycles: Default::default(),
            special: pxl_core::Special {
                transparent: 0,
                outline: 1,
                flash: 1,
            },
        };
        let original = source.clone();
        let (silhouette, _) = review_image(
            &source,
            &palette,
            ReviewMode::Silhouette,
            2,
            Some(Rgba::rgb(10, 10, 10)),
        )
        .unwrap();
        assert_eq!(silhouette.w, 6);
        assert_eq!(silhouette.h, 4);
        assert!(silhouette.pixels.iter().all(|pixel| *pixel != 0));
        assert_eq!(source, original);
        let (gray, gray_palette) = review_image(
            &source,
            &palette,
            ReviewMode::Grayscale,
            1,
            None,
        )
        .unwrap();
        assert_eq!(gray, source);
        assert_eq!(gray_palette.colors[1].r, gray_palette.colors[1].g);
        assert_eq!(gray_palette.colors[1].g, gray_palette.colors[1].b);
    }
}
