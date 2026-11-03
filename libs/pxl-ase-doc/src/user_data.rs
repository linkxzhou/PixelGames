//! Per-object user data (ase-file-specs User Data chunk 0x2020).

use alloc::string::String;
use alloc::vec::Vec;

use pxl_core::Rgba;

/// Text / color / opaque properties map bytes from chunk 0x2020.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UserData {
    pub text: Option<String>,
    pub color: Option<Rgba>,
    /// Raw bytes of the properties-maps section (size DWORD through end), if bit 4 set.
    /// Passthrough only — not interpreted.
    pub properties_raw: Option<Vec<u8>>,
}

impl UserData {
    pub fn is_empty(&self) -> bool {
        self.text.is_none() && self.color.is_none() && self.properties_raw.is_none()
    }

    pub fn from_rgb_compat(r: u8, g: u8, b: u8) -> Self {
        Self {
            text: None,
            color: Some(Rgba { r, g, b, a: 255 }),
            properties_raw: None,
        }
    }
}
