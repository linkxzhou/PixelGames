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

/// Shared text-to-spec options for `ascii import` and `ascii render`.
#[derive(Args, Debug)]
pub struct AsciiImportArgs {
    /// ASCII layout file (*.ascii.txt).
    pub input: String,
    /// digits (0-9, default) or printable (ASCII 0x20-0x7E).
    #[arg(long)]
    pub charset: Option<String>,
    /// Canvas side: 32 (default) or 64.
    #[arg(long)]
    pub size: Option<u16>,
    /// Cell size as N, NxM, or `fit` for the largest integer multiple.
    #[arg(long)]
    pub cell: Option<String>,
    /// center (default), bottom-center, or top-left.
    #[arg(long)]
    pub anchor: Option<String>,
    /// Palette file or built-in id.
    #[arg(long)]
    pub palette: Option<String>,
    /// Shared *.legend.json; stored as legend_ref.
    #[arg(long)]
    pub legend: Option<String>,
    /// Treat blocks as frames (default) or as layers.
    #[arg(long, default_value = "frames")]
    pub blocks: String,
    /// Pad ragged rows on the right instead of failing.
    #[arg(long)]
    pub pad: bool,
    /// Do not treat `::` lines as headers.
    #[arg(long)]
    pub no_directives: bool,
    /// sprite (default), tiles, effect, ui, or overlay.
    #[arg(long)]
    pub kind: Option<String>,
    #[arg(short, long)]
    pub out: String,
}

#[derive(Args, Debug)]
pub struct AsciiRenderArgs {
    /// *.ascii.txt (imported first) or *.ascii.json.
    pub input: String,
    #[arg(long)]
    pub charset: Option<String>,
    #[arg(long)]
    pub size: Option<u16>,
    #[arg(long)]
    pub cell: Option<String>,
    #[arg(long)]
    pub anchor: Option<String>,
    #[arg(long)]
    pub palette: Option<String>,
    #[arg(long)]
    pub legend: Option<String>,
    #[arg(long, default_value = "frames")]
    pub blocks: String,
    #[arg(long)]
    pub pad: bool,
    #[arg(long)]
    pub no_directives: bool,
    #[arg(long)]
    pub kind: Option<String>,
    #[arg(long)]
    pub anim: Option<String>,
    #[arg(long)]
    pub dir: Option<String>,
    #[arg(long, default_value_t = 1)]
    pub scale: u32,
    #[arg(long)]
    pub grid: bool,
    /// Write the compiled spec JSON next to the render.
    #[arg(long)]
    pub emit_json: Option<String>,
    /// Write the compiled pxl document (and palette, if generated).
    #[arg(long)]
    pub emit_doc: Option<String>,
    #[arg(short, long)]
    pub out: String,
}

#[derive(Args, Debug)]
pub struct AsciiRefineArgs {
    /// ASCII layout file (*.ascii.txt).
    pub input: String,
    /// digits (0-9, default) or printable (ASCII 0x20-0x7E).
    #[arg(long)]
    pub charset: Option<String>,
    /// copy (nearest 2x baseline) or corners (EPX edge rounding, default).
    #[arg(long, default_value = "corners")]
    pub mode: String,
    /// Do not treat `::` lines as headers.
    #[arg(long)]
    pub no_directives: bool,
    #[arg(short, long)]
    pub out: String,
}

#[derive(Args, Debug)]
pub struct ShapesRefineArgs {
    /// shapes spec (*.shapes.json).
    pub input: String,
    /// copy (nearest 2x baseline) or corners (EPX edge rounding, default).
    #[arg(long, default_value = "corners")]
    pub mode: String,
    #[arg(short, long)]
    pub out: String,
}

#[derive(Subcommand, Debug)]
pub enum ShapesCmd {
    /// Double a shapes spec into a pixel matrix an author can refine further.
    Refine(ShapesRefineArgs),
}

#[derive(Args, Debug)]
pub struct PrototypeDirArgs {
    /// Library directory holding prototype/, surface/ and base/ subdirectories.
    #[arg(long, value_name = "DIR")]
    pub library: String,
}

