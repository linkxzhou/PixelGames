use serde::{Deserialize, Serialize};

/// Percentages use basis points (10000 = 100%). None disables a style limit.
/// These constraints describe pixels, never asset names, directions or genres.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct QaConfig {
    pub strict: bool,
    pub max_frame_pixels: usize,
    pub max_total_pixels: usize,
    pub max_entries: usize,
    pub max_samples: usize,
    pub max_colors: Option<usize>,
    pub max_components: Option<usize>,
    pub max_isolated_pixels: Option<usize>,
    pub max_holes: Option<usize>,
    pub max_border_pixels: Option<usize>,
    pub min_coverage_bps: Option<u32>,
    pub max_coverage_bps: Option<u32>,
    pub max_color_specks: Option<usize>,
    pub max_outline_gaps: Option<usize>,
    pub max_duplicate_colors: Option<usize>,
    pub max_checkerboard_bps: Option<u32>,
    pub min_neighbor_contrast: Option<u32>,
    pub max_animation_diff_bps: Option<u32>,
    pub max_area_change_bps: Option<u32>,
    pub max_centroid_shift_millipixels: Option<u32>,
    pub max_palette_change_bps: Option<u32>,
    pub allow_duplicate_frames: bool,
    pub allow_empty_frames: bool,
    pub allow_terminal_empty: bool,
    pub require_tile_full_bleed: bool,
    pub tile_horizontal: bool,
    pub tile_vertical: bool,
    pub max_seam_channel_delta: u32,
    pub max_seam_ramp_steps: usize,
    /// Diagnostics about native resolution, not about the art itself. A value
    /// is a warning by default and an error in strict. Display scale is reported
    /// by `pxl render`/`pxl build` instead: QA only sees pixels, not the
    /// multiple a game will draw them at.
    ///
    /// Warn when the opaque subject covers less than this share of the canvas,
    /// which is what "the canvas grew but the art did not" looks like.
    /// `None` disables.
    pub min_content_coverage_bps: Option<u32>,
    /// Warn when this share of aligned 2x2 blocks is one colour, which is what
    /// an upscaled image looks like. `None` measures without judging.
    pub copy2x_warn_bps: Option<u32>,
}

impl Default for QaConfig {
    fn default() -> Self {
        Self {
            strict: false,
            max_frame_pixels: 2048 * 2048,
            max_total_pixels: 32 * 1024 * 1024,
            max_entries: 100_000,
            max_samples: 16,
            max_colors: Some(32),
            max_components: Some(8),
            max_isolated_pixels: Some(4),
            max_holes: Some(0),
            max_border_pixels: None,
            min_coverage_bps: None,
            max_coverage_bps: None,
            max_color_specks: Some(8),
            max_outline_gaps: Some(0),
            max_duplicate_colors: Some(0),
            max_checkerboard_bps: Some(2500),
            min_neighbor_contrast: Some(8),
            max_animation_diff_bps: Some(3500),
            max_area_change_bps: Some(3500),
            max_centroid_shift_millipixels: Some(2000),
            max_palette_change_bps: Some(3500),
            allow_duplicate_frames: false,
            allow_empty_frames: false,
            allow_terminal_empty: true,
            require_tile_full_bleed: true,
            tile_horizontal: true,
            tile_vertical: true,
            max_seam_channel_delta: 32,
            max_seam_ramp_steps: 1,
            min_content_coverage_bps: None,
            copy2x_warn_bps: None,
        }
    }
}

impl QaConfig {
    /// Thresholds for a size tier: a 32 sprite and a 256 sprite should not be
    /// judged by the same noise budget. Ratios that scale with detail
    /// (isolated pixels, specks) grow with the tier; palette and component
    /// caps stay proportional to how much room there is to say something.
    pub fn for_profile(profile: &pxl_core::SizeProfile) -> Self {
        Self {
            max_colors: Some(profile.qa_max_colors),
            max_components: Some(profile.qa_max_components),
            max_isolated_pixels: Some(profile.qa_max_isolated),
            max_color_specks: Some(profile.qa_max_specks),
            min_content_coverage_bps: Some(profile.qa_min_content_bps),
            copy2x_warn_bps: profile.qa_copy2x_warn_bps,
            ..Self::default()
        }
    }

    /// Thresholds from an effective policy: generation, dry-run and final QA
    /// read the same budgets, so a build can never pass QA under rules it was
    /// not authored with. The modern profile carries its own palette and
    /// frame-colour budgets; retro block checks (per-8x8 four colours, NES
    /// attribute limits) stay off because the policy says they do not apply.
    pub fn for_policy(policy: &pxl_contract::policy::QaThresholds) -> Self {
        Self {
            max_colors: Some(usize::from(policy.frame_opaque_colors)),
            max_entries: usize::from(policy.palette_entries),
            ..Self::default()
        }
    }

    pub(crate) fn check(&self) -> pxl_core::PxlResult<()> {
        let ratios = [
            self.min_coverage_bps,
            self.max_coverage_bps,
            self.max_checkerboard_bps,
            self.max_animation_diff_bps,
            self.max_area_change_bps,
            self.max_palette_change_bps,
            self.min_content_coverage_bps,
            self.copy2x_warn_bps,
        ];
        if ratios.into_iter().flatten().any(|v| v > 10000)
            || self
                .min_coverage_bps
                .zip(self.max_coverage_bps)
                .is_some_and(|(a, b)| a > b)
            || self.max_frame_pixels == 0
            || self.max_total_pixels == 0
            || self.max_entries == 0
            || self.max_samples > 1024
            || self.max_seam_channel_delta > 255
            || self.min_neighbor_contrast.is_some_and(|v| v > 255)
        {
            return Err(pxl_core::PxlError::usage("invalid QA configuration: ratios must be 0..=10000, channel deltas 0..=255, sample limit <=1024, and budgets positive"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod profile_tests {
    use super::*;
    use pxl_core::{SizeProfile, SizeTier};

    #[test]
    fn tiers_get_their_own_budgets() {
        let a = QaConfig::for_profile(&SizeProfile::of(SizeTier::S32));
        let b = QaConfig::for_profile(&SizeProfile::of(SizeTier::S256));
        assert_eq!(a.max_colors, Some(16));
        assert_eq!(b.max_colors, Some(48));
        assert!(b.max_isolated_pixels > a.max_isolated_pixels);
        assert_eq!(a.copy2x_warn_bps, None);
        assert_eq!(b.copy2x_warn_bps, Some(8000));
        b.check().unwrap();
    }
}
