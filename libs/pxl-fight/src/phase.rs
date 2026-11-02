//! A compact, deterministic 2D versus fighter.
//!
//! Two fighters stand on one arena floor. Attacks are frame-windowed (startup,
//! active, recovery), blocking chips damage, throws ignore block and a motion
//! command fires a projectile. Rounds are best of three and a ladder of
//! opponents advances the match.

use pxl_arcade_kit::input::{Dir4, InputState};
use pxl_arcade_kit::replay::StateHash;
use pxl_arcade_kit::rng::Rng;


use super::types::Phase;

impl Phase {
    pub fn as_u8(self) -> u8 {
        match self {
            Phase::Intro(_) => 0,
            Phase::Fight => 1,
            Phase::RoundEnd(_) => 2,
            Phase::MatchEnd(_) => 3,
        }
    }
}

impl std::convert::From<Phase> for u8 {
    fn from(phase: Phase) -> u8 {
        phase.as_u8()
    }
}
