//! GD-3 layer CRUD.

use pxl_ase_doc::{BlendMode, Layer, LayerId, LayerKind, Sprite};

use crate::{AseErr, Result};

fn find_layer_mut<'a>(sprite: &'a mut Sprite, name: &str) -> Result<&'a mut Layer> {
    sprite
        .layers
        .iter_mut()
        .find(|l| l.name == name)
        .ok_or_else(|| AseErr(format!("layer '{name}' not found")))
}

pub fn layer_add(sprite: &mut Sprite, name: String) -> Result<LayerId> {
    if name.is_empty() {
        return Err(AseErr("layer name must be non-empty".into()));
    }
    if sprite.layers.iter().any(|l| l.name == name) {
        return Err(AseErr(format!("layer '{name}' already exists")));
    }
    Ok(sprite.add_image_layer(name))
}

pub fn layer_remove(sprite: &mut Sprite, name: &str) -> Result<()> {
    let pos = sprite
        .layers
        .iter()
        .position(|l| l.name == name)
        .ok_or_else(|| AseErr(format!("layer '{name}' not found")))?;
    if sprite.layers.len() <= 1 {
        return Err(AseErr("cannot remove the last layer".into()));
    }
    sprite.layers.remove(pos);
    Ok(())
}

pub fn layer_rename(sprite: &mut Sprite, old: &str, new: &str) -> Result<()> {
    if new.is_empty() {
        return Err(AseErr("new layer name must be non-empty".into()));
    }
    if sprite.layers.iter().any(|l| l.name == new) {
        return Err(AseErr(format!("layer '{new}' already exists")));
    }
    find_layer_mut(sprite, old)?.name = new.into();
    Ok(())
}

pub fn layer_set(
    sprite: &mut Sprite,
    name: &str,
    opacity: Option<u8>,
    blend: Option<BlendMode>,
    visible: Option<bool>,
) -> Result<()> {
    let layer = find_layer_mut(sprite, name)?;
    if let Some(o) = opacity {
        layer.opacity = o;
    }
    if let Some(b) = blend {
        layer.blend = b;
    }
    if let Some(v) = visible {
        layer.flags.visible = v;
    }
    let _ = LayerKind::Image;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pxl_ase_doc::ColorMode;

    #[test]
    fn layer_crud() {
        let mut s = Sprite::new(1, 1, ColorMode::Rgba);
        layer_add(&mut s, "a".into()).unwrap();
        layer_add(&mut s, "b".into()).unwrap();
        layer_set(&mut s, "a", Some(128), Some(BlendMode::Multiply), Some(false)).unwrap();
        assert_eq!(s.layers[0].opacity, 128);
        assert!(!s.layers[0].flags.visible);
        layer_rename(&mut s, "a", "aa").unwrap();
        layer_remove(&mut s, "aa").unwrap();
        assert_eq!(s.layers.len(), 1);
    }
}
