//! Tidying a scaled frame into something an author would have drawn by hand.
//! Every rule reports how many pixels it touched and refuses to work on kept
//! features.

use alloc::vec;
use alloc::vec::Vec;

use pxl_core::IndexFrame;

use crate::analyze::{Axis, SpriteGraph};
use crate::metrics::{
    irregular_runs, silhouette_profiles, PROFILE_BOTTOM, PROFILE_LEFT, PROFILE_RIGHT, PROFILE_TOP,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CleanOptions {
    pub orphans: bool,
    pub elbows: bool,
    pub notches: bool,
    pub rhythm: bool,
    pub symmetry: bool,
}

impl CleanOptions {
    pub const NONE: Self = Self {
        orphans: false,
        elbows: false,
        notches: false,
        rhythm: false,
        symmetry: false,
    };
    pub const ALL: Self = Self {
        orphans: true,
        elbows: true,
        notches: true,
        rhythm: true,
        symmetry: true,
    };
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CleanReport {
    pub orphans: u32,
    pub elbows: u32,
    pub notches: u32,
    pub rhythm: u32,
    pub symmetry: u32,
}

impl CleanReport {
    pub fn changed(&self) -> u32 {
        self.orphans + self.elbows + self.notches + self.rhythm + self.symmetry
    }
}

/// Run the requested rules, in the order fixed here so two runs of the same
/// input always end with the same pixels.
pub fn clean(
    frame: &mut IndexFrame,
    native: &IndexFrame,
    k: u16,
    graph: &SpriteGraph,
    options: CleanOptions,
) -> CleanReport {
    let introduced = if options.symmetry && graph.symmetry.is_some() {
        introduced_blocks(frame, native, k)
    } else {
        Vec::new()
    };
    let mut report = CleanReport::default();
    let editable = |x: i32, y: i32| graph.editable(x / k as i32, y / k as i32);
    if options.orphans {
        report.orphans = remove_orphans(frame, &editable);
    }
    if options.elbows {
        report.elbows = remove_elbows(frame, &editable);
    }
    if options.notches {
        report.notches = fill_notches(frame, &editable);
    }
    if options.rhythm {
        report.rhythm = repair_rhythm(frame, k, graph);
    }
    if options.symmetry {
        report.symmetry = repair_symmetry(frame, native, k, graph, &introduced);
    }
    report
}

/// A visible pixel with no neighbour of its own colour is noise at any scale.
fn remove_orphans(frame: &mut IndexFrame, editable: &dyn Fn(i32, i32) -> bool) -> u32 {
    let mut kill = Vec::new();
    for y in 0..frame.h as i32 {
        for x in 0..frame.w as i32 {
            let c = frame.get(x, y);
            if c == 0 || !editable(x, y) {
                continue;
            }
            let alone = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                .iter()
                .all(|(dx, dy)| frame.get(x + dx, y + dy) != c);
            if alone {
                kill.push((x, y));
            }
        }
    }
    for (x, y) in &kill {
        frame.set(*x, *y, 0);
    }
    kill.len() as u32
}

/// The elbow of a one-pixel line: two same-colour neighbours at right angles.
/// Dropping the corner pixel leaves the two ends diagonally connected, which is
/// what a hand-drawn diagonal looks like.
fn remove_elbows(frame: &mut IndexFrame, editable: &dyn Fn(i32, i32) -> bool) -> u32 {
    let mut kill = Vec::new();
    for y in 0..frame.h as i32 {
        for x in 0..frame.w as i32 {
            let c = frame.get(x, y);
            if c == 0 || !editable(x, y) {
                continue;
            }
            for (a, b) in [
                ((-1, 0), (0, -1)),
                ((1, 0), (0, -1)),
                ((-1, 0), (0, 1)),
                ((1, 0), (0, 1)),
            ] {
                let na = frame.get(x + a.0, y + a.1) == c;
                let nb = frame.get(x + b.0, y + b.1) == c;
                let corner = frame.get(x + a.0 + b.0, y + a.1 + b.1) == c;
                if na && nb && !corner {
                    kill.push((x, y));
                    break;
                }
            }
        }
    }
    for (x, y) in &kill {
        frame.set(*x, *y, 0);
    }
    kill.len() as u32
}

/// A clear pixel with three same-colour sides is a hole in a surface, not a
/// feature.
fn fill_notches(frame: &mut IndexFrame, editable: &dyn Fn(i32, i32) -> bool) -> u32 {
    let mut fill = Vec::new();
    for y in 0..frame.h as i32 {
        for x in 0..frame.w as i32 {
            if frame.get(x, y) != 0 || !editable(x, y) {
                continue;
            }
            let sides = [
                frame.get(x - 1, y),
                frame.get(x + 1, y),
                frame.get(x, y - 1),
                frame.get(x, y + 1),
            ];
            // Most common non-clear side, if it holds at least three sides.
            let mut winner = 0u8;
            let mut best = 0usize;
            for (i, c) in sides.iter().enumerate() {
                if *c == 0 {
                    continue;
                }
                let n = sides.iter().skip(i).filter(|o| *o == c).count();
                if n > best {
                    best = n;
                    winner = *c;
                }
            }
            if best >= 3 {
                fill.push((x, y, winner));
            }
        }
    }
    for (x, y, c) in &fill {
        frame.set(*x, *y, *c);
    }
    fill.len() as u32
}

#[derive(Clone, Copy, Debug)]
struct ProfileRun {
    position: i32,
    length: i32,
    start: usize,
}

fn repair_rhythm(frame: &mut IndexFrame, k: u16, graph: &SpriteGraph) -> u32 {
    let mut changed = 0;
    for side in [PROFILE_LEFT, PROFILE_RIGHT, PROFILE_TOP, PROFILE_BOTTOM] {
        let profiles = silhouette_profiles(frame);
        let mut repaired = profiles[side].clone();
        regularize_profile(&mut repaired);
        changed += apply_profile(frame, &profiles, side, &repaired, k, graph);
    }
    changed
}

fn regularize_profile(profile: &mut [Option<i32>]) {
    let source = profile.to_vec();
    let mut runs: Vec<ProfileRun> = Vec::new();
    for (index, entry) in source.iter().enumerate() {
        match entry {
            Some(position) => match runs.last_mut() {
                Some(last) if last.position == *position => last.length += 1,
                _ => runs.push(ProfileRun {
                    position: *position,
                    length: 1,
                    start: index,
                }),
            },
            None => {
                regularize_runs(profile, &runs);
                runs.clear();
            }
        }
    }
    regularize_runs(profile, &runs);
}

fn regularize_runs(profile: &mut [Option<i32>], runs: &[ProfileRun]) {
    let mut i = 0;
    while i + 1 < runs.len() {
        let direction = runs[i + 1].position - runs[i].position;
        if direction.abs() != 1 {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j + 1 < runs.len() && runs[j + 1].position - runs[j].position == direction {
            j += 1;
        }
        let scored: Vec<(i32, i32)> = runs[i..=j]
            .iter()
            .map(|run| (run.position, run.length))
            .collect();
        let count = j.saturating_sub(i + 1);
        if count >= 3 && irregular_runs(&scored) > 0 {
            let total: usize = runs[i + 1..j].iter().map(|run| run.length as usize).sum();
            let cursor = runs[i + 1].start;
            for (offset, run) in runs[i + 1..j].iter().enumerate() {
                let begin = (offset * total + count / 2) / count;
                let end = ((offset + 1) * total + count / 2) / count;
                for index in cursor + begin..cursor + end {
                    profile[index] = Some(run.position);
                }
            }
            debug_assert_eq!(runs[i + 1].start + total, runs[j].start);
        }
        i = j;
    }
}

fn apply_profile(
    frame: &mut IndexFrame,
    profiles: &[Vec<Option<i32>>; 4],
    side: usize,
    repaired: &[Option<i32>],
    k: u16,
    graph: &SpriteGraph,
) -> u32 {
    let kk = k.max(1) as i32;
    let mut edits = Vec::new();
    for (index, (old, new)) in profiles[side].iter().zip(repaired).enumerate() {
        let (Some(old), Some(new)) = (*old, *new) else {
            continue;
        };
        if old == new {
            continue;
        }
        let crosses_opposite = match side {
            PROFILE_LEFT => new > old && profiles[PROFILE_RIGHT][index].is_some_and(|v| new > v),
            PROFILE_RIGHT => new < old && profiles[PROFILE_LEFT][index].is_some_and(|v| new < v),
            PROFILE_TOP => new > old && profiles[PROFILE_BOTTOM][index].is_some_and(|v| new > v),
            PROFILE_BOTTOM => new < old && profiles[PROFILE_TOP][index].is_some_and(|v| new < v),
            _ => true,
        };
        if crosses_opposite {
            continue;
        }
        let adding = matches!(side, PROFILE_LEFT | PROFILE_TOP) && new < old
            || matches!(side, PROFILE_RIGHT | PROFILE_BOTTOM) && new > old;
        let line = index as i32;
        let (old_x, old_y) = match side {
            PROFILE_LEFT | PROFILE_RIGHT => (old, line),
            _ => (line, old),
        };
        let color = if adding { frame.get(old_x, old_y) } else { 0 };
        let mut local = Vec::new();
        let mut valid = color != 0 || !adding;
        for position in old.min(new)..old.max(new) {
            let (x, y) = match side {
                PROFILE_LEFT | PROFILE_RIGHT => (position, line),
                _ => (line, position),
            };
            let current = frame.get(x, y);
            if (adding && current != 0)
                || (!adding && current == 0)
                || !graph.editable(x / kk, y / kk)
            {
                valid = false;
                break;
            }
            local.push((x, y, color));
        }
        if valid {
            edits.extend(local);
        }
    }
    let mut changed = 0;
    for (x, y, color) in edits {
        if frame.get(x, y) != color {
            frame.set(x, y, color);
            changed += 1;
        }
    }
    changed
}

fn introduced_blocks(frame: &IndexFrame, native: &IndexFrame, k: u16) -> Vec<bool> {
    let Ok(baseline) = pxl_core::scale_copy(native, k) else {
        return Vec::new();
    };
    if baseline.w != frame.w || baseline.h != frame.h {
        return Vec::new();
    }
    let kk = k.max(1) as i32;
    let mut introduced = vec![false; native.pixels.len()];
    for y in 0..frame.h as i32 {
        for x in 0..frame.w as i32 {
            if frame.get(x, y) != baseline.get(x, y) {
                let i = (y / kk * native.w as i32 + x / kk) as usize;
                introduced[i] = true;
            }
        }
    }
    introduced
}

fn repair_symmetry(
    frame: &mut IndexFrame,
    native: &IndexFrame,
    k: u16,
    graph: &SpriteGraph,
    introduced: &[bool],
) -> u32 {
    let Some(axis) = graph.symmetry else {
        return 0;
    };
    if introduced.len() != native.pixels.len() {
        return 0;
    }
    let Ok(baseline) = pxl_core::scale_copy(native, k) else {
        return 0;
    };
    if baseline.w != frame.w || baseline.h != frame.h {
        return 0;
    }
    let kk = k.max(1) as i32;
    let (w, h) = (frame.w as i32, frame.h as i32);
    let mut costs = [0u32; 2];
    for y in 0..h {
        for x in 0..w {
            let (mx, my) = mirror(x, y, w, h, axis);
            let before = match axis {
                Axis::Vertical => x < mx,
                Axis::Horizontal => y < my,
            };
            if !before {
                continue;
            }
            if frame.get(x, y) != baseline.get(x, y) {
                costs[0] += 1;
            }
            if frame.get(mx, my) != baseline.get(mx, my) {
                costs[1] += 1;
            }
        }
    }
    let source_side = usize::from(costs[1] < costs[0]);
    let snapshot = frame.clone();
    let mut edits = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let (mx, my) = mirror(x, y, w, h, axis);
            let before = match axis {
                Axis::Vertical => x < mx,
                Axis::Horizontal => y < my,
            };
            if !before || snapshot.get(x, y) == snapshot.get(mx, my) {
                continue;
            }
            let a = (y / kk * native.w as i32 + x / kk) as usize;
            let b = (my / kk * native.w as i32 + mx / kk) as usize;
            if !introduced[a] && !introduced[b] {
                continue;
            }
            let ((sx, sy), (tx, ty)) = if source_side == 0 {
                ((x, y), (mx, my))
            } else {
                ((mx, my), (x, y))
            };
            if !graph.editable(tx / kk, ty / kk) {
                continue;
            }
            let color = snapshot.get(sx, sy);
            if snapshot.get(tx, ty) != color {
                edits.push((tx, ty, color));
            }
        }
    }
    let mut changed = 0;
    for (x, y, color) in edits {
        if frame.get(x, y) != color {
            frame.set(x, y, color);
            changed += 1;
        }
    }
    changed
}

fn mirror(x: i32, y: i32, w: i32, h: i32, axis: Axis) -> (i32, i32) {
    match axis {
        Axis::Vertical => (w - 1 - x, y),
        Axis::Horizontal => (x, h - 1 - y),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::{String, ToString};

    fn frame(rows: &[&str]) -> IndexFrame {
        let rows: Vec<String> = rows.iter().map(|r| r.to_string()).collect();
        pxl_core::decode_matrix(&rows).unwrap()
    }

    fn palette() -> pxl_core::Palette {
        let mut p = pxl_core::Palette {
            id: "t".into(),
            colors: alloc::vec![
                pxl_core::Rgba::TRANSPARENT,
                pxl_core::Rgba::rgb(32, 32, 40),
                pxl_core::Rgba::rgb(240, 240, 232),
            ],
            ramps: Default::default(),
            cycles: Default::default(),
            special: pxl_core::Special::default(),
        };
        p.special.outline = 1;
        p
    }

    fn always(_x: i32, _y: i32) -> bool {
        true
    }

    #[test]
    fn a_lone_pixel_goes_away() {
        let mut f = frame(&["000", "020", "000"]);
        assert_eq!(remove_orphans(&mut f, &always), 1);
        assert_eq!(f.count_opaque(), 0);
    }

    #[test]
    fn an_elbow_becomes_a_diagonal() {
        let mut f = frame(&["020", "022", "000"]);
        // (1,1) has neighbours up (1,0) and right (2,1) but no corner at (2,0).
        assert_eq!(remove_elbows(&mut f, &always), 1);
        assert_eq!(f.get(1, 1), 0);
        assert_eq!(f.get(1, 0), 2);
        assert_eq!(f.get(2, 1), 2);
    }

    #[test]
    fn a_three_sided_notch_is_filled() {
        let mut f = frame(&["222", "202", "222"]);
        assert_eq!(fill_notches(&mut f, &always), 1);
        assert_eq!(f.get(1, 1), 2);
    }

    #[test]
    fn kept_features_are_left_alone() {
        let f0 = frame(&["000", "020", "000"]);
        let g = SpriteGraph::build(&f0, &palette(), None);
        let mut f = f0.clone();
        let report = clean(&mut f, &f0, 1, &g, CleanOptions::ALL);
        assert_eq!(report.changed(), 0, "a kept detail is never cleaned away");
        assert_eq!(f, f0);
    }

    fn staircase(lengths: &[i32]) -> IndexFrame {
        let mut rows = Vec::new();
        for (position, length) in lengths.iter().enumerate() {
            for _ in 0..*length {
                rows.push(
                    (0..9)
                        .map(|x| if x < position as i32 { '0' } else { '2' })
                        .collect::<String>(),
                );
            }
        }
        pxl_core::decode_matrix(&rows).unwrap()
    }

    #[test]
    fn rhythm_repairs_irregular_runs_but_preserves_a_curve() {
        let mut f = staircase(&[4, 2, 1, 3, 4]);
        let pal = palette();
        let mut graph = SpriteGraph::build(&f, &pal, None);
        graph.protect.fill(false);
        let before = crate::metrics::jaggy_irregular(&f);
        assert!(before > 0);
        assert!(repair_rhythm(&mut f, 1, &graph) > 0);
        assert!(crate::metrics::jaggy_irregular(&f) < before);

        let curve = staircase(&[5, 3, 2, 1, 1, 2]);
        let mut profile = silhouette_profiles(&curve)[PROFILE_LEFT].clone();
        let original = profile.clone();
        regularize_profile(&mut profile);
        assert_eq!(profile, original);
    }

    #[test]
    fn symmetry_repairs_only_s2_changes_and_respects_protect() {
        let native = frame(&["2222", "2222", "2222", "2222"]);
        let pal = palette();
        let graph = SpriteGraph::build(&native, &pal, None);
        let axis = graph.symmetry.unwrap();
        let mut scaled = pxl_core::scale_copy(&native, 2).unwrap();
        scaled.set(0, 0, 0);
        let introduced = introduced_blocks(&scaled, &native, 2);
        assert_eq!(
            repair_symmetry(&mut scaled, &native, 2, &graph, &introduced),
            1
        );
        let (mx, my) = mirror(0, 0, scaled.w as i32, scaled.h as i32, axis);
        assert_eq!(scaled.get(0, 0), scaled.get(mx, my));

        scaled.set(0, 0, 0);
        let untouched = vec![false; native.pixels.len()];
        assert_eq!(
            repair_symmetry(&mut scaled, &native, 2, &graph, &untouched),
            0
        );

        let mut protected = graph.clone();
        protected.protect[0] = true;
        assert_eq!(
            repair_symmetry(&mut scaled, &native, 2, &protected, &introduced),
            0
        );
        assert_eq!(scaled.get(0, 0), 0);
    }
}
