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
/// See `plan/v8.1/playback.md`. `repeat == 0` means play once on export
/// (UI infinite loop is not a batch multiplier). Ping-pong + `repeat == 1`
/// exports a single direction only.
pub fn expand_tag_frames(tag: &Tag) -> Vec<u16> {
    let forward: Vec<u16> = (tag.from..=tag.to).collect();
    let pingpong_one_way = tag.repeat == 1
        && matches!(
            tag.direction,
            TagDirection::PingPong | TagDirection::PingPongReverse
        );

    let mut base = match tag.direction {
        TagDirection::Forward => forward,
        TagDirection::Reverse => {
            let mut v = forward;
            v.reverse();
            v
        }
        TagDirection::PingPong if pingpong_one_way => forward,
        TagDirection::PingPongReverse if pingpong_one_way => {
            let mut v = forward;
            v.reverse();
            v
        }
        TagDirection::PingPong => append_pingpong_return(forward),
        TagDirection::PingPongReverse => {
            let mut v = forward;
            v.reverse();
            append_pingpong_return(v)
        }
    };

    let times = if tag.repeat <= 1 { 1 } else { tag.repeat as usize };
    if times > 1 {
        let once = base.clone();
        for _ in 1..times {
            base.extend_from_slice(&once);
        }
    }
    base
}

fn append_pingpong_return(mut base: Vec<u16>) -> Vec<u16> {
    if base.len() > 2 {
        let mut mid = base[1..base.len() - 1].to_vec();
        mid.reverse();
        base.extend(mid);
    }
    base
}

pub fn frames_for_tag(sprite: &Sprite, tag: Option<&str>) -> Result<Vec<u16>> {
    frames_for_tag_opts(sprite, tag, false)
}

/// Frame selection for a tag.
///
/// When `play_subtags` is true: best-effort over the flat [`Tag`] list (no
/// parent pointer in the model). Subtags = range-contained tags; expanded with
/// their own direction/repeat; gaps filled **forward** (parent direction is
/// not re-applied to the concatenation). See `plan/v8.1/playback.md`.
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



#[cfg(test)]
mod tests {
    use super::*;
    use pxl_ase_doc::{Tag, TagDirection};

    fn tag(dir: TagDirection, from: u16, to: u16, repeat: u16) -> Tag {
        Tag {
            name: "t".into(),
            from,
            to,
            direction: dir,
            repeat,
            user_data: Default::default(),
        }
    }

    #[test]
    fn pingpong_repeat0_is_roundtrip_once() {
        let f = expand_tag_frames(&tag(TagDirection::PingPong, 0, 3, 0));
        assert_eq!(f, vec![0, 1, 2, 3, 2, 1]);
    }

    #[test]
    fn pingpong_repeat1_is_one_direction() {
        let f = expand_tag_frames(&tag(TagDirection::PingPong, 0, 3, 1));
        assert_eq!(f, vec![0, 1, 2, 3]);
    }

    #[test]
    fn forward_repeat2_duplicates() {
        let f = expand_tag_frames(&tag(TagDirection::Forward, 1, 2, 2));
        assert_eq!(f, vec![1, 2, 1, 2]);
    }
}
