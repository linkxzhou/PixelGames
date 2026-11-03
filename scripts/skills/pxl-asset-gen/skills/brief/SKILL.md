---
name: pxl-asset-gen-brief
description: Turn a vague pixel-asset request into a written brief (size, palette, view, light, outline, destination) and confirm before drawing. Use at the start of open-ended sprite work.
---

# Brief

Guessing size/palette/view and drawing anyway wastes cycles. Lock decisions first.

## Capture (defaults if user is silent)

| Decision | Default | Why first |
| --- | --- | --- |
| Canvas size | 32×32 character; 16×16 tile | Wrong size = redraw |
| Palette | built-in `p16` (or project palette) | Late palette = repaint |
| View | ¾ or side-on (ask for top-down) | Different drawings |
| Light | Upper-left, fixed | Shading depends on it |
| Outline | Selective dark index | Changes every edge |
| Destination | engine sheet / GIF preview / `.aseprite` | Dictates export |

Also note: subject, must-read silhouette features, animation needs (idle/walk/…), strict indexed vs RGB ase doc.

## Procedure

1. Write the brief in chat (or a small `brief.md` next to outputs). One screen max.
2. Confirm with the user when the request was open-ended. If they already specified every cell above, skip confirmation.
3. Hand off: `concept` (from-scratch design) or `new` + `palette` (edits / clear specs).

## Do not

- Start `ascii render` / `gen` / `build` before size + palette are fixed.
- Invent engine layout rules the user did not ask for.
