//! PNG and GIF encoders with fixed settings, plus Aseprite hash-format sheets.
//! No timestamps. Indexed color uses PLTE + tRNS. Filters and compression are pinned.

mod background;
mod gif;
mod label;
mod png;
mod preview;
mod raster;
mod review;
mod sheet;

pub use background::{composite_on, Background, MIN_OUTLINE_DL};
pub use gif::{encode_gif, encode_gif_timed, gif_delays_cs};
pub use label::{contact_sheet, text_height, text_width, Panel};
pub use png::{decode_indexed, decode_png_rgba, encode_indexed_png, encode_rgba_png};
pub use preview::{render_asset_png, render_preview_png};
pub use raster::{
    composite, composite_checked, scale_nearest, scale_rgba, scale_rgba_checked, scaled_dims,
};
pub use review::{review_image, ReviewMode};
pub use sheet::{pack_sheet, pack_sheet_padded, sheet_budget_json, sheet_json, sheet_json_format, PackedSheet, Placement, SheetBudget, SheetCell, SheetPadding};

#[cfg(test)]
mod tests;
