//! Double an ASCII layout grid so each cell becomes a 2x2 block that an author
//! can edit pixel by pixel.
//!
//! This is a text-to-text transform: characters are still legend keys or
//! palette indices, only the grid gets denser. `copy` is the honest baseline
//! (a nearest 2x, no new information). `corners` applies the EPX edge rule so
//! the draft stops looking like 2x2 squares, but it can only reuse colours that
//! already touch the block: real detail still has to be drawn by hand.

use std::collections::{BTreeMap, BTreeSet};

use pxl_core::{PxlError, PxlResult};

use crate::ascii::{Charset, RowEncoding};
use crate::ascii_text::{AsciiBlock, AsciiText, BlockKind};

/// How a source cell fills its four refined sub-pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefineMode {
    /// Four identical sub-pixels: a plain nearest 2x, kept as a control.
    Copy,
    /// EPX: a sub-pixel takes a neighbour's colour when two adjacent
    /// neighbours agree, which rounds off blocky corners.
    Corners,
}

impl RefineMode {
    pub fn parse(value: &str) -> PxlResult<Self> {
        match value {
            "copy" => Ok(RefineMode::Copy),
            "corners" => Ok(RefineMode::Corners),
            other => Err(PxlError::usage(format!("unknown refine mode '{other}'"))
                .with_hint("mode is copy (nearest 2x) or corners (EPX edge rounding)")),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            RefineMode::Copy => "copy",
            RefineMode::Corners => "corners",
        }
    }
}

/// Double every block. Block names, order, and frame/layer kind are preserved;
/// row counts and widths are exactly doubled.
pub fn refine_text(text: &AsciiText, charset: Charset, mode: RefineMode) -> PxlResult<AsciiText> {
    let blocks = text
        .blocks
        .iter()
        .map(|block| refine_block(block, charset, mode))
        .collect::<PxlResult<Vec<_>>>()?;
    Ok(AsciiText {
        size: text.size,
        encoding: Some(RowEncoding::Rows),
        blocks,
        width: text.width * 2,
        height: text.height * 2,
        warnings: Vec::new(),
    })
}

fn refine_block(block: &AsciiBlock, charset: Charset, mode: RefineMode) -> PxlResult<AsciiBlock> {
    let grid: Vec<Vec<char>> = block.rows.iter().map(|row| row.chars().collect()).collect();
    Ok(AsciiBlock {
        name: block.name.clone(),
        kind: block.kind,
        line: block.line,
        rows: refine_rows(&grid, charset, mode)?,
    })
}

/// Layout characters are legend keys, not palette indices, so they are mapped to
/// dense ids for [`pxl_core::epx2x`] and mapped straight back. Every transparent
/// character collapses to id 0, which keeps `.` and a space interchangeable in
/// printable layouts.
fn refine_rows(grid: &[Vec<char>], charset: Charset, mode: RefineMode) -> PxlResult<Vec<String>> {
    let width = grid.iter().map(Vec::len).max().unwrap_or(0);
    let height = grid.len();
    if width == 0 || height == 0 {
        return Ok(Vec::new());
    }
    let plain = |c: char| !charset.default_transparent().contains(&c);
    let mut used: BTreeSet<char> = BTreeSet::new();
    for row in grid {
        used.extend(row.iter().copied().filter(|c| plain(*c)));
    }
    let mut to_id: BTreeMap<char, u8> = BTreeMap::new();
    for (i, c) in used.into_iter().enumerate() {
        let id = u8::try_from(i + 1).map_err(|_| {
            PxlError::schema(
                "ascii.refine_colors: the layout uses more than 254 distinct characters",
            )
        })?;
        to_id.insert(c, id);
    }
    let mut frame = pxl_core::IndexFrame::new(width as u16, height as u16)?;
    for (y, row) in grid.iter().enumerate() {
        for (x, c) in row.iter().enumerate() {
            if let Some(id) = to_id.get(c) {
                frame.set(x as i32, y as i32, *id);
            }
        }
    }
    let refined = match mode {
        RefineMode::Copy => pxl_core::double_copy(&frame)?,
        RefineMode::Corners => pxl_core::epx2x(&frame)?,
    };
    let mut back = vec![charset.pad_char(); 255];
    for (c, id) in &to_id {
        back[*id as usize] = *c;
    }
    let rows = pxl_core::encode_matrix(&refined)?;
    Ok(rows
        .into_iter()
        .map(|row| {
            row.chars()
                .map(|c| {
                    let id = pxl_core::decode_index(c).unwrap_or(0);
                    if id == 0 {
                        charset.pad_char()
                    } else {
                        back[id as usize]
                    }
                })
                .collect()
        })
        .collect())
}