#[derive(Args, Debug)]
pub struct PrototypeExpandArgs {
    #[arg(long, value_name = "DIR")]
    pub library: String,
    /// asset-instance JSON file.
    pub instance: String,
    /// Where to write the expanded *.shapes.json.
    #[arg(short, long)]
    pub out: String,
    /// Accept prototypes and surfaces that no human has approved yet.
    #[arg(long)]
    pub allow_pending: bool,
}

#[derive(Args, Debug)]
pub struct PrototypeLockArgs {
    #[arg(long, value_name = "DIR")]
    pub library: String,
    /// Where to write catalog.lock.json.
    #[arg(short, long)]
    pub out: String,
    /// Include entries that no human has approved yet.
    #[arg(long)]
    pub allow_pending: bool,
}

#[derive(Subcommand, Debug)]
pub enum PrototypeCmd {
    /// Validate every prototype, surface and base file in a library.
    Check(PrototypeDirArgs),
    /// Expand one asset-instance into a shapes file, deterministically.
    Expand(PrototypeExpandArgs),
    /// Write catalog.lock.json for the whole library.
    Lock(PrototypeLockArgs),
}

#[derive(Args, Debug)]
pub struct SceneCompileArgs {
    /// Library directory holding tileset/ and module/ subdirectories.
    #[arg(long, value_name = "DIR")]
    pub library: String,
    /// Root module id to expand into a scene.
    #[arg(long)]
    pub module: String,
    /// Where to write the compiled scene JSON.
    #[arg(short, long)]
    pub out: String,
    /// Scene id recorded in the compiled output.
    #[arg(long)]
    pub scene: String,
    /// Scene bounds in pixels, WIDTHxHEIGHT (default: root module size × tile size).
    #[arg(long, value_name = "WxH")]
    pub bounds: Option<String>,
    /// Seed the expansion freezes; same seed, byte-identical scene.
    #[arg(long)]
    pub seed: u64,
    /// Include library entries that no human has approved yet.
    #[arg(long)]
    pub allow_pending: bool,
}

#[derive(Subcommand, Debug)]
pub enum SceneCmd {
    /// Compile a module tree into a deterministic compiled-scene/v1.
    Compile(SceneCompileArgs),
}

#[derive(Subcommand, Debug)]
pub enum SvgCmd {
    /// Rasterise an SVG to a palette-indexed PNG.
    Render(SvgRenderArgs),
    /// Report subset problems, colours and lost features without writing art.
    Lint(SvgLintArgs),
    /// Write the SVG as an ASCII layout an author can edit.
    Matrix(SvgMatrixArgs),
}

#[derive(Subcommand, Debug)]
pub enum AsciiCmd {
    /// Turn an ASCII grid into an *.ascii.json spec.
    Import(AsciiImportArgs),
    /// Compile an ASCII grid or spec and render a PNG/GIF in one step.
    Render(AsciiRenderArgs),
    /// Double the layout grid so every cell becomes an editable 2x2 block.
    Refine(AsciiRefineArgs),
}

#[derive(Args, Debug)]
pub struct SvgRenderArgs {
    /// SVG file, or `-` for stdin.
    pub input: String,
    /// Canvas side: 32, 64, 128 or 256.
    #[arg(long, default_value_t = 64)]
    pub size: u16,
    #[arg(long)]
    pub palette: Option<String>,
    /// strict (default), nearest, or extend.
    #[arg(long, default_value = "strict")]
    pub color_mode: String,
    /// Samples per pixel and axis; 0 picks by size.
    #[arg(long, default_value_t = 0)]
    pub samples: u8,
    #[arg(long, default_value_t = 4)]
    pub gradient_bands: usize,
    /// Skip the size tier's finishing pass.
    #[arg(long)]
    pub no_finish: bool,
    /// Fail on unsupported elements or approximated colours.
    #[arg(long)]
    pub strict: bool,
    /// Also write the document.
    #[arg(long)]
    pub doc: Option<String>,
    /// Extra sizes to write as `<stem>@<size>.png`.
    #[arg(long)]
    pub sizes: Option<String>,
    /// Keep transparency instead of compositing on the checker backdrop.
    #[arg(long)]
    pub asset: bool,
    #[arg(long, default_value_t = 1)]
    pub scale: u32,
    #[arg(short, long)]
    pub out: String,
}

