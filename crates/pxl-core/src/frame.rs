use crate::{PxlError, PxlResult, MAX_DIM};
use alloc::vec;
use alloc::vec::Vec;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    pub fn contains(self, px: i32, py: i32) -> bool {
        px >= self.x && py >= self.y && px < self.x + self.w && py < self.y + self.h
    }

    pub fn area(self) -> i64 {
        (self.w as i64) * (self.h as i64)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameStats {
    pub colors_used: usize,
    pub bbox: Option<Rect>,
    pub opaque: usize,
    pub components: usize,
    pub isolated: usize,
}

/// Row-major palette-index matrix. Index 0 is transparent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexFrame {
    pub w: u16,
    pub h: u16,
    pub pixels: Vec<u8>,
}

impl IndexFrame {
    pub fn new(w: u16, h: u16) -> PxlResult<Self> {
        if w == 0 || h == 0 || w > MAX_DIM || h > MAX_DIM {
            return Err(
                PxlError::schema(alloc::format!("frame size {w}x{h} is outside 1..=64"))
                    .with_hint("M1 canvases are at most 64×64; the tank demo uses 16×16"),
            );
        }
        Self::canvas(w, h)
    }

    /// Sheet-sized buffer. Sprite generators stay on [`Self::new`].
    pub fn canvas(w: u16, h: u16) -> PxlResult<Self> {
        if w == 0 || h == 0 || w > 2048 || h > 2048 {
            return Err(PxlError::schema(alloc::format!(
                "canvas {w}x{h} is outside 1..=2048"
            )));
        }
        let n = (w as usize) * (h as usize);
        Ok(Self {
            w,
            h,
            pixels: vec![0; n],
        })
    }

    pub fn filled(w: u16, h: u16, color: u8) -> PxlResult<Self> {
        let mut f = Self::new(w, h)?;
        f.pixels.fill(color);
        Ok(f)
    }

    pub fn idx(&self, x: i32, y: i32) -> Option<usize> {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            None
        } else {
            Some((y as usize) * (self.w as usize) + (x as usize))
        }
    }

    pub fn get(&self, x: i32, y: i32) -> u8 {
        self.idx(x, y).map(|i| self.pixels[i]).unwrap_or(0)
    }

    pub fn set(&mut self, x: i32, y: i32, c: u8) {
        if let Some(i) = self.idx(x, y) {
            self.pixels[i] = c;
        }
    }

    pub fn in_bounds(&self, x: i32, y: i32) -> bool {
        self.idx(x, y).is_some()
    }

    pub fn blank_like(&self) -> Self {
        Self {
            w: self.w,
            h: self.h,
            pixels: vec![0; self.pixels.len()],
        }
    }

    pub fn opaque_at(&self, x: i32, y: i32) -> bool {
        self.get(x, y) != 0
    }

    pub fn colors_used(&self) -> usize {
        let mut seen = [false; 256];
        let mut n = 0;
        for &p in &self.pixels {
            if !seen[p as usize] {
                seen[p as usize] = true;
                n += 1;
            }
        }
        n
    }

    pub fn bbox(&self) -> Option<Rect> {
        let mut min_x = self.w as i32;
        let mut min_y = self.h as i32;
        let mut max_x = -1;
        let mut max_y = -1;
        for y in 0..self.h as i32 {
            for x in 0..self.w as i32 {
                if self.opaque_at(x, y) {
                    if x < min_x {
                        min_x = x;
                    }
                    if y < min_y {
                        min_y = y;
                    }
                    if x > max_x {
                        max_x = x;
                    }
                    if y > max_y {
                        max_y = y;
                    }
                }
            }
        }
        if max_x < 0 {
            None
        } else {
            Some(Rect::new(
                min_x,
                min_y,
                max_x - min_x + 1,
                max_y - min_y + 1,
            ))
        }
    }

    pub fn count_opaque(&self) -> usize {
        self.pixels.iter().filter(|p| **p != 0).count()
    }

    /// 4-connected components of opaque pixels, plus how many of those are single pixels.
    pub fn components(&self) -> (usize, usize) {
        let w = self.w as usize;
        let h = self.h as usize;
        let mut seen = vec![false; w * h];
        let mut comps = 0;
        let mut isolated = 0;
        let mut stack = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let start = y * w + x;
                if seen[start] || self.pixels[start] == 0 {
                    continue;
                }
                comps += 1;
                stack.clear();
                stack.push((x as i32, y as i32));
                seen[start] = true;
                let mut size = 0;
                while let Some((cx, cy)) = stack.pop() {
                    size += 1;
                    for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                        let nx = cx + dx;
                        let ny = cy + dy;
                        if let Some(i) = self.idx(nx, ny) {
                            if !seen[i] && self.pixels[i] != 0 {
                                seen[i] = true;
                                stack.push((nx, ny));
                            }
                        }
                    }
                }
                if size == 1 {
                    isolated += 1;
                }
            }
        }
        (comps, isolated)
    }

    pub fn stats(&self) -> FrameStats {
        let (components, isolated) = self.components();
        FrameStats {
            colors_used: self.colors_used(),
            bbox: self.bbox(),
            opaque: self.count_opaque(),
            components,
            isolated,
        }
    }

    /// Changed pixels over `w*h`. Both frames must match in size.
    pub fn diff_count(&self, other: &IndexFrame) -> usize {
        self.pixels
            .iter()
            .zip(other.pixels.iter())
            .filter(|(a, b)| a != b)
            .count()
    }

    pub fn hash_bytes(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(4 + self.pixels.len());
        v.extend_from_slice(&self.w.to_le_bytes());
        v.extend_from_slice(&self.h.to_le_bytes());
        v.extend_from_slice(&self.pixels);
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn components_and_isolated() {
        let mut f = IndexFrame::new(4, 4).unwrap();
        f.set(0, 0, 1);
        f.set(2, 2, 1);
        f.set(3, 2, 1);
        let (c, iso) = f.components();
        assert_eq!(c, 2);
        assert_eq!(iso, 1);
        let b = f.bbox().unwrap();
        assert_eq!(b, Rect::new(0, 0, 4, 3));
    }
}
