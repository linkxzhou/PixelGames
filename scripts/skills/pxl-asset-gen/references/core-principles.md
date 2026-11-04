# Core principles

1. **Readability over detail.** If it fails at 1×, more pixels will not save it.
2. **Decide before drawing.** Size, palette, view, light — then pixels.
3. **Big → small.** Silhouette → masses → edges → texture → polish.
4. **Clusters, not confetti.** Same-index pixels should form deliberate shapes.
5. **One light.** Direction stays fixed across the sprite and its frames.
6. **Palette is a design tool.** Fewer colours with clear roles beat RGB soup.
7. **Look every batch.** Render scaled PNG; fix from evidence, not memory.
8. **Pixels from the `pxl-asset-gen` binary only.** Specs (ASCII/shapes/recipe) are the source of truth.
9. **Cheap mistakes first.** Wrong silhouette costs less than wrong eyelash.
10. **Ship with proof.** `validate` + cold read before export.
