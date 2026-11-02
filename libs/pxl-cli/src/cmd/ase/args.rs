//! Clap args for ase.

use clap::Args;

#[derive(Args, Debug)]
pub struct AseInspectArgs {
    pub file: String,
    #[arg(long)]
    pub oneframe: bool,
}

#[derive(Args, Debug)]
pub struct AseFileArgs {
    pub file: String,
    #[arg(long)]
    pub oneframe: bool,
}

#[derive(Args, Debug)]
pub struct AseConvertArgs {
    /// One or more `.aseprite` inputs (batch when multiple).
    pub files: Vec<String>,
    /// Output path (`.png`, `.gif`, or `.aseprite`). With multiple inputs, treat as directory or format pattern.
    #[arg(long = "save-as", short = 'o')]
    pub save_as: String,
    #[arg(long, short = 'b')]
    pub batch: bool,
    #[arg(long, short = 'p')]
    pub preview: bool,
    #[arg(long)]
    pub oneframe: bool,
    #[arg(long)]
    pub tag: Option<String>,
    #[arg(long = "frame-range")]
    pub frame_range: Option<String>,
    #[arg(long, default_value_t = 1)]
    pub scale: u32,
    /// rgba | indexed | grayscale
    #[arg(long = "color-mode")]
    pub color_mode: Option<String>,
    /// External `.gpl` / `.act` palette applied before color-mode convert.
    #[arg(long = "palette")]
    pub palette: Option<String>,
    /// none | ordered | old
    #[arg(long = "dithering-algorithm", default_value = "none")]
    pub dithering_algorithm: String,
    /// bayer2x2 | bayer4x4 | bayer8x8
    #[arg(long = "dithering-matrix", default_value = "bayer8x8")]
    pub dithering_matrix: String,
    /// Frame index when not using --tag/--frame-range (default 0 for single PNG).
    #[arg(long, default_value_t = 0)]
    pub frame: u16,
    /// Filename format for sequences: `{frame}` `{tag}` `{layer}` `{title}` …
    #[arg(long = "filename-format")]
    pub filename_format: Option<String>,
    /// Skip fully transparent frames when writing sequences/GIF/sheet.
    #[arg(long = "ignore-empty")]
    pub ignore_empty: bool,
    /// Include hidden layers in flatten.
    #[arg(long = "all-layers")]
    pub all_layers: bool,
    /// Only include this layer name (hide others).
    #[arg(long = "layer")]
    pub layer: Option<String>,
    /// Split slices to cropped PNGs under this directory (also usable via `ase split-slices`).
    #[arg(long = "split-slices")]
    pub split_slices: Option<String>,
    /// Crop document to `x,y,w,h` before export.
    #[arg(long = "crop")]
    pub crop: Option<String>,
    /// Skip consecutive duplicate frames.
    #[arg(long = "merge-duplicates")]
    pub merge_duplicates: bool,
    /// Trim transparent borders on the document before export.
    #[arg(long = "trim")]
    pub trim: bool,
    /// Trim then snap crop edges to sprite Grid (from .aseprite header).
    #[arg(long = "trim-by-grid")]
    pub trim_by_grid: bool,
    /// Expand nested/sub tags by range containment when using --tag.
    #[arg(long = "play-subtags")]
    pub play_subtags: bool,
}

#[derive(Args, Debug)]
pub struct AseSheetArgs {
    pub file: String,
    #[arg(long, short = 'o')]
    pub out: String,
    #[arg(long)]
    pub data: Option<String>,
    #[arg(long)]
    pub tag: Option<String>,
    #[arg(long = "frame-range")]
    pub frame_range: Option<String>,
    #[arg(long)]
    pub oneframe: bool,
    /// horizontal|vertical|packed|rows|grid (aliases map to pxl-export layouts).
    #[arg(long, default_value = "horizontal")]
    pub layout: String,
    #[arg(long = "ignore-empty")]
    pub ignore_empty: bool,
    #[arg(long = "all-layers")]
    pub all_layers: bool,
    /// Padding between packed cells (px).
    #[arg(long = "padding", default_value_t = 0)]
    pub padding: u32,
    /// Extrude each cell edge by N px (edge duplicate).
    #[arg(long = "extrude", default_value_t = 0)]
    pub extrude: u32,
    /// Trim transparent borders per cell before pack.
    #[arg(long = "trim")]
    pub trim: bool,
    /// Trim document by content bbox snapped to sprite Grid, then pack.
    #[arg(long = "trim-by-grid")]
    pub trim_by_grid: bool,
    /// Skip consecutive duplicate frames.
    #[arg(long = "merge-duplicates")]
    pub merge_duplicates: bool,
    /// Expand nested/sub tags by range containment when using --tag.
    #[arg(long = "play-subtags")]
    pub play_subtags: bool,
}

