use pxl_core::{Palette, PxlError, PxlResult};
use pxl_format::{parse_palette, parse_template, TemplateFile};

pub struct Builtin {
    pub id: &'static str,
    pub json: &'static str,
}

const TEMPLATES: &[Builtin] = &[
    Builtin {
        id: "slime_8x8",
        json: include_str!("../../../assets/templates/slime_8x8.template.json"),
    },
    Builtin {
        id: "ship_8x12",
        json: include_str!("../../../assets/templates/ship_8x12.template.json"),
    },
    Builtin {
        id: "robot_6x10",
        json: include_str!("../../../assets/templates/robot_6x10.template.json"),
    },
    Builtin {
        id: "bug_8x8",
        json: include_str!("../../../assets/templates/bug_8x8.template.json"),
    },
    Builtin {
        id: "gem_6x6",
        json: include_str!("../../../assets/templates/gem_6x6.template.json"),
    },
    Builtin {
        id: "mushroom_8x8",
        json: include_str!("../../../assets/templates/mushroom_8x8.template.json"),
    },
    Builtin {
        id: "tree_8x12",
        json: include_str!("../../../assets/templates/tree_8x12.template.json"),
    },
    Builtin {
        id: "rock_6x6",
        json: include_str!("../../../assets/templates/rock_6x6.template.json"),
    },
    Builtin {
        id: "dragon_side_8x10",
        json: include_str!("../../../assets/templates/dragon_side_8x10.template.json"),
    },
    Builtin {
        id: "dragon_top_8x8",
        json: include_str!("../../../assets/templates/dragon_top_8x8.template.json"),
    },
    Builtin {
        id: "panel_4x8",
        json: include_str!("../../../assets/templates/panel_4x8.template.json"),
    },
];

const PALETTES: &[Builtin] = &[
    Builtin {
        id: "p16",
        json: include_str!("../../../assets/palettes/p16.palette.json"),
    },
    Builtin {
        id: "tank",
        json: include_str!("../../../assets/palettes/tank.palette.json"),
    },
];

pub fn template_ids() -> Vec<&'static str> {
    TEMPLATES.iter().map(|t| t.id).collect()
}

pub fn palette_ids() -> Vec<&'static str> {
    PALETTES.iter().map(|t| t.id).collect()
}

pub fn builtin_template(id: &str) -> PxlResult<TemplateFile> {
    let b = TEMPLATES.iter().find(|t| t.id == id).ok_or_else(|| {
        PxlError::semantic(format!("unknown template '{id}'"))
            .with_hint("pass a file path, or one of the ids from `pxl list templates`")
    })?;
    let mut t = parse_template(b.json)?;
    if t.id.is_empty() {
        t.id = id.to_string();
    }
    Ok(t)
}

pub fn load_builtin_mask(id: &str) -> PxlResult<crate::RuntimeMask> {
    let file = builtin_template(id)?;
    crate::RuntimeMask::from_file(&file, None)
}

pub fn builtin_palette(id: &str) -> PxlResult<Palette> {
    let b = PALETTES.iter().find(|t| t.id == id).ok_or_else(|| {
        PxlError::semantic(format!("unknown palette '{id}'"))
            .with_hint("pass a .palette.json path, or a built-in id from `pxl list palettes`")
    })?;
    parse_palette(b.json, id)
}
