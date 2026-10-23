use std::collections::BTreeMap;

use pxl_core::{colors_from_hex, Cycle, IndexFrame, Palette, Special};

use crate::{validate, ValidateInput};

mod animation;
mod profiles;
mod rules;

fn pal() -> Palette {
    let colors = ["#00000000", "#14100c", "#4a4038", "#9a8c7a"]
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let mut ramps = BTreeMap::new();
    ramps.insert("tread".into(), vec![1, 2]);
    ramps.insert("ground".into(), vec![2, 3]);
    let mut cycles = BTreeMap::new();
    cycles.insert(
        "tread".into(),
        Cycle {
            indices: vec![2, 1],
            period: 2,
        },
    );
    Palette {
        id: "t".into(),
        colors: colors_from_hex(&colors).unwrap(),
        ramps,
        cycles,
        special: Special {
            transparent: 0,
            outline: 1,
            flash: 3,
        },
    }
}

#[test]
fn out_of_palette_pixel_fails() {
    let palette = pal();
    let mut frame = IndexFrame::new(4, 4).unwrap();
    frame.set(1, 1, 9);
    let input = ValidateInput {
        palette,
        frames: vec![("base".into(), frame)],
        anims: vec![],
        facings: BTreeMap::new(),
        tiles: vec![],
        tile_ramp: "ground".into(),
        tile_ramps: BTreeMap::new(),
        asset_kinds: BTreeMap::new(),
        frame_assets: BTreeMap::new(),
    };
    let report = validate(&input, "default").unwrap();
    let check = report
        .checks
        .iter()
        .find(|c| c.id == "frame.base.palette")
        .unwrap();
    assert!(!check.pass);
    assert!(!report.ok());
}
