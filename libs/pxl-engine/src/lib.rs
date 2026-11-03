//! Assembly and rendering that never touches the filesystem, stdin, or the
//! clock. `pxl-cli` and `pxl-wasm` both drive it; files a spec refers to come
//! through a [`Resolver`], so the same code runs against a directory or an
//! in-memory table.

mod ascii_compile;
mod ascii_import;
mod document;
mod render;
mod resolver;
mod session;
mod sprite_build;
mod timing;

pub use ascii_compile::{build_parts, file_name, frames_for_recipe, RecipeAscii};
pub use ascii_import::{build_spec, check_canvas_size, AsciiImportOptions};
pub use document::doc_from_frames;
pub use render::{
    check_scale, render_doc_bytes, render_doc_bytes_as, FrameSelection, OutputFormat,
    OutputPurpose, Rendered,
};
pub use resolver::{load_palette, MemoryResolver, Resolver, ResourceKind};
pub use session::{
    assemble, describe, detect_kind, inspect, render, AnimInfo, DocInfo, FormatChoice, InputKind,
    RenderOutput, RenderRequest, DEFAULT_PALETTE, MAX_INPUT_BYTES,
};
pub use sprite_build::{build_one, build_sprite, finish_options, BuiltSprite};
pub use timing::{check_anim_timing, MAX_ANIM_FPS, MAX_ANIM_FRAMES};
