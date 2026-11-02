//! Size tiers. A 32 sprite and a 256 sprite are drawn by different rules: the
//! palette budget, ramp depth, outline style, how much anti-aliasing and
//! dithering is allowed, the SVG sample grid and the QA noise budget all grow
//! with the canvas. Every entry point picks its defaults from here so the same
//! size behaves the same way everywhere.

use crate::{PxlError, PxlResult};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SizeTier {
    S32,
    S64,
    S128,
    S256,
}

/// Canvas sides of the four tiers, smallest first.
pub const SIZE_TIERS: [u16; 4] = [32, 64, 128, 256];

impl SizeTier {
    pub const ALL: [SizeTier; 4] = [Self::S32, Self::S64, Self::S128, Self::S256];

    pub fn side(self) -> u16 {
        match self {
            Self::S32 => 32,
            Self::S64 => 64,
            Self::S128 => 128,
            Self::S256 => 256,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::S32 => "s32",
            Self::S64 => "s64",
            Self::S128 => "s128",
            Self::S256 => "s256",
        }
    }

    /// Exact tier side (`32`, `s64`, ...).
    pub fn parse(text: &str) -> PxlResult<Self> {
        let t = text.trim().trim_start_matches('s');
        Self::ALL
            .into_iter()
            .find(|tier| t.parse::<u16>().ok() == Some(tier.side()))
            .ok_or_else(|| {
                PxlError::usage(alloc::format!("unknown size tier '{text}'"))
                    .with_hint("size tiers are 32, 64, 128 and 256")
            })
    }

    /// Smallest tier whose side holds `max(w, h)`.
    pub fn for_size(w: u16, h: u16) -> PxlResult<Self> {
        let side = w.max(h);
        if w == 0 || h == 0 {
            return Err(PxlError::schema("profile.bad_size: a side is 0"));
        }
        Self::ALL
            .into_iter()
            .find(|tier| side <= tier.side())
            .ok_or_else(|| {
                PxlError::schema(alloc::format!(
                    "profile.bad_size: {w}x{h} is larger than the 256 tier"
                ))
                .with_hint("canvases are at most 256x256")
            })
    }
}

/// How outlines are drawn at a tier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutlineStyle {
    Flat,
    Selout,
    /// Selout plus inner lines between large regions.
    SeloutInner,
}

impl OutlineStyle {
    pub fn name(self) -> &'static str {
        match self {
            Self::Flat => "flat",
            Self::Selout => "selout",
            Self::SeloutInner => "selout_inner",
        }
    }
}

/// Manual anti-aliasing allowed at a tier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AaPolicy {
    Off,
    /// Only on the inside corners of the outer silhouette.
    Corners,
    On,
}

impl AaPolicy {
    pub fn name(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Corners => "corners",
            Self::On => "on",
        }
    }
}

/// Defaults for one tier. Counts are absolute; ratios are basis points.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SizeProfile {
    pub tier: SizeTier,
    pub max_colors: usize,
    pub ramp_levels: u8,
    pub outline: OutlineStyle,
    pub aa: AaPolicy,
    /// Volume shading on by default.
    pub shade: bool,
    /// Ordered dithering allowed on large flat areas.
    pub dither: bool,
    /// Jaggy-rhythm repair in cleanup.
    pub rhythm: bool,
    /// SVG supersampling grid side (N for an N x N grid per pixel).
    pub svg_samples: u8,
    /// Smallest SVG feature, in pixels, that must survive rasterisation.
    pub min_feature: u8,
    /// Nearest-neighbour preview zoom so every tier previews near 256 px.
    pub preview_scale: u32,
    pub qa_max_colors: usize,
    pub qa_max_components: usize,
    pub qa_max_isolated: usize,
    pub qa_max_specks: usize,
    pub qa_min_content_bps: u32,
    /// Warn when this share of aligned 2x2 blocks is uniform (an upscaled
    /// image passed off as native). `None` measures only.
    pub qa_copy2x_warn_bps: Option<u32>,
}

