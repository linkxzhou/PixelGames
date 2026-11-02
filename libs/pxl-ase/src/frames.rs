//! Frame range / tag selection helpers.
use pxl_ase_doc::{Sprite, Tag, TagDirection};
use crate::{AseErr, Result};

pub fn parse_frame_range(spec: &str, frame_count: u16) -> Result<Vec<u16>> {
    let mut out = Vec::new();
    for part in spec.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((a, b)) = part.split_once('-') {
            let from: u16 = a
                .trim()
                .parse()
                .map_err(|_| AseErr(format!("bad frame-range '{spec}'")))?;
            let to: u16 = b
                .trim()
                .parse()
                .map_err(|_| AseErr(format!("bad frame-range '{spec}'")))?;
            if from > to {
                return Err(AseErr(format!("frame-range from>to in '{part}'")));
            }
            for f in from..=to {
                if f < frame_count {
                    out.push(f);
                }
            }
        } else {
            let f: u16 = part
                .parse()
                .map_err(|_| AseErr(format!("bad frame-range '{spec}'")))?;
            if f < frame_count {
                out.push(f);
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    if out.is_empty() {
        return Err(AseErr(format!("frame-range '{spec}' selected no frames")));
    }
    Ok(out)
}

/// Expand a tag's `[from, to]` range honoring `direction` and finite `repeat`.
///
/// `repeat == 0` means "play once" for export (Aseprite's infinite loop is a
/// playback concern, not a batch export multiplier).
pub fn expand_tag_frames(tag: &Tag) -> Vec<u16> {
    let mut base: Vec<u16> = (tag.from..=tag.to).collect();
    match tag.direction {
        TagDirection::Forward => {}
        TagDirection::Reverse => base.reverse(),
        TagDirection::PingPong => {
            if base.len() > 2 {
                let mut mid = base[1..base.len() - 1].to_vec();
                mid.reverse();
                base.extend(mid);
            } else if base.len() == 2 {
                // A-B already; pingpong mid empty
            }
        }
        TagDirection::PingPongReverse => {
            base.reverse();
            if base.len() > 2 {
                let mut mid = base[1..base.len() - 1].to_vec();
                mid.reverse();
                base.extend(mid);
            }
        }
    }
    if tag.repeat > 1 {
        let once = base.clone();
        for _ in 1..tag.repeat {
            base.extend_from_slice(&once);
        }
    }
    base
}

pub fn frames_for_tag(sprite: &Sprite, tag: Option<&str>) -> Result<Vec<u16>> {
    frames_for_tag_opts(sprite, tag, false)
}

/// Frame selection for a tag.
///
/// When `play_subtags` is true: best-effort over the flat [`Tag`] list (the
/// doc model has no parent pointer). Nested/sub tags are inferred by **range
/// containment** — any tag whose `[from,to]` lies strictly inside the selected
/// tag is treated as a child. Children are expanded (with their own direction)
/// in ascending `from` order; overlapping siblings keep the earlier `from`
/// (then shorter span). Gaps between children are filled with the parent
/// direction's remaining frames. Limits: no true hierarchy metadata; overlapping
/// ranges are heuristic; infinite repeat is not expanded.
pub fn frames_for_tag_opts(
    sprite: &Sprite,
    tag: Option<&str>,
    play_subtags: bool,
) -> Result<Vec<u16>> {
    if let Some(name) = tag {
        let parent = sprite
            .tags
            .iter()
            .find(|t| t.name == name)
            .ok_or_else(|| AseErr(format!("tag '{name}' not found")))?;
        if !play_subtags {
            return Ok(expand_tag_frames(parent));
        }
        let mut children: Vec<&Tag> = sprite
            .tags
            .iter()
            .filter(|t| {
                t.name != parent.name
                    && t.from >= parent.from
                    && t.to <= parent.to
                    && (t.from > parent.from || t.to < parent.to)
            })
            .collect();
        if children.is_empty() {
            return Ok(expand_tag_frames(parent));
        }
        children.sort_by_key(|t| (t.from, t.to.saturating_sub(t.from), &t.name));
        // Drop a child fully contained in an earlier-accepted sibling.
        let mut accepted: Vec<&Tag> = Vec::new();
        for c in children {
            let nested = accepted
                .iter()
                .any(|a| c.from >= a.from && c.to <= a.to);
            if !nested {
                accepted.push(c);
            }
        }
        let mut out = Vec::new();
        let mut cursor = parent.from;
        for c in &accepted {
            while cursor < c.from {
                out.push(cursor);
                cursor += 1;
            }
            out.extend(expand_tag_frames(c));
            cursor = c.to.saturating_add(1);
        }
        while cursor <= parent.to {
            out.push(cursor);
            cursor += 1;
        }
        // If parent is reverse/pingpong, we only filled forward gaps; document
        // that play-subtags prefers child order over re-applying parent direction
        // on the concatenated result.
        if out.is_empty() {
            return Err(AseErr(format!("tag '{name}' with --play-subtags selected no frames")));
        }
        Ok(out)
    } else {
        Ok((0..sprite.frame_count() as u16).collect())
    }
}

pub fn select_frames(
    sprite: &Sprite,
    tag: Option<&str>,
    frame_range: Option<&str>,
    single_frame: Option<u16>,
) -> Result<Vec<u16>> {
    select_frames_opts(sprite, tag, frame_range, single_frame, false)
}

pub fn select_frames_opts(
    sprite: &Sprite,
    tag: Option<&str>,
    frame_range: Option<&str>,
    single_frame: Option<u16>,
    play_subtags: bool,
) -> Result<Vec<u16>> {
    if let Some(spec) = frame_range {
        return parse_frame_range(spec, sprite.frame_count() as u16);
    }
    if tag.is_some() {
        return frames_for_tag_opts(sprite, tag, play_subtags);
    }
    if let Some(f) = single_frame {
        if f as usize >= sprite.frame_count() {
            return Err(AseErr(format!("frame {f} out of range")));
        }
        return Ok(vec![f]);
    }
    frames_for_tag_opts(sprite, None, false)
}

/// True if every pixel is fully transparent (alpha < 8).
pub fn frame_is_empty(rgba: &[u8]) -> bool {
    rgba.chunks_exact(4).all(|c| c[3] < 8)
}

