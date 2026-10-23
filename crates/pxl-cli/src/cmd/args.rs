use clap::{Args, Subcommand};

#[derive(Args, Debug)]
pub struct GenArgs {
    /// bollinger, shapes, noise-tile, or parts.
    pub generator: String,
    #[arg(long)]
    pub template: Option<String>,
    #[arg(long)]
    pub spec: Option<String>,
    #[arg(long)]
    pub palette: Option<String>,
    #[arg(long)]
    pub ramp: Option<String>,
    #[arg(long)]
    pub mirror: Option<String>,
    /// Facing to draw when `generator` is `parts`. Omit for a blank canvas.
    #[arg(long)]
    pub dir: Option<String>,
    #[arg(long)]
    pub kind: Option<String>,
    #[arg(long)]
    pub size: Option<u16>,
    #[arg(long, default_value_t = 1)]
    pub variants: u32,
    #[arg(short, long)]
    pub out: String,
}

#[derive(Args, Debug)]
pub struct OpArgs {
    pub input: String,
    #[arg(long)]
    pub ops: Option<String>,
    #[arg(long = "op")]
    pub op: Vec<String>,
    #[arg(long)]
    pub frame: Option<String>,
    /// Store the result under this frame name instead of replacing `--frame`.
    #[arg(long = "as")]
    pub as_frame: Option<String>,
    #[arg(short, long)]
    pub out: String,
}

#[derive(Args, Debug)]
pub struct AnimArgs {
    pub input: String,
    #[arg(long)]
    pub name: String,
    #[arg(long)]
    pub dirs: Option<String>,
    #[arg(long = "macro")]
    pub macro_call: Option<String>,
    #[arg(long)]
    pub frames: Option<String>,
    #[arg(long)]
    pub fps: Option<u32>,
    #[arg(long)]
    pub no_loop: bool,
    #[arg(short, long)]
    pub out: String,
}

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
    #[arg(long, default_value = "default")]
    pub rules: String,
}

#[derive(Args, Debug)]
pub struct BuildArgs {
    pub recipe: String,
    #[arg(short, long)]
    pub out: String,
}
