use clap::{Args, Subcommand};

#[derive(Subcommand, Debug)]
pub enum PaletteCmd {
    List,
    Show {
        palette: String,
    },
    Apply {
        input: String,
        palette: String,
        #[arg(short, long)]
        out: String,
    },
    Swap {
        input: String,
        #[arg(long)]
        map: String,
        #[arg(long)]
        frame: Option<String>,
        #[arg(short, long)]
        out: String,
    },
    /// Generate shadow and light steps for every colour and write the extended palette.
    Ramp {
        /// Palette file or built-in id.
        palette: String,
        /// Steps per colour: 2 (shadow), 3 (+ light) or 4 (+ deep shadow).
        #[arg(long, default_value_t = 4)]
        levels: u8,
        /// 0 to 100: how far shadows lean cool and lights lean warm.
        #[arg(long, default_value_t = 40)]
        hue_shift: u8,
        /// Palette size cap. Documents hold at most 36 colours as text.
        #[arg(long = "max", default_value_t = 36)]
        max: usize,
        /// Output *.palette.json.
        #[arg(short, long)]
        out: String,
    },
    Cycle {
        input: String,
        #[arg(long)]
        cycle: String,
        #[arg(long, default_value_t = 1)]
        step: i32,
        #[arg(long)]
        frame: Option<String>,
        #[arg(short, long)]
        out: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum TileCmd {
    Gen {
        #[arg(long)]
        kind: String,
        #[arg(long, default_value_t = 16)]
        size: u16,
        #[arg(long)]
        palette: String,
        #[arg(long)]
        ramp: Option<String>,
        #[arg(long, default_value_t = 4)]
        variants: u32,
        #[arg(short, long)]
        out: String,
    },
    Autotile,
}

#[derive(Args, Debug)]
pub struct SheetArgs {
    pub inputs: Vec<String>,
    #[arg(long, default_value = "rows-by-anim")]
    pub layout: String,
    #[arg(long, default_value_t = 1)]
    pub pad: u32,
    #[arg(short, long)]
    pub out: String,
}

#[derive(Args, Debug)]
pub struct RenderArgs {
    pub input: String,
    #[arg(long)]
    pub frame: Option<String>,
    #[arg(long)]
    pub anim: Option<String>,
    #[arg(long)]
    pub dir: Option<String>,
    #[arg(long, default_value_t = 1)]
    pub scale: u32,
    #[arg(long)]
    pub grid: bool,
    /// Write a transparent asset PNG: no backdrop, no grid. Default is a preview.
    #[arg(long, conflicts_with = "grid")]
    pub asset: bool,
    #[arg(short, long)]
    pub out: String,
}

#[derive(Args, Debug)]
pub struct InspectArgs {
    pub input: String,
    #[arg(long)]
    pub frame: Option<String>,
}

#[derive(Args, Debug)]
pub struct ValidateArgs {
    pub input: String,
    #[arg(long, default_value = "default", conflicts_with = "qa_config")]
    pub rules: String,
    /// Read QA thresholds and policies from a JSON configuration file.
    #[arg(long)]
    pub qa_config: Option<String>,
}

#[derive(Args, Debug)]
pub struct BuildArgs {
    pub recipe: String,
    #[arg(short, long)]
    pub out: String,
    /// Override recipe QA rules with a JSON configuration file.
    #[arg(long)]
    pub qa_config: Option<String>,
}

/// `plan` is the batch entry between steps: it reads the step-200 output
/// (spec-manifest.json when present, otherwise a scan for recipe files),
/// builds every recipe with the same pipeline as `build`, and writes one
/// index.json that the package step consumes. A failed recipe never aborts
/// the batch; the worst status wins the exit code.
#[derive(Args, Debug)]
pub struct PlanArgs {
    /// Directory holding spec-manifest.json or <id>/<id>.recipe.json files.
    #[arg(long = "in", value_name = "DIR")]
    pub input: String,
    /// Root output directory; each recipe builds into <out>/<id>/.
    #[arg(short, long)]
    pub out: String,
    /// Build only the recipe whose id (stem or manifest id) matches.
    #[arg(long)]
    pub asset: Option<String>,
    /// Parse and validate every recipe without writing anything.
    #[arg(long)]
    pub dry_run: bool,
    /// Build every recipe twice in isolation and write a deterministic baseline.
    #[arg(long)]
    pub baseline: bool,
    /// Scope label recorded in index.json (all or slice).
    #[arg(long, default_value = "all")]
    pub scope: String,
    /// QA configuration forwarded to every recipe build.
    #[arg(long, value_name = "FILE")]
    pub qa_config: Option<String>,
}

/// Flags `pxl enhance` and `pxl enhance compare` share.
#[derive(Args, Debug)]
pub struct EnhanceCommon {
    /// Integer scale applied to the native frame: 2, 3 or 4.
    #[arg(long, default_value_t = 3)]
    pub scale: u16,
    /// none, flat, selout, selout_lit, or keep (scale the drawn outline as is).
    #[arg(long, default_value = "selout")]
    pub outline: String,
    /// top_left, top_right, or top. Which side selout_lit lifts.
    #[arg(long, default_value = "top_left")]
    pub light: String,
    /// none or volume. volume lights and shades each region from its distance field.
    #[arg(long, default_value = "none")]
    pub shade: String,
    /// auto keeps small and thin features axis-aligned; none lets every stage edit them.
    #[arg(long, default_value = "auto")]
    pub protect: String,
    /// all, none, or a comma list of orphan, elbow, notch, rhythm, symmetry.
    #[arg(long, default_value = "all")]
    pub cleanup: String,
    /// Ramp steps generated per colour: 2, 3 or 4.
    #[arg(long, default_value_t = 4)]
    pub levels: u8,
    /// 0 to 100: how far shadows lean cool and lights lean warm.
    #[arg(long, default_value_t = 40)]
    pub hue_shift: u8,
    /// Palette size cap for generated ramp colours.
    #[arg(long, default_value_t = 36)]
    pub max_colors: usize,
    /// Palette file or built-in id, for inputs that name none.
    #[arg(long)]
    pub palette: Option<String>,
    /// Frame to enhance: `base` when the document has one, else the first.
    #[arg(long)]
    pub frame: Option<String>,
    /// classic, auto, checker, or #rrggbb. Default: auto (compare: checker).
    #[arg(long)]
    pub background: Option<String>,
    /// Checker square, in native pixels.
    #[arg(long, default_value_t = 8)]
    pub checker_cell: u32,
    /// Nearest-neighbour zoom of the written image. Default: 1 (compare: 4).
    #[arg(long)]
    pub zoom: Option<u32>,
}

#[derive(Args, Debug)]
pub struct EnhanceRunArgs {
    /// *.ascii.txt, *.ascii.json, *.shapes.json, a *.pxl.json document, or a png.
    /// Optional for clap only, so `compare` can take over; `enhance` checks it.
    pub input: Option<String>,
    #[command(flatten)]
    pub common: EnhanceCommon,
    /// copy, epx, xbr, scale3x, contour, or hybrid.
    #[arg(long, default_value = "hybrid")]
    pub upscale: String,
    /// Write a transparent PNG at the enhanced size instead of a preview.
    #[arg(long)]
    pub asset: bool,
    /// Also write the stage report and metrics as JSON.
    #[arg(long)]
    pub report: Option<String>,
    /// Print the metric table.
    #[arg(long)]
    pub metrics: bool,
    /// Output png. Required, checked by `enhance` itself.
    #[arg(short, long)]
    pub out: Option<String>,
}

#[derive(Args, Debug)]
pub struct EnhanceCompareArgs {
    /// Same inputs as `pxl enhance`.
    pub input: String,
    #[command(flatten)]
    pub common: EnhanceCommon,
    /// Comma list of copy, epx, xbr, scale3x, contour, hybrid. Default: every
    /// upscaler that supports --scale.
    #[arg(long)]
    pub variants: Option<String>,
    #[arg(short, long)]
    pub out: String,
}

#[derive(Subcommand, Debug)]
pub enum EnhanceSub {
    /// Lay the nearest-neighbour baseline and each upscaler side by side on one sheet.
    Compare(EnhanceCompareArgs),
}

#[derive(Args, Debug)]
#[command(args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
pub struct EnhanceArgs {
    #[command(flatten)]
    pub run: EnhanceRunArgs,
    #[command(subcommand)]
    pub action: Option<EnhanceSub>,
}
