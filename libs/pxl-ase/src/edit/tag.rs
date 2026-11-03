//! GD-2 tag CRUD.

use pxl_ase_doc::{Sprite, Tag, TagDirection, UserData};

use crate::{AseErr, Result};

pub fn tag_add(
    sprite: &mut Sprite,
    name: String,
    from: u16,
    to: u16,
    direction: TagDirection,
    repeat: u16,
) -> Result<()> {
    if name.is_empty() {
        return Err(AseErr("tag name must be non-empty".into()));
    }
    if from > to {
        return Err(AseErr(format!("tag range {from}..={to} inverted")));
    }
    let n = sprite.frame_count() as u16;
    if to >= n {
        return Err(AseErr(format!("tag to={to} past last frame {}", n.saturating_sub(1))));
    }
    if sprite.tags.iter().any(|t| t.name == name) {
        return Err(AseErr(format!("tag '{name}' already exists")));
    }
    sprite.tags.push(Tag {
        name,
        from,
        to,
        direction,
        repeat,
        user_data: UserData::default(),
    });
    Ok(())
}

pub fn tag_remove(sprite: &mut Sprite, name: &str) -> Result<()> {
    let before = sprite.tags.len();
    sprite.tags.retain(|t| t.name != name);
    if sprite.tags.len() == before {
        return Err(AseErr(format!("tag '{name}' not found")));
    }
    Ok(())
}

pub fn tag_rename(sprite: &mut Sprite, old: &str, new: &str) -> Result<()> {
    if new.is_empty() {
        return Err(AseErr("new tag name must be non-empty".into()));
    }
    if sprite.tags.iter().any(|t| t.name == new) {
        return Err(AseErr(format!("tag '{new}' already exists")));
    }
    let t = sprite
        .tags
        .iter_mut()
        .find(|t| t.name == old)
        .ok_or_else(|| AseErr(format!("tag '{old}' not found")))?;
    t.name = new.into();
    Ok(())
}

pub fn tag_set(
    sprite: &mut Sprite,
    name: &str,
    from: Option<u16>,
    to: Option<u16>,
    direction: Option<TagDirection>,
    repeat: Option<u16>,
) -> Result<()> {
    let n = sprite.frame_count() as u16;
    let t = sprite
        .tags
        .iter_mut()
        .find(|t| t.name == name)
        .ok_or_else(|| AseErr(format!("tag '{name}' not found")))?;
    if let Some(f) = from {
        t.from = f;
    }
    if let Some(tt) = to {
        t.to = tt;
    }
    if t.from > t.to {
        return Err(AseErr(format!("tag range {}..={} inverted", t.from, t.to)));
    }
    if t.to >= n {
        return Err(AseErr(format!("tag to={} past last frame", t.to)));
    }
    if let Some(d) = direction {
        t.direction = d;
    }
    if let Some(r) = repeat {
        t.repeat = r;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pxl_ase_doc::ColorMode;

    #[test]
    fn tag_crud() {
        let mut s = Sprite::new(1, 1, ColorMode::Rgba);
        s.frame_durations_ms = vec![100, 100, 100];
        tag_add(&mut s, "a".into(), 0, 1, TagDirection::Forward, 0).unwrap();
        tag_set(&mut s, "a", None, Some(2), Some(TagDirection::PingPong), Some(1)).unwrap();
        assert_eq!(s.tags[0].to, 2);
        assert_eq!(s.tags[0].direction, TagDirection::PingPong);
        tag_rename(&mut s, "a", "b").unwrap();
        tag_remove(&mut s, "b").unwrap();
        assert!(s.tags.is_empty());
    }
}
