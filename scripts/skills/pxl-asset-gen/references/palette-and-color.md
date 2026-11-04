# Palette and colour

## Roles

Reserve indices for: transparent/empty, outline, deep shadow, mid, light, accent (1–2), emissive/UI if needed. Name them in the brief.

## Limited sets

- Aim for 8–16 colours for characters; tiles can share a ground ramp.
- Separate hues for identity (team colours) from value ramps for form.
- Avoid near-duplicate greys that fight under scaled preview.

## Harmony habits

- Keep saturation higher on accents, lower on large fill masses.
- Skin / metal / cloth should not share one undifferentiated brown.
- Project consistency beats a “nicer” one-off palette.

## Tooling here

- Built-ins: `p16`, `tank` (`$PXL --json list palettes`).
- `palette ramp` for hue-shifted shadow/light steps.
- `palette apply` / `swap` to remap without redrawing topology.
