//! Preview backgrounds that keep a dark outline visible.
//!
//! [`crate::composite`] paints transparent pixels `(28, 26, 24)`, which is close
//! enough to a typical `#202028` outline that the outline disappears and the
//! sprite looks coarser than it is. `composite_on` takes the background as an
//! argument so a preview can pick one the outline stands out against. The old
//! function is untouched, so existing previews and hashes do not change.

use pxl_core::{parse_hex, Oklab, Palette, PxlError, PxlResult, Rgba};

use crate::raster::scaled_dims;

/// The background `composite` has always used.
pub const CLASSIC: Rgba = Rgba {
    r: 28,
    g: 26,
    b: 24,
    a: 255,
};
/// Mid grey the `auto` choice falls back to.
pub const MID_GREY: Rgba = Rgba {
    r: 0x6b,
    g: 0x6b,
    b: 0x78,
    a: 255,
};
const CHECKER_DARK: Rgba = MID_GREY;
const CHECKER_LIGHT: Rgba = Rgba {
    r: 0x80,
    g: 0x80,
    b: 0x8c,
    a: 255,
};
/// 0.15 OKLab lightness in Q16: below this gap an outline reads as "gone".
pub const MIN_OUTLINE_DL: i32 = 9830;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Background {
    /// `(28, 26, 24)`, what `composite` paints.
    Classic,
    Solid(Rgba),
    /// Two greys in squares of `cell` output pixels.
    Checker {
        cell: u32,
    },
}

impl Background {
    /// `classic`, `auto`, `checker`, or `#rrggbb`. `auto` keeps the classic
    /// background unless the palette's outline would be hidden on it, then uses
    /// a mid grey. `cell` is the checker square in output pixels.
    pub fn resolve(text: &str, palette: &Palette, cell: u32) -> PxlResult<Self> {
        match text {
            "classic" => Ok(Self::Classic),
            "auto" => Ok(Self::auto_for(palette)),
            "checker" => Ok(Self::Checker { cell: cell.max(1) }),
            hex if hex.starts_with('#') => {
                let color = parse_hex(hex)?;
                if color.a != 255 {
                    return Err(PxlError::usage(format!("background {hex} must be opaque"))
                        .with_hint("use #rrggbb"));
                }
                Ok(Self::Solid(color))
            }
            other => Err(PxlError::usage(format!("unknown background '{other}'"))
                .with_hint("background is classic, auto, checker, or #rrggbb")),
        }
    }

    pub fn auto_for(palette: &Palette) -> Self {
        let outline = outline_color(palette);
        let gap = (Oklab::from_rgba(outline).l - Oklab::from_rgba(CLASSIC).l).abs();
        if gap < MIN_OUTLINE_DL {
            Self::Solid(MID_GREY)
        } else {
            Self::Classic
        }
    }

    /// The colour a dark outline has to stand out against. For a checker this
    /// is the darker square, the worst case for a dark outline.
    pub fn reference(self) -> Rgba {
        match self {
            Self::Classic => CLASSIC,
            Self::Solid(c) => c,
            Self::Checker { .. } => CHECKER_DARK,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::Solid(_) => "solid",
            Self::Checker { .. } => "checker",
        }
    }

    fn at(self, x: u32, y: u32) -> Rgba {
        match self {
            Self::Classic => CLASSIC,
            Self::Solid(c) => c,
            Self::Checker { cell } => {
                if (x / cell + y / cell) % 2 == 0 {
                    CHECKER_DARK
                } else {
                    CHECKER_LIGHT
                }
            }
        }
    }
}

fn outline_color(palette: &Palette) -> Rgba {
    palette
        .colors
        .get(palette.special.outline as usize)
        .copied()
        .filter(|c| c.a != 0)
        .unwrap_or(Rgba::rgb(0, 0, 0))
}

