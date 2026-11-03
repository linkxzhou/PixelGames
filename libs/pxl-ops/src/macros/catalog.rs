pub const MACRO_NAMES: &[&str] = &[
    "tread_cycle",
    "idle_rumble",
    "idle_breathe",
    "bob",
    "walk_side",
    "run_side",
    "hurt_flash",
    "death_dissolve",
    "blink",
    "spin_projectile",
    "pulse_glow",
    "doll",
];

/// Arguments each macro reads. A name outside this list is a typo or an option
/// the macro does not implement, and silently ignoring it hides the mistake.
pub(super) fn allowed_args(name: &str) -> &'static [&'static str] {
    match name {
        "tread_cycle" => &["steps", "cycle", "bob", "region"],
        "idle_rumble" => &["steps", "amp"],
        "idle_breathe" => &["steps", "period", "amp"],
        "bob" => &["amp"],
        "walk_side" | "run_side" => &["stride", "bob"],
        "hurt_flash" => &["steps"],
        "death_dissolve" => &["steps", "seed", "pattern"],
        "blink" => &["x", "y", "w", "h", "lid"],
        "spin_projectile" => &["steps"],
        "pulse_glow" => &["ramp"],
        "doll" => &["anim"],
        _ => &[],
    }
}

/// Default values per argument, mirroring the `arg_*` helpers used at expand
/// time. `catalogue_defaults_and_frame_counts_match_real_expansion` expands each
/// macro bare and with these values spelled out and requires identical ops, so
/// the listing cannot drift from the implementation. Arguments absent here have
/// no fixed default: they are either required (`required_args`) or derived from
/// the sprite (`blink`'s `lid`).
pub(super) fn arg_defaults(name: &str) -> &'static [(&'static str, &'static str)] {
    match name {
        "tread_cycle" => &[
            ("steps", "4"),
            ("cycle", "tread"),
            ("bob", "1"),
            ("region", "auto_tread"),
        ],
        "idle_rumble" => &[("steps", "2"), ("amp", "1")],
        // `period` is the documented frame count; `steps` is the older spelling.
        "idle_breathe" => &[("steps", "4"), ("period", "4"), ("amp", "1")],
        "bob" => &[("amp", "1")],
        "walk_side" => &[("stride", "1"), ("bob", "1")],
        "run_side" => &[("stride", "2"), ("bob", "1")],
        "hurt_flash" => &[("steps", "2")],
        "death_dissolve" => &[("steps", "6"), ("seed", "7"), ("pattern", "bayer4")],
        "spin_projectile" => &[("steps", "4")],
        "pulse_glow" => &[("ramp", "olive")],
        "doll" => &[("anim", "idle")],
        _ => &[],
    }
}

/// Arguments the macro refuses to run without.
pub(super) fn required_args(name: &str) -> &'static [&'static str] {
    match name {
        "blink" => &["x", "y", "w", "h"],
        _ => &[],
    }
}

/// Frames a macro emits when called with no frame-count argument.
pub(super) fn macro_frames(name: &str) -> u32 {
    match name {
        "tread_cycle" => 4,
        "idle_rumble" | "hurt_flash" => 2,
        "idle_breathe" => 4,
        "bob" => 4,
        "walk_side" | "run_side" => 4,
        "death_dissolve" => 6,
        "blink" => 2,
        "spin_projectile" => 4,
        // pulse_glow drives a brightness pulse across two frames.
        "pulse_glow" => 2,
        "doll" => 1,
        _ => 0,
    }
}

/// One macro argument as exposed by `pxl list macros`.
pub struct MacroArgInfo {
    pub name: &'static str,
    /// Default used when the argument is omitted. `None` with `required` false
    /// means the macro derives the value from the sprite.
    pub default: Option<&'static str>,
    pub required: bool,
}

/// Structured metadata for `pxl list macros`: arguments, defaults, output
/// frame count and an example. Derived from `allowed_args`/`arg_defaults` so
/// the listing and the validator share one source of truth.
pub struct MacroInfo {
    pub name: &'static str,
    pub args: Vec<MacroArgInfo>,
    pub frames: u32,
    pub example: String,
}

pub fn macro_info(name: &str) -> Option<MacroInfo> {
    let canonical = MACRO_NAMES.iter().find(|n| **n == name)?;
    let args = allowed_args(canonical)
        .iter()
        .map(|a| MacroArgInfo {
            name: a,
            default: arg_defaults(canonical)
                .iter()
                .find(|(k, _)| k == a)
                .map(|(_, v)| *v),
            required: required_args(canonical).contains(a),
        })
        .collect();
    Some(MacroInfo {
        name: canonical,
        args,
        frames: macro_frames(canonical),
        example: example_call(canonical),
    })
}

pub(super) fn example_call(name: &str) -> String {
    match name {
        "tread_cycle" => "tread_cycle(steps=4, region=auto_tread, cycle=tread, bob=1)".into(),
        "idle_rumble" => "idle_rumble(steps=2, amp=1)".into(),
        "idle_breathe" => "idle_breathe(period=4, amp=1)".into(),
        "bob" => "bob(amp=1)".into(),
        "walk_side" => "walk_side(stride=1, bob=1)".into(),
        "run_side" => "run_side(stride=2, bob=1)".into(),
        "hurt_flash" => "hurt_flash(steps=2)".into(),
        "death_dissolve" => "death_dissolve(steps=6)".into(),
        // The eye rect is mandatory; `lid` is optional (defaults to the skin tone).
        "blink" => "blink(x=5, y=6, w=4, h=1)".into(),
        "spin_projectile" => "spin_projectile(steps=4)".into(),
        // No `steps` argument exists; frame count follows the ramp pulse.
        "pulse_glow" => "pulse_glow(ramp=water)".into(),
        "doll" => "doll(anim=walk)".into(),
        _ => name.to_string(),
    }
}

/// Full listing consumed by the CLI and the harness capability tool.
pub fn macro_catalog() -> Vec<MacroInfo> {
    MACRO_NAMES.iter().filter_map(|n| macro_info(n)).collect()
}
