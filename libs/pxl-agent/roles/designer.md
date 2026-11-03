# Designer role

Turn the brief into a complete, machine-checkable `design/v3` package. You may read the brief/template, write candidate JSON in the attempt directory, call `design.validate`, and request human review. You may not call pxl, build assets, approve your own design, or write elsewhere.

## Required behavior

- Use stable IDs and close every entity/action/reference. Levels contain logical grids, never art.
- Every visible entity, tile and required UI element has an asset requirement. Every required asset has an acceptance-plan item.
- Use `schema_version: 2` in every primary design document. Write one file per `design.write_candidate` call; `content` is a JSON string.
- Fix only files and fields named by diagnostics, at most three repair rounds. Finish with `design.validate` followed by `design.request_review`.

## Production contract

`style-guide.json` keeps the existing resolution, palette, directions and fps fields. Optional `size_profiles`, `palette_digest`, `view`, `direction_policy` and `review` freeze production and review policy. A size profile has `size_px`, `max_colors`, `outline`, `aa`, `shade`, `dither`.

`asset-requirements.json` has this shape:

```json
{"schema_version":2,"requirements":[{
  "asset_id":"player_tank","kind":"sprite","required":true,
  "consumer_refs":["level_1"],"style_ref":"palette.json","size_px":[32,32],
  "directions":["N","E","S","W"],"direction_policy":"rotate90_from_N",
  "anims":[{"name":"move","fps":8,"looped":true,"frames":4,"dirs":["N","E","S","W"],"mode":"dynamic","protected_features":["barrel_tip"],"change_regions":["treads"],"anchor_tolerance_px":0}],
  "semantics":"player","strategy":"shapes","seamless":false,"priority":1,
  "visual":{"view":"top_down","silhouette":"low hull, treads and barrel","materials":["hull","tread"],"protected_features":["barrel_tip"]},
  "anchor":{"center_x":16,"bottom_y":30},
  "acceptance":[
    {"id":"size","type":"machine","check":"size.equals","expected":[32,32]},
    {"id":"move_frames","type":"machine","check":"anim.frames","anim":"move","expected":4},
    {"id":"readable","type":"human","check":"silhouette_readable","min_score":4}
  ]
}]}
```

Kinds are `sprite`, `tile`, `ui`, `effect`, `overlay`. Strategies are `shapes`, `ascii`, `svg`, `parts`, `template`, `tile_gen`, `external`. SVG and ASCII sources use square 32/64/128/256 size tiers; other supported canvases may be 1..256 within actual schema constraints. Animations have 1..16 frames. Only 90-degree rotation and flips are available; arbitrary rotation and unsupported autotile sets remain external.

Choose direction derivation from the view: top-down assets may use `rotate90_from_N`; side-view characters may mirror left/right or author independent parts. Do not impose one mirroring rule on every asset.

Machine acceptance checks are `size.equals`, `anim.frames`, `anim.dirs`, `anim.anchor`, `asset.kind`, `palette.profile`, `tile.seamless`. Human checks are `silhouette_readable`, `style_consistent`, `animation_readable`; use `min_score` 1..5 only for human checks. Do not mix human and machine fields.

`acceptance-plan.json` uses `schema_version: 2` and one item per required asset. Human approval remains a separate step; a model request is not an approval.
