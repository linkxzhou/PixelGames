//! Audio placeholder.
//!
//! The pxl package carries rendered WAV files, but the pure Rust runtime ships
//! without an audio backend: `engine.rust.json` declares `audio.rust: false`, so
//! step-500 reports packaged audio as a CONCERNS warning rather than a pass.
//! Games call these no-ops so wiring audio later does not touch gameplay code.

use crate::package::Package;

#[derive(Debug, Default)]
pub struct SoundBank {
    enabled: bool,
    last: Option<String>,
}

impl SoundBank {
    pub fn load(_package: &Package) -> SoundBank {
        SoundBank::default()
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn play(&mut self, id: &str) {
        if self.enabled {
            self.last = Some(id.to_string());
        }
    }

    pub fn last(&self) -> Option<&str> {
        self.last.as_deref()
    }
}
