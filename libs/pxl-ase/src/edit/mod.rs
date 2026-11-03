//! Document edit ops (v8.1 GD) — non-official CLI surface over the aseprite model.
mod cel;
mod flatten;
mod frame;
mod layer;
mod palette;
mod sprite_attrs;
mod tag;

pub use cel::{cel_clear, cel_move, cel_set_opacity, cel_set_position, cel_unlink};
pub use flatten::flatten_layers;
pub use frame::{frame_add, frame_copy, frame_remove, frame_set_duration};
pub use layer::{layer_add, layer_remove, layer_rename, layer_set};
pub use palette::palette_sort;
pub use sprite_attrs::{sprite_set_attrs, SpriteAttrPatch};
pub use tag::{tag_add, tag_remove, tag_rename, tag_set};
