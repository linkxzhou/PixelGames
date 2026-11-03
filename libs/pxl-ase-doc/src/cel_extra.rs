//! Cel Extra chunk (0x2006) precise bounds — stored for write-back; render may ignore.

/// FIXED = 16.16 signed; we keep the raw i32 words from the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct CelExtra {
    pub flags: u32,
    pub precise_x: i32,
    pub precise_y: i32,
    pub width: i32,
    pub height: i32,
}

impl CelExtra {
    pub fn has_precise_bounds(self) -> bool {
        self.flags & 1 != 0
    }
}
