//! Battle City slice.
//!
//! The simulation steps at a fixed 60 Hz. [`replay::run_replay`] drives that
//! clock from a script and writes PNGs. The `window` feature draws the same
//! indexed buffer through macroquad with nearest-neighbour filtering.

pub mod draw;
pub mod level;
pub mod palette;
pub mod pngutil;
pub mod replay;
pub mod sim;

pub use replay::{cli, run_replay, HeadlessRequest, PlayReport, Replay};

#[cfg(test)]
mod macos_window_patch {
    /// Locks the vendored miniquad 0.4.6 startup order. `center` used to run
    /// while `display.view` was still null; Apple Silicon then aborted in
    /// `update_dimensions` (`macos.rs:168`).
    #[test]
    fn center_runs_after_the_view_is_stored() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../vendor/miniquad/src/native/macos.rs");
        let src = std::fs::read_to_string(&path).unwrap_or_else(|err| {
            panic!("read {}: {err}", path.display());
        });
        let view_assign = src
            .find("display.view = view")
            .expect("display.view assignment");
        let center = src
            .find("msg_send![window, center]")
            .expect("center message");
        assert!(
            view_assign < center,
            "NSWindow center must run after display.view is stored"
        );
        assert!(
            src.contains("self.view.is_null() || self.window.is_null()"),
            "update_dimensions must return before messaging a nil view"
        );
        assert!(
            src.contains("if payload.gl_context.is_null()"),
            "windowDidMove must return before messaging a nil GL context"
        );
    }
}