/// Render a refined layout back to `*.ascii.txt`, with a `:: name` header for
/// every block so frame and layer types survive the round trip.
pub fn format_ascii_text(text: &AsciiText) -> String {
    let mut out = String::new();
    for (i, block) in text.blocks.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(":: ");
        out.push_str(&block.name);
        if block.kind == BlockKind::Layers {
            out.push_str(" layer");
        }
        out.push('\n');
        for row in &block.rows {
            out.push_str(row);
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ascii_text::{parse_ascii_text, AsciiTextOptions};

    fn options(charset: Charset) -> AsciiTextOptions {
        AsciiTextOptions::new(charset, BlockKind::Frames)
    }

    fn refine(text: &str, mode: RefineMode) -> Vec<String> {
        let parsed = parse_ascii_text(text, &options(Charset::Digits)).unwrap();
        refine_text(&parsed, Charset::Digits, mode).unwrap().blocks[0]
            .rows
            .clone()
    }

    fn rows(text: &[&str]) -> Vec<String> {
        text.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn copy_doubles_every_cell_without_changing_it() {
        assert_eq!(
            refine("12\n03\n", RefineMode::Copy),
            rows(&["1122", "1122", "0033", "0033"])
        );
    }

    #[test]
    fn a_single_cell_becomes_a_two_by_two_block() {
        assert_eq!(refine("1\n", RefineMode::Copy), rows(&["11", "11"]));
    }

    #[test]
    fn copy_is_deterministic_and_keeps_the_grid_shape() {
        let once = refine("123\n456\n", RefineMode::Copy);
        assert_eq!(once, refine("123\n456\n", RefineMode::Copy));
        assert_eq!(once.len(), 4);
        assert!(once.iter().all(|row| row.chars().count() == 6));
    }

    #[test]
    fn corners_leaves_an_isolated_cell_square() {
        // All four neighbours are the same background, so no pair of adjacent
        // neighbours differs from the other pair and nothing is rounded.
        assert_eq!(refine("1\n", RefineMode::Corners), rows(&["11", "11"]));
    }

    #[test]
    fn corners_round_a_detached_block_but_never_the_canvas_edge() {
        // A transparent margin gives the block room to round.
        let source = "0000\n0110\n0110\n0000\n";
        assert_ne!(
            refine(source, RefineMode::Corners),
            refine(source, RefineMode::Copy)
        );
        let got = refine(source, RefineMode::Corners);
        assert_eq!(got[2].chars().nth(2), Some('0'), "outer corner rounds away");
        assert_eq!(got[3].chars().nth(3), Some('1'), "the body stays solid");

        // A layout that fills its grid keeps its border: rounding there would
        // clip the sprite against the canvas edge.
        assert_eq!(
            refine("11\n10\n", RefineMode::Corners),
            rows(&["1111", "1111", "1100", "1100"])
        );
    }

    #[test]
    fn corners_keeps_the_interior_of_a_filled_area() {
        // The centre cell of a 3x3 fill has four equal neighbours.
        let got = refine("111\n111\n111\n", RefineMode::Corners);
        for row in &got[2..4] {
            let chars: Vec<char> = row.chars().collect();
            assert_eq!(&chars[2..4], &['1', '1']);
        }
    }

    #[test]
    fn corners_never_invents_a_colour() {
        let got = refine("12\n30\n", RefineMode::Corners);
        for row in &got {
            for c in row.chars() {
                assert!("1230".contains(c), "unexpected {c}");
            }
        }
    }

    #[test]
    fn corners_differs_from_a_plain_copy_on_a_diagonal_edge() {
        let source = "0011\n0111\n1111\n";
        assert_ne!(
            refine(source, RefineMode::Corners),
            refine(source, RefineMode::Copy)
        );
    }

    #[test]
    fn transparent_cells_stay_transparent() {
        assert_eq!(
            refine("00\n00\n", RefineMode::Corners),
            rows(&["0000", "0000", "0000", "0000"])
        );
    }

    #[test]
    fn printable_treats_dot_and_space_as_the_same_background() {
        let mut opts = options(Charset::Printable);
        opts.directives = false;
        let parsed = parse_ascii_text("a.\n a\n", &opts).unwrap();
        let refined = refine_text(&parsed, Charset::Printable, RefineMode::Corners).unwrap();
        assert_eq!((refined.width, refined.height), (4, 4));
        assert_eq!(refined.blocks[0].rows.len(), 4);
    }

    #[test]
    fn block_names_and_layer_kind_survive_a_round_trip() {
        let source = ":: body layer\n11\n\n:: base\n22\n";
        let parsed = parse_ascii_text(source, &options(Charset::Digits)).unwrap();
        let refined = refine_text(&parsed, Charset::Digits, RefineMode::Copy).unwrap();
        let formatted = format_ascii_text(&refined);
        let reparsed = parse_ascii_text(&formatted, &options(Charset::Digits)).unwrap();
        assert_eq!(reparsed.blocks.len(), 2);
        assert_eq!(reparsed.blocks[0].name, "body");
        assert_eq!(reparsed.blocks[0].kind, BlockKind::Layers);
        assert_eq!(reparsed.blocks[1].name, "base");
        assert_eq!(reparsed.blocks[0].rows, refined.blocks[0].rows);
        // Each source block was one row of two cells.
        assert_eq!((reparsed.width, reparsed.height), (4, 2));
        assert!(reparsed.warnings.is_empty());
    }
}
