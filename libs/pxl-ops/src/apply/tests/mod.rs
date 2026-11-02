use std::cell::RefCell;
use std::collections::BTreeMap;

use super::ApplyCx;
use pxl_core::{IndexFrame, Palette};

mod color;
mod pixels;
mod transform;

fn grid(rows: &[&str]) -> IndexFrame {
    let owned: Vec<String> = rows.iter().map(|s| (*s).to_string()).collect();
    pxl_core::decode_matrix(&owned).unwrap()
}

fn text(frame: &IndexFrame) -> Vec<String> {
    pxl_core::encode_matrix(frame).unwrap()
}

fn pal() -> Palette {
    pxl_gen::builtin_palette("tank").unwrap()
}

fn cx(pal: &Palette) -> ApplyCx<'_> {
    ApplyCx {
        palette: pal,
        seed: 1,
        frame_name: "base",
        load_template: None,
        parts: None,
        pose: RefCell::new(BTreeMap::new()),
    }
}