#[derive(Args, Debug)]
pub struct SvgLintArgs {
    pub input: String,
    #[arg(long, default_value_t = 128)]
    pub size: u16,
    #[arg(long)]
    pub palette: Option<String>,
    #[arg(long, default_value = "nearest")]
    pub color_mode: String,
}

#[derive(Args, Debug)]
pub struct SvgMatrixArgs {
    pub input: String,
    #[arg(long, default_value_t = 64)]
    pub size: u16,
    #[arg(long)]
    pub palette: Option<String>,
    #[arg(long, default_value = "nearest")]
    pub color_mode: String,
    #[arg(short, long)]
    pub out: String,
}

#[derive(Args, Debug)]
pub struct PixelizeArgs {
    /// PNG to read.
    pub input: String,
    /// Target size. Omit to recover the image's own pixel grid.
    #[arg(long)]
    pub size: Option<u16>,
    /// nearest, mode, kcentroid (default), or contrast.
    #[arg(long, default_value = "kcentroid")]
    pub method: String,
    /// Colours to build when no palette is given.
    #[arg(long, default_value_t = 16)]
    pub colors: usize,
    #[arg(long)]
    pub palette: Option<String>,
    /// Bayer strength when mapping onto the palette.
    #[arg(long, default_value_t = 0)]
    pub dither: u8,
    /// Outline expansion radius in source pixels; omit to derive from the grid.
    #[arg(long)]
    pub outline: Option<usize>,
    /// Skip the finishing pass.
    #[arg(long)]
    pub no_finish: bool,
    /// Keep transparency and write the exact native pixels.
    #[arg(long)]
    pub asset: bool,
    #[arg(long, default_value_t = 1)]
    pub scale: u32,
    #[arg(long)]
    pub doc: Option<String>,
    #[arg(short, long)]
    pub out: String,
}

#[derive(Args, Debug)]
pub struct SpriteBuildArgs {
    /// A *.sprite.json source spec.
    pub input: String,
    #[arg(long)]
    pub palette: Option<String>,
    /// Write the document here (default `<stem>.pxl.json` beside -o).
    #[arg(long)]
    pub doc: Option<String>,
    /// Preview zoom.
    #[arg(long, default_value_t = 0)]
    pub zoom: u32,
    #[arg(short, long)]
    pub out: String,
}

#[derive(Args, Debug)]
pub struct ProfileArgs {
    /// 32, 64, 128 or 256. Omit for every tier.
    pub tier: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum SpriteCmd {
    /// Compile a *.sprite.json into a document and preview.
    Build(SpriteBuildArgs),
}

#[derive(Subcommand, Debug)]
pub enum AudioCmd {
    /// Check an audio-spec/v1 file and every song it names, writing nothing.
    Validate(AudioValidateArgs),
    /// Render a spec to WAV files plus audio-manifest.json.
    Render(AudioRenderArgs),
    /// Re-verify a rendered directory against its manifest, or describe one WAV.
    Inspect(AudioInspectArgs),
}

#[derive(Args, Debug)]
pub struct AudioValidateArgs {
    /// audio-spec/v1 JSON file.
    pub spec: String,
}

#[derive(Args, Debug)]
pub struct AudioRenderArgs {
    /// audio-spec/v1 JSON file.
    pub spec: String,
    /// Output directory (wav files and audio-manifest.json).
    #[arg(short, long)]
    pub out: String,
    /// Render only the sfx or bgm with this id.
    #[arg(long)]
    pub only: Option<String>,
    /// audio-toolchain directory holding cli.mjs (default: auto-detected).
    #[arg(long, value_name = "DIR")]
    pub adapter: Option<String>,
}

#[derive(Args, Debug)]
pub struct AudioInspectArgs {
    /// A directory written by `audio render`, or a single .wav file.
    pub path: String,
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
