//! Parser for the ASCII layout text: blocks of rows separated by blank lines,
//! with optional `:: name [frame|layer]` headers.

use pxl_core::{PxlError, PxlResult};

use crate::ascii::{Charset, RowEncoding};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Frames,
    Layers,
}

impl BlockKind {
    pub fn parse(value: &str) -> PxlResult<Self> {
        match value {
            "frames" => Ok(BlockKind::Frames),
            "layers" => Ok(BlockKind::Layers),
            other => Err(PxlError::usage(format!("unknown block kind '{other}'"))
                .with_hint("--blocks is frames or layers")),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            BlockKind::Frames => "frame",
            BlockKind::Layers => "layer",
        }
    }

    fn auto_prefix(self) -> &'static str {
        match self {
            BlockKind::Frames => "f",
            BlockKind::Layers => "layer",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AsciiTextOptions {
    pub charset: Charset,
    pub blocks: BlockKind,
    /// Pad ragged rows on the right instead of failing.
    pub pad: bool,
    /// Parse `:: name` header lines.
    pub directives: bool,
    pub file: Option<String>,
}

impl AsciiTextOptions {
    pub fn new(charset: Charset, blocks: BlockKind) -> Self {
        Self {
            charset,
            blocks,
            pad: false,
            directives: true,
            file: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AsciiBlock {
    pub name: String,
    pub kind: BlockKind,
    /// 1-based line of the first row.
    pub line: usize,
    pub rows: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct AsciiText {
    pub blocks: Vec<AsciiBlock>,
    pub width: usize,
    pub height: usize,
    /// `:: size N` from the file, when present.
    pub size: Option<u16>,
    /// `:: encoding rows|rle` from the file, when present.
    pub encoding: Option<RowEncoding>,
    /// Non-fatal findings, such as rows padded by `--pad`.
    pub warnings: Vec<String>,
}

/// A `::` line that sets an option instead of naming a block.
enum Directive {
    Block(String, Option<BlockKind>),
    Setting(String, String),
}

struct Pending {
    name: String,
    kind: BlockKind,
    line: usize,
    rows: Vec<String>,
    width: usize,
}

pub fn parse_ascii_text(text: &str, opts: &AsciiTextOptions) -> PxlResult<AsciiText> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut blocks: Vec<AsciiBlock> = Vec::new();
    let mut names: Vec<String> = Vec::new();
    let mut pending_name: Option<(String, Option<BlockKind>)> = None;
    let mut current: Option<Pending> = None;
    let mut auto_index = 0usize;
    let mut warnings: Vec<String> = Vec::new();
    let mut size: Option<u16> = None;
    let mut encoding: Option<RowEncoding> = None;

    for (index, raw) in text.split('\n').enumerate() {
        let line_no = index + 1;
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if opts.directives && line.starts_with("::") {
            flush(&mut current, &mut blocks)?;
            match parse_directive(line, line_no, opts)? {
                Directive::Block(name, kind) => pending_name = Some((name, kind)),
                Directive::Setting(key, value) => match key.as_str() {
                    "size" => {
                        let n: u16 = value.parse().map_err(|_| {
                            bad_directive(opts, line_no, "size takes a number: 32, 64, 128 or 256")
                        })?;
                        if !pxl_core::SIZE_TIERS.contains(&n) {
                            return Err(bad_directive(opts, line_no, "size is 32, 64, 128 or 256"));
                        }
                        size = Some(n);
                    }
                    "encoding" => {
                        encoding = Some(
                            RowEncoding::parse(&value)
                                .map_err(|e| bad_directive(opts, line_no, &e.message))?,
                        );
                    }
                    other => {
                        return Err(bad_directive(
                            opts,
                            line_no,
                            &format!("'{other}' is not a setting; settings are size and encoding"),
                        ))
                    }
                },
            }
            continue;
        }
        if line.trim().is_empty() {
            flush(&mut current, &mut blocks)?;
            continue;
        }
        for (col, c) in line.chars().enumerate() {
            check_char(c, line_no, col + 1, opts)?;
        }
        let width = line.chars().count();
        let block = match current.as_mut() {
            Some(block) => block,
            None => {
                let (name, kind) = match pending_name.take() {
                    Some((name, kind)) => (name, kind.unwrap_or(opts.blocks)),
                    None => {
                        let name = loop {
                            let candidate = if opts.blocks == BlockKind::Frames && auto_index == 0 {
                                "base".to_string()
                            } else {
                                format!("{}{auto_index}", opts.blocks.auto_prefix())
                            };
                            auto_index += 1;
                            if !names.contains(&candidate) {
                                break candidate;
                            }
                        };
                        (name, opts.blocks)
                    }
                };
                if names.contains(&name) {
                    return Err(PxlError::schema(format!(
                        "ascii.duplicate_block: '{}' is used twice",
                        name
                    ))
                    .with_hint("give each block a unique `:: name` header"));
                }
                names.push(name.clone());
                current.insert(Pending {
                    name,
                    kind,
                    line: line_no,
                    rows: Vec::new(),
                    width,
                })
            }
        };
        if width != block.width {
            if !opts.pad {
                return Err(PxlError::schema(format!(
                    "ascii.ragged_row: {} row {} has width {}, expected {}",
                    location(opts, line_no, 1),
                    block.rows.len() + 1,
                    width,
                    block.width
                ))
                .with_hint("pad the row with the transparent character, or pass --pad"));
            }
            warnings.push(format!(
                "ascii.padded_row: {} has width {}, padded to match its block",
                location(opts, line_no, 1),
                width
            ));
            if width > block.width {
                let pad = opts.charset.pad_char().to_string();
                for row in block.rows.iter_mut() {
                    while row.chars().count() < width {
                        row.push_str(&pad);
                    }
                }
                block.width = width;
            }
        }
        let mut row = line.to_string();
        while row.chars().count() < block.width {
            row.push(opts.charset.pad_char());
        }
        block.rows.push(row);
    }
    flush(&mut current, &mut blocks)?;
    if blocks.is_empty() {
        return Err(
            PxlError::schema("ascii.no_blocks: the file has no layout rows")
                .with_hint("write digits 0-9 (0 transparent) one character per cell"),
        );
    }
    let width = blocks
        .iter()
        .map(|b| b.rows.first().map(|r| r.chars().count()).unwrap_or(0))
        .max()
        .unwrap_or(0);
    let height = blocks.iter().map(|b| b.rows.len()).max().unwrap_or(0);
    Ok(AsciiText {
        blocks,
        width,
        height,
        size,
        encoding,
        warnings,
    })
}

fn flush(current: &mut Option<Pending>, blocks: &mut Vec<AsciiBlock>) -> PxlResult<()> {
    let Some(block) = current.take() else {
        return Ok(());
    };
    blocks.push(AsciiBlock {
        name: block.name,
        kind: block.kind,
        line: block.line,
        rows: block.rows,
    });
    Ok(())
}

fn parse_directive(line: &str, line_no: usize, opts: &AsciiTextOptions) -> PxlResult<Directive> {
    let rest = line.trim_start_matches("::").trim();
    let mut parts = rest.split_whitespace();
    let Some(name) = parts.next() else {
        return Err(bad_directive(opts, line_no, "a `::` line needs a name")
            .with_hint("write `:: base`, `:: body layer`, `:: size 128` or `:: encoding rle`"));
    };
    if !valid_name(name) {
        return Err(bad_directive(
            opts,
            line_no,
            "names are lowercase letters, digits, and underscores",
        ));
    }
    if matches!(name, "size" | "encoding") {
        let Some(value) = parts.next() else {
            return Err(bad_directive(
                opts,
                line_no,
                &format!("::{name} needs a value"),
            ));
        };
        if parts.next().is_some() {
            return Err(bad_directive(opts, line_no, "a setting is `:: name value`"));
        }
        return Ok(Directive::Setting(name.to_string(), value.to_string()));
    }
    let kind = match parts.next() {
        None => None,
        Some("frame") => Some(BlockKind::Frames),
        Some("layer") => Some(BlockKind::Layers),
        Some(other) => {
            return Err(bad_directive(
                opts,
                line_no,
                &format!("'{other}' is not frame or layer"),
            ))
        }
    };
    if parts.next().is_some() {
        return Err(bad_directive(
            opts,
            line_no,
            "a `::` line is `:: name [frame|layer]`",
        ));
    }
    Ok(Directive::Block(name.to_string(), kind))
}

fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn bad_directive(opts: &AsciiTextOptions, line_no: usize, message: &str) -> PxlError {
    PxlError::schema(format!(
        "ascii.bad_directive: {} - {message}",
        location(opts, line_no, 1)
    ))
    .with_hint("pass --no-directives when the layout itself starts with `::`")
}

fn check_char(c: char, line: usize, col: usize, opts: &AsciiTextOptions) -> PxlResult<()> {
    if c == '\t' {
        return Err(PxlError::schema(format!(
            "ascii.tab: {} contains a tab",
            location(opts, line, col)
        ))
        .with_hint("use the transparent character instead of tabs"));
    }
    if !c.is_ascii() {
        return Err(PxlError::schema(format!(
            "ascii.non_ascii: {} contains '{c}'",
            location(opts, line, col)
        ))
        .with_hint(
            "one cell is one printable ASCII character; full-width characters count as two columns",
        ));
    }
    if !opts.charset.accepts(c) {
        return Err(PxlError::schema(format!(
            "ascii.bad_char: {} contains '{c}', which charset {} does not allow",
            location(opts, line, col),
            opts.charset.name()
        ))
        .with_hint(match opts.charset {
            Charset::Digits => "use 0-9 (0 transparent); pass --charset printable for symbols",
            Charset::Printable => "charset printable allows ASCII 0x20-0x7E",
        }));
    }
    Ok(())
}

fn location(opts: &AsciiTextOptions, line: usize, col: usize) -> String {
    match &opts.file {
        Some(file) => format!("{file}:{line}:{col}"),
        None => format!("line {line}, column {col}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(charset: Charset, blocks: BlockKind) -> AsciiTextOptions {
        AsciiTextOptions::new(charset, blocks)
    }

    #[test]
    fn splits_blocks_on_blank_lines_and_names_them() {
        let text = ":: base\n01\n10\n\n:: blink\n11\n00\n";
        let got = parse_ascii_text(text, &opts(Charset::Digits, BlockKind::Frames)).unwrap();
        assert_eq!(got.blocks.len(), 2);
        assert_eq!(got.blocks[0].name, "base");
        assert_eq!(got.blocks[0].line, 2);
        assert_eq!(got.blocks[1].name, "blink");
        assert_eq!(got.width, 2);
        assert_eq!(got.height, 2);
    }

    #[test]
    fn auto_names_start_at_base_and_do_not_collide_with_headers() {
        let text = ":: base\n01\n\n12\n";
        let got = parse_ascii_text(text, &opts(Charset::Digits, BlockKind::Frames)).unwrap();
        assert_eq!(got.blocks[0].name, "base");
        assert_eq!(got.blocks[1].name, "f1");

        let single = parse_ascii_text("01\n", &opts(Charset::Digits, BlockKind::Frames)).unwrap();
        assert_eq!(single.blocks[0].name, "base");
    }

    #[test]
    fn digits_rejects_letters_and_dots_with_a_location() {
        let mut o = opts(Charset::Digits, BlockKind::Frames);
        o.file = Some("a.txt".into());
        let err = parse_ascii_text("01.\n", &o).unwrap_err();
        assert!(err.message.contains("ascii.bad_char"), "{}", err.message);
        assert!(err.message.contains("a.txt:1:3"), "{}", err.message);
    }

    #[test]
    fn tabs_and_non_ascii_are_reported() {
        let o = opts(Charset::Printable, BlockKind::Frames);
        assert!(parse_ascii_text("01\t\n", &o)
            .unwrap_err()
            .message
            .contains("ascii.tab"));
        assert!(parse_ascii_text("01é\n", &o)
            .unwrap_err()
            .message
            .contains("ascii.non_ascii"));
    }

    #[test]
    fn ragged_rows_fail_unless_padded() {
        let o = opts(Charset::Digits, BlockKind::Frames);
        let err = parse_ascii_text("012\n01\n", &o).unwrap_err();
        assert!(err.message.contains("ascii.ragged_row"), "{}", err.message);
        let mut pad = opts(Charset::Digits, BlockKind::Frames);
        pad.pad = true;
        let got = parse_ascii_text("012\n01\n", &pad).unwrap();
        assert_eq!(got.blocks[0].rows[1], "010");
        assert_eq!(got.warnings.len(), 1);
        assert!(got.warnings[0].contains("ascii.padded_row"));
    }

    #[test]
    fn bad_directive_points_at_the_line() {
        let o = opts(Charset::Digits, BlockKind::Frames);
        let err = parse_ascii_text(":: Bad Name\n01\n", &o).unwrap_err();
        assert!(err.message.contains("ascii.bad_directive"));
        let err = parse_ascii_text(":: base sprite\n01\n", &o).unwrap_err();
        assert!(err.message.contains("frame or layer"));
    }

    #[test]
    fn directives_can_be_turned_off() {
        let mut o = opts(Charset::Printable, BlockKind::Frames);
        o.directives = false;
        let got = parse_ascii_text("::ab\n", &o).unwrap();
        assert_eq!(got.blocks[0].rows[0], "::ab");
    }

    #[test]
    fn empty_input_is_an_error() {
        let o = opts(Charset::Digits, BlockKind::Frames);
        assert!(parse_ascii_text("\n\n", &o)
            .unwrap_err()
            .message
            .contains("ascii.no_blocks"));
    }
}
