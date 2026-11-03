//! GD-1 frame add/remove/copy/set-duration + tag-boundary rules (plan 07 §10).

use std::collections::BTreeMap;

use pxl_ase_doc::{Cel, FrameIndex, Sprite};

use crate::{AseErr, Result};

fn shift_cel_keys(cels: &mut BTreeMap<FrameIndex, Cel>, from: FrameIndex, delta: i32) {
    if delta == 0 {
        return;
    }
    let keys: Vec<FrameIndex> = cels.keys().copied().collect();
    let mut moved = BTreeMap::new();
    for k in keys {
        let cel = cels.remove(&k).unwrap();
        let nk = if k >= from {
            ((k as i32) + delta).max(0) as FrameIndex
        } else {
            k
        };
        // Remap linked targets that point at/after `from`.
        let cel = match cel {
            Cel::Linked { frame, z_index, user_data, extra } => {
                let frame = if frame >= from {
                    ((frame as i32) + delta).max(0) as FrameIndex
                } else {
                    frame
                };
                Cel::Linked { frame, z_index, user_data, extra }
            }
            other => other,
        };
        moved.insert(nk, cel);
    }
    *cels = moved;
}

/// Plan 07 §10 delete-frame tag rules. Returns removed tag names.
pub fn adjust_tags_on_remove(sprite: &mut Sprite, i: FrameIndex) -> Vec<String> {
    let mut dropped = Vec::new();
    sprite.tags.retain_mut(|t| {
        if i < t.from {
            t.from = t.from.saturating_sub(1);
            t.to = t.to.saturating_sub(1);
            true
        } else if i >= t.from && i <= t.to {
            if t.to == t.from {
                dropped.push(t.name.clone());
                false
            } else {
                t.to = t.to.saturating_sub(1);
                true
            }
        } else {
            true
        }
    });
    dropped
}

/// Plan 07 §10 insert-frame tag rules.
pub fn adjust_tags_on_insert(sprite: &mut Sprite, i: FrameIndex) {
    for t in &mut sprite.tags {
        if i <= t.from {
            t.from = t.from.saturating_add(1);
            t.to = t.to.saturating_add(1);
        } else if i > t.from && i <= t.to {
            t.to = t.to.saturating_add(1);
        }
    }
}

/// Insert empty frame at `at` (0..=frame_count). Duration defaults to 100ms.
pub fn frame_add(sprite: &mut Sprite, at: FrameIndex, duration_ms: Option<u16>) -> Result<()> {
    let n = sprite.frame_count();
    if at as usize > n {
        return Err(AseErr(format!("frame index {at} out of range 0..={n}")));
    }
    let dur = duration_ms.unwrap_or(100);
    sprite.frame_durations_ms.insert(at as usize, dur);
    for layer in &mut sprite.layers {
        shift_cel_keys(&mut layer.cels, at, 1);
    }
    adjust_tags_on_insert(sprite, at);
    Ok(())
}

pub fn frame_remove(sprite: &mut Sprite, index: FrameIndex) -> Result<Vec<String>> {
    let n = sprite.frame_count();
    if n <= 1 {
        return Err(AseErr("cannot remove the last frame".into()));
    }
    if index as usize >= n {
        return Err(AseErr(format!("frame index {index} out of range 0..{}", n - 1)));
    }
    sprite.frame_durations_ms.remove(index as usize);
    for layer in &mut sprite.layers {
        layer.cels.remove(&index);
        shift_cel_keys(&mut layer.cels, index.saturating_add(1), -1);
    }
    Ok(adjust_tags_on_remove(sprite, index))
}

