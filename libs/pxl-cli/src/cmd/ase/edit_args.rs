//! Clap args for GD edit commands（非官方）.
use clap::{Args, Subcommand};

#[derive(Subcommand, Debug)]
pub enum FrameCmd {
    Add(FrameAddArgs),
    Remove(FrameIndexArgs),
    Copy(FrameCopyArgs),
    SetDuration(FrameDurationArgs),
}

#[derive(Args, Debug)]
pub struct FrameAddArgs {
    pub file: String,
    #[arg(long, default_value_t = 0)]
    pub at: u16,
    #[arg(long)]
    pub duration: Option<u16>,
    #[arg(long, short = 'o')]
    pub out: Option<String>,
}

#[derive(Args, Debug)]
pub struct FrameIndexArgs {
    pub file: String,
    pub index: u16,
    #[arg(long, short = 'o')]
    pub out: Option<String>,
}

#[derive(Args, Debug)]
pub struct FrameCopyArgs {
    pub file: String,
    pub from: u16,
    pub to: u16,
    #[arg(long, short = 'o')]
    pub out: Option<String>,
}

#[derive(Args, Debug)]
pub struct FrameDurationArgs {
    pub file: String,
    pub index: u16,
    pub ms: u16,
    #[arg(long, short = 'o')]
    pub out: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum TagCmd {
    Add(TagAddArgs),
    Remove(TagNameArgs),
    Rename(TagRenameArgs),
    Set(TagSetArgs),
}

#[derive(Args, Debug)]
pub struct TagAddArgs {
    pub file: String,
    pub name: String,
    #[arg(long)]
    pub from: u16,
    #[arg(long)]
    pub to: u16,
    #[arg(long, default_value = "forward")]
    pub direction: String,
    #[arg(long, default_value_t = 0)]
    pub repeat: u16,
    #[arg(long, short = 'o')]
    pub out: Option<String>,
}

#[derive(Args, Debug)]
pub struct TagNameArgs {
    pub file: String,
    pub name: String,
    #[arg(long, short = 'o')]
    pub out: Option<String>,
}

#[derive(Args, Debug)]
pub struct TagRenameArgs {
    pub file: String,
    pub old: String,
    pub new: String,
    #[arg(long, short = 'o')]
    pub out: Option<String>,
}

#[derive(Args, Debug)]
pub struct TagSetArgs {
    pub file: String,
    pub name: String,
    #[arg(long)]
    pub from: Option<u16>,
    #[arg(long)]
    pub to: Option<u16>,
    #[arg(long)]
    pub direction: Option<String>,
    #[arg(long)]
    pub repeat: Option<u16>,
    #[arg(long, short = 'o')]
    pub out: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum LayerCmd {
    Add(LayerAddArgs),
    Remove(LayerNameArgs),
    Rename(LayerRenameArgs),
    Set(LayerSetArgs),
}

#[derive(Args, Debug)]
pub struct LayerAddArgs {
    pub file: String,
    pub name: String,
    #[arg(long, short = 'o')]
    pub out: Option<String>,
}

#[derive(Args, Debug)]
pub struct LayerNameArgs {
    pub file: String,
    pub name: String,
    #[arg(long, short = 'o')]
    pub out: Option<String>,
}

#[derive(Args, Debug)]
pub struct LayerRenameArgs {
    pub file: String,
    pub old: String,
    pub new: String,
    #[arg(long, short = 'o')]
    pub out: Option<String>,
}

#[derive(Args, Debug)]
pub struct LayerSetArgs {
    pub file: String,
    pub name: String,
    #[arg(long)]
    pub opacity: Option<u8>,
    #[arg(long)]
    pub blend: Option<String>,
    #[arg(long)]
    pub visible: Option<bool>,
    #[arg(long, short = 'o')]
    pub out: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum CelCmd {
    Move(CelMoveArgs),
    Unlink(CelRefArgs),
    Clear(CelRefArgs),
    Set(CelSetArgs),
}

#[derive(Args, Debug)]
pub struct CelMoveArgs {
    pub file: String,
    pub layer: String,
    pub from: u16,
    pub to: u16,
    #[arg(long, short = 'o')]
    pub out: Option<String>,
}

#[derive(Args, Debug)]
pub struct CelRefArgs {
    pub file: String,
    pub layer: String,
    pub frame: u16,
    #[arg(long, short = 'o')]
    pub out: Option<String>,
}

#[derive(Args, Debug)]
pub struct CelSetArgs {
    pub file: String,
    pub layer: String,
    pub frame: u16,
    #[arg(long)]
    pub x: Option<i16>,
    #[arg(long)]
    pub y: Option<i16>,
    #[arg(long)]
    pub opacity: Option<u8>,
    #[arg(long, short = 'o')]
    pub out: Option<String>,
}

#[derive(Args, Debug)]
pub struct FlattenArgs {
    pub file: String,
    #[arg(long, short = 'o')]
    pub out: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum SpriteCmd {
    Set(SpriteSetArgs),
}

#[derive(Args, Debug)]
pub struct SpriteSetArgs {
    pub file: String,
    #[arg(long = "transparent-index")]
    pub transparent_index: Option<u8>,
    #[arg(long = "pixel-ratio")]
    pub pixel_ratio: Option<String>,
    #[arg(long)]
    pub grid: Option<String>,
    #[arg(long, short = 'o')]
    pub out: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum PaletteCmd {
    Sort(PaletteSortArgs),
    /// Set palette entry color (`#RRGGBB` / `#RRGGBBAA`). Optional `--frame` for frame palette.
    Set(PaletteSetArgs),
    /// Append a color to the (frame) palette.
    Add(PaletteAddArgs),
    /// Remove palette entry by index.
    Remove(PaletteRemoveArgs),
    /// Drop a frame-level palette override.
    ClearFrame(PaletteClearFrameArgs),
}

#[derive(Args, Debug)]
pub struct PaletteSortArgs {
    pub file: String,
    #[arg(long = "by", default_value = "hue")]
    pub by: String,
    #[arg(long = "keep-zero", default_value_t = true)]
    pub keep_zero: bool,
    #[arg(long, short = 'o')]
    pub out: Option<String>,
}

#[derive(Args, Debug)]
pub struct ExportTilesetArgs {
    pub file: String,
    #[arg(long, short = 'o')]
    pub out: String,
    #[arg(long, default_value_t = 0)]
    pub index: usize,
}


#[derive(Args, Debug)]
pub struct PaletteSetArgs {
    pub file: String,
    pub index: usize,
    /// `#RRGGBB` or `#RRGGBBAA`
    pub color: String,
    #[arg(long)]
    pub frame: Option<u16>,
    #[arg(long, short = 'o')]
    pub out: Option<String>,
}

#[derive(Args, Debug)]
pub struct PaletteAddArgs {
    pub file: String,
    pub color: String,
    #[arg(long)]
    pub frame: Option<u16>,
    #[arg(long, short = 'o')]
    pub out: Option<String>,
}

#[derive(Args, Debug)]
pub struct PaletteRemoveArgs {
    pub file: String,
    pub index: usize,
    #[arg(long)]
    pub frame: Option<u16>,
    #[arg(long, short = 'o')]
    pub out: Option<String>,
}

#[derive(Args, Debug)]
pub struct PaletteClearFrameArgs {
    pub file: String,
    pub frame: u16,
    #[arg(long, short = 'o')]
    pub out: Option<String>,
}
