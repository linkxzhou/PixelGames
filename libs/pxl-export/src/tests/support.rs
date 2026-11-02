use pxl_core::{Palette, Rgba};

pub(crate) fn palette() -> Palette {
    // Local palette so export tests do not depend on pxl-gen.
    Palette {
        id: "t".into(),
        colors: vec![
            Rgba::TRANSPARENT,
            Rgba::rgb(20, 16, 12),
            Rgba::rgb(59, 74, 42),
            Rgba::rgb(90, 110, 58),
            Rgba::rgb(134, 160, 82),
            Rgba::rgb(185, 201, 138),
            Rgba::rgb(74, 64, 56),
        ],
        ramps: Default::default(),
        cycles: Default::default(),
        special: pxl_core::Special {
            transparent: 0,
            outline: 1,
            flash: 5,
        },
    }
}