pub fn frame_copy(sprite: &mut Sprite, from: FrameIndex, to: FrameIndex) -> Result<()> {
    let n = sprite.frame_count();
    if from as usize >= n {
        return Err(AseErr(format!("source frame {from} out of range")));
    }
    // Insert at `to`, then copy cel content from the shifted source.
    frame_add(sprite, to, Some(sprite.frame_durations_ms[from as usize]))?;
    // After insert at `to`, original `from` may have shifted.
    let src = if from >= to { from + 1 } else { from };
    // Snapshot source cels, then deep-copy image buffers without overlapping borrows.
    let sources: Vec<_> = sprite
        .layers
        .iter()
        .map(|l| l.cels.get(&src).cloned())
        .collect();
    for (layer, maybe) in sprite.layers.iter_mut().zip(sources) {
        let Some(cel) = maybe else { continue };
        let cel = match cel {
            Cel::Image {
                image,
                x,
                y,
                opacity,
                z_index,
                user_data,
                extra,
            } => Cel::Image {
                image,
                x,
                y,
                opacity,
                z_index,
                user_data,
                extra,
            },
            other => other,
        };
        layer.cels.insert(to, cel);
    }
    // Second pass: duplicate image buffers for Image cels at `to`.
    let image_ids: Vec<_> = sprite
        .layers
        .iter()
        .filter_map(|l| match l.cels.get(&to) {
            Some(Cel::Image { image, .. }) => Some(*image),
            _ => None,
        })
        .collect();
    let mut new_ids = Vec::new();
    for id in image_ids {
        if let Some(buf) = sprite.images.get(&id).cloned() {
            new_ids.push(Some(sprite.insert_image(buf)));
        } else {
            new_ids.push(None);
        }
    }
    let mut ni = 0usize;
    for layer in &mut sprite.layers {
        if let Some(Cel::Image { image, .. }) = layer.cels.get_mut(&to) {
            if let Some(Some(nid)) = new_ids.get(ni) {
                *image = *nid;
            }
            ni += 1;
        }
    }
    Ok(())
}

pub fn frame_set_duration(sprite: &mut Sprite, index: FrameIndex, ms: u16) -> Result<()> {
    let n = sprite.frame_count();
    if index as usize >= n {
        return Err(AseErr(format!("frame index {index} out of range")));
    }
    if ms == 0 {
        return Err(AseErr("duration must be > 0".into()));
    }
    sprite.frame_durations_ms[index as usize] = ms;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pxl_ase_doc::{ColorMode, Tag, TagDirection, UserData};

    fn tagged() -> Sprite {
        let mut s = Sprite::new(2, 2, ColorMode::Rgba);
        s.frame_durations_ms = vec![100, 100, 100, 100];
        s.tags.push(Tag {
            name: "walk".into(),
            from: 1,
            to: 2,
            direction: TagDirection::Forward,
            repeat: 0,
            user_data: UserData::default(),
        });
        s
    }

    #[test]
    fn tag_bounds_on_insert_and_remove() {
        let mut s = tagged();
        adjust_tags_on_insert(&mut s, 0); // before tag
        assert_eq!((s.tags[0].from, s.tags[0].to), (2, 3));
        let mut s = tagged();
        adjust_tags_on_insert(&mut s, 2); // inside (from < i <= to)
        assert_eq!((s.tags[0].from, s.tags[0].to), (1, 3));
        let mut s = tagged();
        adjust_tags_on_insert(&mut s, 3); // after
        assert_eq!((s.tags[0].from, s.tags[0].to), (1, 2));

        let mut s = tagged();
        adjust_tags_on_remove(&mut s, 0);
        assert_eq!((s.tags[0].from, s.tags[0].to), (0, 1));
        let mut s = tagged();
        let dropped = adjust_tags_on_remove(&mut s, 1);
        assert!(dropped.is_empty());
        assert_eq!((s.tags[0].from, s.tags[0].to), (1, 1));
        let mut s = tagged();
        s.tags[0].to = 1;
        let dropped = adjust_tags_on_remove(&mut s, 1);
        assert_eq!(dropped, vec!["walk".to_string()]);
        assert!(s.tags.is_empty());
    }

    #[test]
    fn add_remove_copy_duration() {
        let mut s = Sprite::new(1, 1, ColorMode::Rgba);
        s.frame_durations_ms = vec![50, 60];
        frame_add(&mut s, 1, Some(70)).unwrap();
        assert_eq!(s.frame_durations_ms, vec![50, 70, 60]);
        frame_set_duration(&mut s, 0, 40).unwrap();
        assert_eq!(s.frame_durations_ms[0], 40);
        frame_remove(&mut s, 1).unwrap();
        assert_eq!(s.frame_durations_ms, vec![40, 60]);
        frame_copy(&mut s, 0, 2).unwrap();
        assert_eq!(s.frame_count(), 3);
        assert_eq!(s.frame_durations_ms[2], 40);
    }
}