#[derive(Args, Debug)]
pub struct AseFilterArgs {
    pub file: String,
    /// outline | replace-index | replace-color | invert | brightness-contrast | hue-saturation
    pub name: String,
    #[arg(long, short = 'o')]
    pub out: String,
    #[arg(long, default_value_t = 1)]
    pub color: u8,
    #[arg(long, default_value = "outer")]
    pub mode: String,
    #[arg(long, default_value = "square")]
    pub brush: String,
    #[arg(long, default_value_t = 1)]
    pub width: i32,
    #[arg(long)]
    pub from: Option<u8>,
    #[arg(long)]
    pub to: Option<u8>,
    /// Source color `#RRGGBB` / `#RRGGBBAA` for replace-color.
    #[arg(long = "from-color")]
    pub from_color: Option<String>,
    /// Target color for replace-color.
    #[arg(long = "to-color")]
    pub to_color: Option<String>,
    #[arg(long, default_value_t = 0)]
    pub tolerance: u8,
    #[arg(long, default_value_t = 0)]
    pub brightness: i32,
    #[arg(long, default_value_t = 0)]
    pub contrast: i32,
    /// Outline every frame (default: frame 0 only for outline).
    #[arg(long = "all-frames")]
    pub all_frames: bool,
    #[arg(long = "hue", default_value_t = 0)]
    pub hue: i32,
    #[arg(long = "sat", default_value_t = 0)]
    pub sat: i32,
    #[arg(long = "value", default_value_t = 0)]
    pub value: i32,
}

#[derive(Args, Debug)]
pub struct AseOpArgs {
    pub file: String,
    /// flip | rotate90 | flood-fill | crop | canvas-size | trim
    pub name: String,
    #[arg(long, short = 'o')]
    pub out: String,
    /// flip: h|v|horizontal|vertical ; rotate90: cw|ccw
    #[arg(long, default_value = "h")]
    pub axis: String,
    #[arg(long, default_value = "cw")]
    pub dir: String,
    #[arg(long, default_value_t = 0)]
    pub x: i32,
    #[arg(long, default_value_t = 0)]
    pub y: i32,
    #[arg(long)]
    pub w: Option<u16>,
    #[arg(long)]
    pub h: Option<u16>,
    #[arg(long, default_value_t = 0)]
    pub frame: u16,
    /// Fill / target color `#RRGGBB` / `#RRGGBBAA` for flood-fill.
    #[arg(long = "to-color")]
    pub to_color: Option<String>,
    #[arg(long, default_value_t = 0)]
    pub tolerance: u8,
    /// canvas-size: place old content at this origin.
    #[arg(long = "ox", default_value_t = 0)]
    pub ox: i32,
    #[arg(long = "oy", default_value_t = 0)]
    pub oy: i32,
}

#[derive(Args, Debug)]
pub struct AseImportArgs {
    pub file: String,
    #[arg(long, short = 'o')]
    pub out: String,
    /// Asset id for recipe/palette (default: file stem).
    #[arg(long)]
    pub asset: Option<String>,
    #[arg(long)]
    pub oneframe: bool,
}

#[derive(Args, Debug)]
pub struct AseSplitSlicesArgs {
    pub file: String,
    #[arg(long, short = 'o')]
    pub out: String,
    #[arg(long, default_value_t = 0)]
    pub frame: u16,
    #[arg(long, default_value_t = 1)]
    pub scale: u32,
    #[arg(long)]
    pub oneframe: bool,
}


#[derive(Args, Debug)]
pub struct AseRunArgs {
    /// Sequential argv (`--` then flags/files left-to-right). See cli-flag-matrix.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub args: Vec<String>,
}

#[derive(Args, Debug)]
pub struct AseScriptArgs {
    /// Path to a `.rhai` script.
    pub path: String,
    /// Repeatable `key=value` injected as `params.key` in the script.
    #[arg(long = "script-param", value_name = "KEY=VALUE")]
    pub script_param: Vec<String>,
    /// Working directory for path sandbox (default: cwd).
    #[arg(long = "workdir")]
    pub workdir: Option<String>,
    /// Allow script paths outside workdir (dangerous; off by default).
    #[arg(long = "allow-fs")]
    pub allow_fs: bool,
}
