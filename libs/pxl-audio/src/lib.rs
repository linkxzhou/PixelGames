//! Offline audio synthesis for the pxl pipeline.
//!
//! Text specs go in (`audio-spec/v1`: named SFX parameters and BeepBox JSON),
//! 16-bit WAV plus `audio-manifest.json` come out. Synthesis is delegated to a
//! sandboxed Node adapter; everything else here (validation, limits, WAV
//! encoding, QA, digests, publishing) is plain Rust.
//!
//! Source of the rules: plan/v5.0/web-js-engine-plan.md section 14.

pub mod backend;
pub mod error;
pub mod inspect;
pub mod limits;
pub mod manifest;
pub mod qa;
pub mod render;
pub mod sandbox;
pub mod song;
pub mod spec;
pub mod validate;
pub mod wav;

pub use error::{AudioError, AudioErrorKind, AudioResult};

/// The three audio contracts, embedded from `steps/contracts/` so `step-300
/// schema audio-spec` works without the source tree.
pub const SPEC_SCHEMA: &str = include_str!("../../../steps/contracts/audio-spec.schema.json");
pub const MANIFEST_SCHEMA: &str = include_str!("../../../steps/contracts/audio-manifest.schema.json");
pub const REQUIREMENTS_SCHEMA: &str =
    include_str!("../../../steps/contracts/audio-requirements.schema.json");

pub const SCHEMA_KINDS: &[&str] = &["audio-spec", "audio-manifest", "audio-requirements"];

/// The embedded schema text for an audio schema name, if it is one.
pub fn schema_text(kind: &str) -> Option<&'static str> {
    match kind {
        "audio-spec" => Some(SPEC_SCHEMA),
        "audio-manifest" => Some(MANIFEST_SCHEMA),
        "audio-requirements" => Some(REQUIREMENTS_SCHEMA),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_schemas_are_valid_json_with_matching_ids() {
        for (kind, id) in [
            ("audio-spec", "pxl/audio-spec/v1"),
            ("audio-manifest", "pxl/audio-manifest/v1"),
            ("audio-requirements", "pxl/audio-requirements/v1"),
        ] {
            let value: serde_json::Value = serde_json::from_str(schema_text(kind).unwrap())
                .unwrap_or_else(|e| panic!("{kind}: {e}"));
            assert_eq!(value["$id"], id);
        }
        assert!(schema_text("recipe").is_none());
    }

    /// The Rust contract constants and the schema files must not drift apart.
    #[test]
    fn schema_constants_match_limits() {
        let spec: serde_json::Value = serde_json::from_str(SPEC_SCHEMA).unwrap();
        assert_eq!(
            spec["properties"]["contract"]["const"],
            limits::SPEC_CONTRACT
        );
        assert_eq!(
            spec["properties"]["sample_rate"]["const"],
            limits::SAMPLE_RATE
        );
        let manifest: serde_json::Value = serde_json::from_str(MANIFEST_SCHEMA).unwrap();
        assert_eq!(
            manifest["properties"]["contract"]["const"],
            limits::MANIFEST_CONTRACT
        );
        assert_eq!(
            manifest["properties"]["pcm_format"]["const"],
            limits::PCM_FORMAT
        );
        assert_eq!(
            manifest["properties"]["qa_profile"]["const"],
            limits::QA_PROFILE
        );
        let max_bars = spec["definitions"]["bgm"]["properties"]["loop"]["properties"]
            ["end_bar_exclusive"]["maximum"]
            .as_u64()
            .unwrap();
        assert_eq!(max_bars, limits::MAX_BGM_BARS);
    }
}
