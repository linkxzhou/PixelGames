//! PNG and GIF encoders with fixed settings, plus Aseprite hash-format sheets.
//! No timestamps. Indexed color uses PLTE + tRNS. Filters and compression are pinned.

mod gif;
mod png;
mod preview;
mod raster;
mod sheet;

pub use gif::{encode_gif, encode_gif_timed, gif_delays_cs};
pub use png::{decode_indexed, decode_png_rgba, encode_indexed_png, encode_rgba_png};
pub use preview::render_preview_png;
pub use raster::{composite, scale_nearest, scale_rgba, scale_rgba_checked};
pub use sheet::{pack_sheet, sheet_json, PackedSheet, Placement, SheetCell};

#[cfg(test)]
mod tests;
