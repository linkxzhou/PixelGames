pub const CAPABILITIES_JSON: &str = include_str!("../../../assets/capabilities.json");

pub fn capability_digest() -> String {
    format!(
        "sha256:{}",
        pxl_core::sha256_hex(CAPABILITIES_JSON.as_bytes())
    )
}