impl SizeProfile {
    pub fn of(tier: SizeTier) -> Self {
        match tier {
            SizeTier::S32 => Self {
                tier,
                max_colors: 16,
                ramp_levels: 3,
                outline: OutlineStyle::Flat,
                aa: AaPolicy::Off,
                shade: false,
                dither: false,
                rhythm: false,
                svg_samples: 8,
                min_feature: 1,
                preview_scale: 8,
                qa_max_colors: 16,
                qa_max_components: 8,
                qa_max_isolated: 4,
                qa_max_specks: 8,
                qa_min_content_bps: 1500,
                qa_copy2x_warn_bps: None,
            },
            SizeTier::S64 => Self {
                tier,
                max_colors: 24,
                ramp_levels: 4,
                outline: OutlineStyle::Selout,
                aa: AaPolicy::Corners,
                shade: true,
                dither: false,
                rhythm: true,
                svg_samples: 4,
                min_feature: 1,
                preview_scale: 4,
                qa_max_colors: 24,
                qa_max_components: 12,
                qa_max_isolated: 8,
                qa_max_specks: 16,
                qa_min_content_bps: 1500,
                qa_copy2x_warn_bps: None,
            },
            SizeTier::S128 => Self {
                tier,
                max_colors: 32,
                ramp_levels: 4,
                outline: OutlineStyle::SeloutInner,
                aa: AaPolicy::On,
                shade: true,
                dither: false,
                rhythm: true,
                svg_samples: 4,
                min_feature: 1,
                preview_scale: 2,
                qa_max_colors: 32,
                qa_max_components: 24,
                qa_max_isolated: 24,
                qa_max_specks: 48,
                qa_min_content_bps: 1200,
                qa_copy2x_warn_bps: Some(9000),
            },
            SizeTier::S256 => Self {
                tier,
                max_colors: 48,
                ramp_levels: 4,
                outline: OutlineStyle::SeloutInner,
                aa: AaPolicy::On,
                shade: true,
                dither: true,
                rhythm: true,
                svg_samples: 2,
                min_feature: 2,
                preview_scale: 1,
                qa_max_colors: 48,
                qa_max_components: 48,
                qa_max_isolated: 64,
                qa_max_specks: 128,
                qa_min_content_bps: 1000,
                qa_copy2x_warn_bps: Some(8000),
            },
        }
    }

    pub fn for_size(w: u16, h: u16) -> PxlResult<Self> {
        Ok(Self::of(SizeTier::for_size(w, h)?))
    }

    pub fn side(&self) -> u16 {
        self.tier.side()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiers_round_up_and_stop_at_256() {
        assert_eq!(SizeTier::for_size(16, 16).unwrap(), SizeTier::S32);
        assert_eq!(SizeTier::for_size(32, 32).unwrap(), SizeTier::S32);
        assert_eq!(SizeTier::for_size(48, 64).unwrap(), SizeTier::S64);
        assert_eq!(SizeTier::for_size(100, 20).unwrap(), SizeTier::S128);
        assert_eq!(SizeTier::for_size(256, 256).unwrap(), SizeTier::S256);
        assert!(SizeTier::for_size(257, 1).is_err());
        assert!(SizeTier::for_size(0, 4).is_err());
        assert_eq!(SizeTier::parse("s128").unwrap(), SizeTier::S128);
        assert_eq!(SizeTier::parse("64").unwrap(), SizeTier::S64);
        assert!(SizeTier::parse("48").is_err());
    }

    #[test]
    fn budgets_grow_with_the_tier() {
        let all: alloc::vec::Vec<SizeProfile> =
            SizeTier::ALL.iter().map(|t| SizeProfile::of(*t)).collect();
        for pair in all.windows(2) {
            assert!(pair[1].max_colors > pair[0].max_colors);
            assert!(pair[1].qa_max_isolated > pair[0].qa_max_isolated);
            assert!(pair[1].preview_scale < pair[0].preview_scale);
            assert!(pair[1].max_colors <= crate::MAX_TEXT_COLORS);
        }
        for p in &all {
            assert_eq!(p.side() as u32 * p.preview_scale, 256);
        }
    }
}
