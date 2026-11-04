use miniquad::{FilterMode, MipmapFilterMode, RenderingBackend, TextureId};

pub struct PendingTexture {
    pub id: u32,
    pub width: u16,
    pub height: u16,
    pub rgba: Vec<u8>,
}

pub fn upload(ctx: &mut dyn RenderingBackend, pending: &PendingTexture) -> TextureId {
    let texture = ctx.new_texture_from_rgba8(pending.width, pending.height, &pending.rgba);
    ctx.texture_set_filter(texture, FilterMode::Nearest, MipmapFilterMode::None);
    texture
}
