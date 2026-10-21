//! Generators. Algorithms are reimplemented from published descriptions.
//! Bollinger masks follow the MIT zfedoran/pixel-sprite-generator pipeline
//! (body → mirror → edges → indexed color) and do not copy the unlicensed
//! MaartenGr Python port.

mod bollinger;
mod builtin;
mod noise;
mod shapes;

pub use bollinger::{generate_bollinger, MaskTemplate, RuntimeMask};
pub use builtin::{
    builtin_palette, builtin_template, load_builtin_mask, palette_ids, template_ids, Builtin,
};
pub use noise::{generate_noise_tile, variant_seed};
pub use shapes::generate_shapes;