/// Nearest-neighbour RGBA preview of `frame` over `bg`. The size is checked
/// before anything is allocated.
pub fn composite_on(
    frame: &pxl_core::IndexFrame,
    palette: &Palette,
    scale: u32,
    bg: Background,
) -> PxlResult<(u32, u32, Vec<u8>)> {
    let scale = scale.max(1);
    let (w, h) = scaled_dims(frame.w as u32, frame.h as u32, scale)?;
    let mut rgba = vec![0u8; (w as usize) * (h as usize) * 4];
    for y in 0..h {
        for x in 0..w {
            let idx = frame.get((x / scale) as i32, (y / scale) as i32);
            let col = palette
                .colors
                .get(idx as usize)
                .copied()
                .unwrap_or(Rgba::TRANSPARENT);
            let c = if col.a == 0 { bg.at(x, y) } else { col };
            let i = ((y * w + x) * 4) as usize;
            rgba[i] = c.r;
            rgba[i + 1] = c.g;
            rgba[i + 2] = c.b;
            rgba[i + 3] = 255;
        }
    }
    Ok((w, h, rgba))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pxl_core::IndexFrame;

    fn palette(outline: Rgba) -> Palette {
        let mut p = Palette {
            id: "t".into(),
            colors: vec![Rgba::TRANSPARENT, outline, Rgba::rgb(232, 64, 64)],
            ramps: Default::default(),
            cycles: Default::default(),
            special: pxl_core::Special::default(),
        };
        p.special.outline = 1;
        p.special.flash = 1;
        p
    }

    #[test]
    fn auto_leaves_the_classic_background_when_the_outline_stands_out() {
        let light = palette(Rgba::rgb(240, 240, 232));
        assert_eq!(Background::auto_for(&light), Background::Classic);
    }

    #[test]
    fn auto_switches_to_grey_when_a_dark_outline_would_vanish() {
        let dark = palette(Rgba::rgb(32, 32, 40));
        assert_eq!(Background::auto_for(&dark), Background::Solid(MID_GREY));
        let gap = (Oklab::from_rgba(Rgba::rgb(32, 32, 40)).l
            - Oklab::from_rgba(Background::auto_for(&dark).reference()).l)
            .abs();
        assert!(
            gap >= MIN_OUTLINE_DL,
            "the chosen grey must clear 0.15: {gap}"
        );
    }

    #[test]
    fn the_checker_darker_square_also_clears_a_dark_outline() {
        let gap = (Oklab::from_rgba(Rgba::rgb(32, 32, 40)).l
            - Oklab::from_rgba(Background::Checker { cell: 8 }.reference()).l)
            .abs();
        assert!(gap >= MIN_OUTLINE_DL, "{gap}");
    }

    #[test]
    fn resolve_accepts_the_documented_names_and_rejects_the_rest() {
        let p = palette(Rgba::rgb(32, 32, 40));
        assert_eq!(
            Background::resolve("classic", &p, 8).unwrap(),
            Background::Classic
        );
        assert_eq!(
            Background::resolve("#102030", &p, 8).unwrap(),
            Background::Solid(Rgba::rgb(0x10, 0x20, 0x30))
        );
        assert_eq!(
            Background::resolve("checker", &p, 0).unwrap(),
            Background::Checker { cell: 1 }
        );
        let err = Background::resolve("grey", &p, 8).unwrap_err();
        assert!(err.hint.is_some());
        assert!(Background::resolve("#10203040", &p, 8).is_err());
    }

    #[test]
    fn classic_matches_the_old_composite_byte_for_byte() {
        let p = palette(Rgba::rgb(32, 32, 40));
        let mut f = IndexFrame::new(4, 4).unwrap();
        f.set(1, 1, 2);
        f.set(2, 2, 1);
        let old = crate::composite(&f, &p, 3, false);
        let new = composite_on(&f, &p, 3, Background::Classic).unwrap();
        assert_eq!(old, new);
    }

    #[test]
    fn checker_alternates_by_cell_and_leaves_opaque_pixels_alone() {
        let p = palette(Rgba::rgb(32, 32, 40));
        let mut f = IndexFrame::new(4, 1).unwrap();
        f.set(3, 0, 2);
        let (w, _, rgba) = composite_on(&f, &p, 1, Background::Checker { cell: 2 }).unwrap();
        let px = |x: usize| &rgba[x * 4..x * 4 + 3];
        assert_eq!(w, 4);
        assert_eq!(px(0), [CHECKER_DARK.r, CHECKER_DARK.g, CHECKER_DARK.b]);
        assert_eq!(px(1), px(0));
        assert_eq!(px(2), [CHECKER_LIGHT.r, CHECKER_LIGHT.g, CHECKER_LIGHT.b]);
        assert_eq!(px(3), [232, 64, 64]);
    }
}
