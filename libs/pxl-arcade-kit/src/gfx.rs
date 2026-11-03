//! Sheet textures and integer-pixel sprite drawing.

use std::collections::BTreeMap;

use macroquad::prelude::*;

use crate::package::{read_json, Asset, Frame, Package};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Anchor {
    TopLeft,
    #[default]
    Center,
    BottomCenter,
}

#[derive(Debug, Clone, Copy)]
pub struct DrawOpts {
    pub anchor: Anchor,
    pub flip_x: bool,
    pub flip_y: bool,
    /// Radians, clockwise; rotation happens around the sprite's centre.
    pub rotation: f32,
    pub scale: f32,
    pub color: Color,
}

impl Default for DrawOpts {
    fn default() -> Self {
        DrawOpts {
            anchor: Anchor::Center,
            flip_x: false,
            flip_y: false,
            rotation: 0.0,
            scale: 1.0,
            color: WHITE,
        }
    }
}

impl DrawOpts {
    pub fn centered() -> DrawOpts {
        DrawOpts::default()
    }

    pub fn bottom() -> DrawOpts {
        DrawOpts {
            anchor: Anchor::BottomCenter,
            ..DrawOpts::default()
        }
    }

    pub fn flipped(mut self, flip_x: bool, flip_y: bool) -> DrawOpts {
        self.flip_x = flip_x;
        self.flip_y = flip_y;
        self
    }

    pub fn tint(mut self, color: Color) -> DrawOpts {
        self.color = color;
        self
    }

    pub fn alpha(mut self, alpha: f32) -> DrawOpts {
        self.color.a = alpha;
        self
    }

    pub fn rotated(mut self, radians: f32) -> DrawOpts {
        self.rotation = radians;
        self
    }

    pub fn scaled(mut self, scale: f32) -> DrawOpts {
        self.scale = scale;
        self
    }
}

/// Uploaded sheet textures plus the per-sheet frame tables.
pub struct Gfx {
    textures: BTreeMap<String, Texture2D>,
    contents: BTreeMap<String, Frame>,
}

impl Gfx {
    pub fn new(package: &Package) -> Result<Gfx, String> {
        let mut textures = BTreeMap::new();
        for (id, sheet) in &package.sheets {
            let image = Image {
                bytes: sheet.rgba.clone(),
                width: sheet.w as u16,
                height: sheet.h as u16,
            };
            let texture = Texture2D::from_image(&image);
            texture.set_filter(FilterMode::Nearest);
            textures.insert(id.clone(), texture);
        }
        let mut contents = BTreeMap::new();
        for asset in package.assets.values() {
            if let Some(content) = asset.content {
                contents.insert(asset.id.clone(), content);
            }
        }
        Ok(Gfx { textures, contents })
    }

    pub fn texture(&self, sheet_id: &str) -> Option<&Texture2D> {
        self.textures.get(sheet_id)
    }

    pub fn content(&self, asset_id: &str) -> Option<Frame> {
        self.contents.get(asset_id).copied()
    }

    /// Draw one animation frame. `elapsed_ms` is the animation clock; a static
    /// one-frame animation ignores it.
    pub fn sprite(
        &self,
        package: &Package,
        asset_id: &str,
        anim: &str,
        elapsed_ms: u64,
        x: f32,
        y: f32,
        opts: DrawOpts,
    ) {
        let Ok(asset) = package.asset(asset_id) else {
            return;
        };
        let Some(animation) = asset.anim(anim) else {
            return;
        };
        let Some(key) = animation.key(elapsed_ms) else {
            return;
        };
        self.frame_key(package, asset, key, x, y, opts);
    }

    /// Draw a named frame (frame keys are visible in `*.sheet.json`).
    pub fn frame(&self, package: &Package, asset_id: &str, key: &str, x: f32, y: f32, opts: DrawOpts) {
        let Ok(asset) = package.asset(asset_id) else {
            return;
        };
        self.frame_key(package, asset, key, x, y, opts);
    }

    pub fn frame_key(&self, package: &Package, asset: &Asset, key: &str, x: f32, y: f32, opts: DrawOpts) {
        let Ok(sheet) = package.sheet_of(asset) else {
            return;
        };
        let Some(texture) = self.textures.get(&asset.sheet) else {
            return;
        };
        let Some(source) = sheet.frame(key) else {
            return;
        };
        let content = asset.content.unwrap_or(Frame {
            x: 0,
            y: 0,
            w: source.w,
            h: source.h,
        });
        let w = content.w as f32 * opts.scale;
        let h = content.h as f32 * opts.scale;
        let (dx, dy) = match opts.anchor {
            Anchor::TopLeft => (x, y),
            Anchor::Center => (x - w / 2.0, y - h / 2.0),
            Anchor::BottomCenter => (x - w / 2.0, y - h),
        };
        blit(
            texture,
            Rect::new(
                (source.x + content.x) as f32,
                (source.y + content.y) as f32,
                content.w as f32,
                content.h as f32,
            ),
            Rect::new(dx, dy, w, h),
            opts,
        );
    }

    /// Draw a rectangle of a sheet directly; the HUD font uses this.
    pub fn blit_rect(&self, sheet_id: &str, source: Rect, dest: Rect, color: Color) {
        if let Some(texture) = self.textures.get(sheet_id) {
            let opts = DrawOpts {
                color,
                ..DrawOpts::default()
            };
            blit(texture, source, dest, opts);
        }
    }

    pub fn tint_color(package: &Package, index: u8) -> Color {
        let rgba = package.palette.rgba(index);
        Color::new(
            rgba[0] as f32 / 255.0,
            rgba[1] as f32 / 255.0,
            rgba[2] as f32 / 255.0,
            rgba[3] as f32 / 255.0,
        )
    }
}

pub fn blit(texture: &Texture2D, source: Rect, dest: Rect, opts: DrawOpts) {
    draw_texture_ex(
        texture,
        dest.x,
        dest.y,
        opts.color,
        DrawTextureParams {
            dest_size: Some(vec2(dest.w, dest.h)),
            source: Some(source),
            rotation: opts.rotation,
            flip_x: opts.flip_x,
            flip_y: opts.flip_y,
            pivot: None,
        },
    );
}

/// The `hud_font` sheet: 16 glyphs per row, 8×8 cells, ASCII from 32.
pub struct FontSheet {
    pub sheet: String,
    pub cell: u32,
    pub columns: u32,
}

pub fn font_sheet(package: &Package, asset_id: &str) -> Option<FontSheet> {
    let asset = package.assets.get(asset_id)?;
    let sheet = package.sheets.get(&asset.sheet)?;
    // The font is authored on a 128×128 canvas with 8×8 cells; derive the
    // layout from the sheet so a larger font stays usable.
    let cell = 8;
    Some(FontSheet {
        sheet: sheet.id.clone(),
        cell,
        columns: (sheet.w / cell).max(1),
    })
}

/// Read a JSON file from a package (level extras live here).
pub fn package_json(package: &Package, rel: &str) -> Option<serde_json::Value> {
    read_json(&crate::package::resolve(&package.root, rel)).ok()
}
