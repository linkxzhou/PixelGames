# pxl-format (crates/pxl-format)

Pure data layer. Key order is stable (struct fields and `BTreeMap`), so the same value always encodes to the same bytes.

## API

- Parsers: `parse_json::<T>(text, what)`, `parse_palette`, `parse_template`, `parse_shapes`, `parse_parts`, `parse_ops_value` (object or array), `parse_ascii_spec`, `parse_ascii_text`, `parse_legend_file`; `to_pretty` for output.
- Models: `PxlDoc { v, palette, w, h, frames, anims, meta }`, `Anim`, `Op` (+ `OP_NAMES`), `Recipe`, `ShapesFile`, `PartsFile`, `PaletteFile`, `AsciiSpec`.
- Schemas: `schema_json(kind)` for `recipe`, `ops`, `op`, `pxl`, `palette`, `parts`, `sheet` (`schema_names()`); CLI `step-300 schema <kind>`. There is no schema kind for `shapes` or `ascii`.
- Errors are `PxlError::schema` with hints such as "unknown field `axiz`, expected `axis`".

## Document shapes

- `*.pxl.json`: `{"v":1,"palette":"p16","w":8,"h":8,"frames":{"base":["..11.."]},"anims":{...},"meta":{...}}`. Frame rows use the pxl-core text matrix.
- Recipe (`step-300 build`): `asset`, `palette`, `seed`, `sprites[]` (`base` generator, `post` ops, `directions`, `anims`), `tiles[]`, `sheet`, `preview[]`, `validate`. See `examples/tank/tank.recipe.json`.
- ASCII spec: `charset`, `size`, `cell`, `anchor`, `palette`, `legend_ref`, `frames` (rows, or `from` + `patch`), `anims`. See `examples/ascii/slime/slime.ascii.json`. Ops use `deny_unknown_fields`, so typos fail loudly.

## Rules when editing

- Adding a field: use `#[serde(default)]` so old files still load, update the `JsonSchema` derive, and add a test; keep output ordering deterministic (no `HashMap`).
- Adding an op: update `Op`, `name()`, and `OP_NAMES` together.

## Example: parse ops, a palette, and see a schema error

```rust
use pxl_format::{parse_ops_value, parse_palette, schema_json};

fn main() -> pxl_core::PxlResult<()> {
    let ops = parse_ops_value(r#"[{"op":"flip","axis":"x"},{"op":"shift","dx":1}]"#)?;
    println!("{} ops: {}", ops.len(), ops.iter().map(|o| o.name()).collect::<Vec<_>>().join(","));

    let palette = parse_palette(
        r##"{"colors":["#00000000","#140c1c","#d04648"],"ramps":{"red":[1,2]},"special":{"transparent":0,"outline":1,"flash":2}}"##,
        "mini",
    )?;
    println!("palette {} colors", palette.len());
    println!("{}", schema_json("op")?["title"]);

    let err = parse_ops_value(r#"{"op":"flip","axiz":"x"}"#).unwrap_err();
    println!("{:?}: {}", err.code, err.message);
    Ok(())
}
```

Output (verified): `2 ops: flip,shift`, `palette 3 colors`, `"Op"`, and `Schema: op does not match the schema: unknown field `axiz`, expected `axis``. A palette whose `special` indexes exceed its colours is rejected, so set `special` for tiny palettes.

Run tests: `cargo test -p pxl-format`.
