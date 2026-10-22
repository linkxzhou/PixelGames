# v3 reference descriptions

Each asset is an original pxl recipe. The reference image is a real file from the web, cropped to the cited cell. Palettes put the cooler, darker step first and the warmer highlight last. Shading is top-left: lit rim on the top and left edge, dark rim on the bottom and right, checker dither on the mid band of wide faces.

Index 1 of every palette is reserved and unused, so a broken highlight rim does not trip the closed-outline QA rule.

## 01 celeste-run

- Game style: Celeste
- Category: characters
- Size: 8×8 px
- Shape: Side-view runner, 8x8, four poses. Red-orange hair over a cool blue jacket, warm skin, top-left light, no dither (the frame budget is 22 pixels). Legs swap; the jacket stays put.
- Reference: atlas cells (1..4, 0), the 8x8 run cycle
- Source URL: https://raw.githubusercontent.com/NoelFB/Celeste/master/Source/PICO-8/Graphics/atlas.png
- License of the reference pixels: NoelFB/Celeste PICO-8 graphics (review reference; MIT covers the code only)
- Animation: run

## 02 celeste-dash

- Game style: Celeste
- Category: characters
- Size: 16×16 px
- Shape: Dash pose at 16x16 with a small pack. Hair is a warm ramp (shadow purple, highlight peach); jacket is a cool blue ramp. 1px selective rim, checker dither on the mid band. Bob.
- Reference: atlas cell (5, 0), a later run pose used as the dash reference
- Source URL: https://raw.githubusercontent.com/NoelFB/Celeste/master/Source/PICO-8/Graphics/atlas.png
- License of the reference pixels: NoelFB/Celeste PICO-8 graphics (review reference; MIT covers the code only)
- Animation: static

## 03 celeste-climb

- Game style: Celeste
- Category: characters
- Size: 16×24 px
- Shape: Climbing silhouette, 16x24, same Madeline-like palette. Hair mass on the back of the head, jacket reads as one shape, legs split. Top-left light, dither on the torso.
- Reference: atlas cell (6, 0)
- Source URL: https://raw.githubusercontent.com/NoelFB/Celeste/master/Source/PICO-8/Graphics/atlas.png
- License of the reference pixels: NoelFB/Celeste PICO-8 graphics (review reference; MIT covers the code only)
- Animation: static

## 04 undertale-frisk

- Game style: Undertale
- Category: characters
- Size: 16×24 px
- Shape: Front-facing child, blue shirt, striped by the shade ramp rather than a drawn stripe. 16x24, black-blue hair, warm skin, two eyes. Bob.
- Reference: Kenney Pixel Platformer Tiles/Characters/tile_0000.png
- Source URL: https://opengameart.org/sites/default/files/kenney_pixel-platformer.zip
- License of the reference pixels: CC0 Kenney (https://kenney.nl, mirrored on OpenGameArt)
- Animation: static

## 05 undertale-frisk-side

- Game style: Undertale
- Category: characters
- Size: 16×24 px
- Shape: Same child in side view, 16x24. One eye, blue torso, cool hair shadow. Selective rim breaks on the lit side.
- Reference: Kenney Pixel Platformer Tiles/Characters/tile_0001.png
- Source URL: https://opengameart.org/sites/default/files/kenney_pixel-platformer.zip
- License of the reference pixels: CC0 Kenney (https://kenney.nl, mirrored on OpenGameArt)
- Animation: static

## 06 stardew-farmer

- Game style: Stardew Valley
- Category: characters
- Size: 16×32 px
- Shape: Front farmer, 16x32. Green shirt, warm skin, brown hair with a peach highlight. Fills the canvas the way a Stardew walk frame does. Bob.
- Reference: Kenney Pixel Platformer Tiles/Characters/tile_0002.png
- Source URL: https://opengameart.org/sites/default/files/kenney_pixel-platformer.zip
- License of the reference pixels: CC0 Kenney (https://kenney.nl, mirrored on OpenGameArt)
- Animation: static

## 07 stardew-water

- Game style: Stardew Valley
- Category: characters
- Size: 16×32 px
- Shape: Side farmer with a watering can. Can is a cool metal ramp; a two-pixel water drop sits on the spout. 16x32.
- Reference: Kenney Pixel Platformer Tiles/Characters/tile_0003.png
- Source URL: https://opengameart.org/sites/default/files/kenney_pixel-platformer.zip
- License of the reference pixels: CC0 Kenney (https://kenney.nl, mirrored on OpenGameArt)
- Animation: static

## 08 terra-miner

- Game style: Terraria
- Category: characters
- Size: 16×24 px
- Shape: Miner in profile with a pick. Earth-tone clothes, cool metal head. 16x24, top-left light.
- Reference: Kenney Pixel Platformer Tiles/Characters/tile_0004.png
- Source URL: https://opengameart.org/sites/default/files/kenney_pixel-platformer.zip
- License of the reference pixels: CC0 Kenney (https://kenney.nl, mirrored on OpenGameArt)
- Animation: static

## 09 deadcells-idle

- Game style: Dead Cells
- Category: characters
- Size: 24×32 px
- Shape: Horned prisoner, 24x32. Teal cloth against a warm bone-coloured head, no face detail beyond two eyes. The horns are part of the hair mass so they stay connected.
- Reference: Kenney Pixel Platformer Tiles/Characters/tile_0005.png
- Source URL: https://opengameart.org/sites/default/files/kenney_pixel-platformer.zip
- License of the reference pixels: CC0 Kenney (https://kenney.nl, mirrored on OpenGameArt)
- Animation: static

## 10 deadcells-roll

- Game style: Dead Cells
- Category: characters
- Size: 24×24 px
- Shape: Rolled-up silhouette, 24x24, a compact teal body rather than a full figure. Reads as a tucked roll, not a standing pose. Bob stands in for the spin (arbitrary angles are still refused).
- Reference: Kenney Pixel Platformer Tiles/Characters/tile_0006.png
- Source URL: https://opengameart.org/sites/default/files/kenney_pixel-platformer.zip
- License of the reference pixels: CC0 Kenney (https://kenney.nl, mirrored on OpenGameArt)
- Animation: static

## 11 octopath-hero

- Game style: Octopath Traveler
- Category: characters
- Size: 16×24 px
- Shape: HD-2D is out of reach; this is the sprite half. 16x24 traveller, plum coat, gold-warm highlight on the hair, cool purple shadow. Top-left lamp light.
- Reference: Kenney Pixel Platformer Tiles/Characters/tile_0007.png
- Source URL: https://opengameart.org/sites/default/files/kenney_pixel-platformer.zip
- License of the reference pixels: CC0 Kenney (https://kenney.nl, mirrored on OpenGameArt)
- Animation: static

## 12 octopath-dancer

- Game style: Octopath Traveler
- Category: characters
- Size: 16×24 px
- Shape: Dancer in a long coat that replaces the legs. Gem-coloured sash is the cloth highlight. 16x24, same lamp-from-the-top-left shading.
- Reference: Kenney Pixel Platformer Tiles/Characters/tile_0008.png
- Source URL: https://opengameart.org/sites/default/files/kenney_pixel-platformer.zip
- License of the reference pixels: CC0 Kenney (https://kenney.nl, mirrored on OpenGameArt)
- Animation: static

## 13 stardew-chicken

- Game style: Stardew Valley
- Category: characters
- Size: 16×16 px
- Shape: Hen, 16x16. Cream body with a warm highlight, orange beak and comb, two-pixel legs joined to the body. Bob.
- Reference: Kenney Pixel Platformer Tiles/Characters/tile_0011.png
- Source URL: https://opengameart.org/sites/default/files/kenney_pixel-platformer.zip
- License of the reference pixels: CC0 Kenney (https://kenney.nl, mirrored on OpenGameArt)
- Animation: static

## 14 terra-guide

- Game style: Terraria
- Category: characters
- Size: 16×24 px
- Shape: Town NPC, front view, 16x24. Tan clothes, brown hair, same earth palette as the miner so they share a town.
- Reference: Kenney Pixel Platformer Tiles/Characters/tile_0009.png
- Source URL: https://opengameart.org/sites/default/files/kenney_pixel-platformer.zip
- License of the reference pixels: CC0 Kenney (https://kenney.nl, mirrored on OpenGameArt)
- Animation: static

## 15 celeste-hair

- Game style: Celeste
- Category: characters
- Size: 16×16 px
- Shape: Head-and-shoulders study of the runner's hair. The hair ramp shifts from purple shadow to peach highlight. 16x16, bob where a hair sim would be.
- Reference: atlas cell (2, 0), hair mass of the runner
- Source URL: https://raw.githubusercontent.com/NoelFB/Celeste/master/Source/PICO-8/Graphics/atlas.png
- License of the reference pixels: NoelFB/Celeste PICO-8 graphics (review reference; MIT covers the code only)
- Animation: static

## 16 undertale-flowey

- Game style: Undertale
- Category: characters
- Size: 16×16 px
- Shape: Flower with a face, 16x16. Stem and cap use the shroom silhouette recolored to a pale petal and a green stem. Not a portrait.
- Reference: Kenney Pixel Platformer Tiles/Characters/tile_0012.png
- Source URL: https://opengameart.org/sites/default/files/kenney_pixel-platformer.zip
- License of the reference pixels: CC0 Kenney (https://kenney.nl, mirrored on OpenGameArt)
- Animation: static

## 17 terra-tiles

- Game style: Terraria
- Category: tiles
- Size: 8×8 px
- Shape: Six 8x8 stamps in one sheet: dirt, dirt variant, grass-over-dirt, grass variant, ore, ore variant. Edges match within one ramp step. Grass is a real layer boundary (top grass, bottom dirt) so the vertical seam is exempt. Ore sits in the interior.
- Reference: Terralistic base_game/resources/blocks/Template_grass_block.png, first cell of the strip
- Source URL: https://raw.githubusercontent.com/Zorz42/Terralistic/master/base_game/resources/blocks/Template_grass_block.png
- License of the reference pixels: GPL-3.0 (Zorz42/Terralistic art; reference only, not linked into the engine)
- Animation: static

## 18 terra-dirt

- Game style: Terraria
- Category: tiles
- Size: 8×8 px
- Shape: 8x8 Dirt clods, dark corners, mid fill, sealed edges. Hue runs cooler in the dark step and warmer in the light step. Opposite edges match.
- Reference: Terralistic base_game/resources/blocks/Template_dirt.png, first cell of the strip
- Source URL: https://raw.githubusercontent.com/Zorz42/Terralistic/master/base_game/resources/blocks/Template_dirt.png
- License of the reference pixels: GPL-3.0 (Zorz42/Terralistic art; reference only, not linked into the engine)
- Animation: static

## 19 terra-grass

- Game style: Terraria
- Category: tiles
- Size: 8×8 px
- Shape: 8x8 Grass cap on a dirt host. Blades on the top row, dirt on the bottom. Hue runs cooler in the dark step and warmer in the light step. Opposite edges match.
- Reference: Terralistic base_game/resources/blocks/Template_grass_block.png, first cell of the strip
- Source URL: https://raw.githubusercontent.com/Zorz42/Terralistic/master/base_game/resources/blocks/Template_grass_block.png
- License of the reference pixels: GPL-3.0 (Zorz42/Terralistic art; reference only, not linked into the engine)
- Animation: static

## 20 terra-stone

- Game style: Terraria
- Category: tiles
- Size: 8×8 px
- Shape: 8x8 Stone with a darker mortar crack every few pixels. Hue runs cooler in the dark step and warmer in the light step. Opposite edges match.
- Reference: Terralistic base_game/resources/blocks/Template_stone_block.png, first cell of the strip
- Source URL: https://raw.githubusercontent.com/Zorz42/Terralistic/master/base_game/resources/blocks/Template_stone_block.png
- License of the reference pixels: GPL-3.0 (Zorz42/Terralistic art; reference only, not linked into the engine)
- Animation: static

## 21 terra-ore

- Game style: Terraria
- Category: tiles
- Size: 8×8 px
- Shape: 8x8 Stone host, copper-coloured vein kept off the edge. Hue runs cooler in the dark step and warmer in the light step. Opposite edges match.
- Reference: Terralistic base_game/resources/blocks/Template_copper_ore.png, first cell of the strip
- Source URL: https://raw.githubusercontent.com/Zorz42/Terralistic/master/base_game/resources/blocks/Template_copper_ore.png
- License of the reference pixels: GPL-3.0 (Zorz42/Terralistic art; reference only, not linked into the engine)
- Animation: static

## 22 terra-sand

- Game style: Terraria
- Category: tiles
- Size: 8×8 px
- Shape: 8x8 Warm sand, lighter specks, no grass. Hue runs cooler in the dark step and warmer in the light step. Opposite edges match.
- Reference: Terralistic base_game/resources/blocks/sand.png, first cell of the strip
- Source URL: https://raw.githubusercontent.com/Zorz42/Terralistic/master/base_game/resources/blocks/sand.png
- License of the reference pixels: GPL-3.0 (Zorz42/Terralistic art; reference only, not linked into the engine)
- Animation: static

## 23 terra-snow

- Game style: Terraria
- Category: tiles
- Size: 8×8 px
- Shape: 8x8 Snow cap over stone, same layer rule as grass. Hue runs cooler in the dark step and warmer in the light step. Opposite edges match.
- Reference: Terralistic base_game/resources/blocks/snowy_grass_block.png, first cell of the strip
- Source URL: https://raw.githubusercontent.com/Zorz42/Terralistic/master/base_game/resources/blocks/snowy_grass_block.png
- License of the reference pixels: GPL-3.0 (Zorz42/Terralistic art; reference only, not linked into the engine)
- Animation: static

## 24 terra-wood

- Game style: Terraria
- Category: tiles
- Size: 8×8 px
- Shape: 8x8 Vertical planks, darker seam every fourth column. Hue runs cooler in the dark step and warmer in the light step. Opposite edges match.
- Reference: Terralistic base_game/resources/blocks/Template_wood_planks.png, first cell of the strip
- Source URL: https://raw.githubusercontent.com/Zorz42/Terralistic/master/base_game/resources/blocks/Template_wood_planks.png
- License of the reference pixels: GPL-3.0 (Zorz42/Terralistic art; reference only, not linked into the engine)
- Animation: static

## 25 stardew-path

- Game style: Stardew Valley
- Category: tiles
- Size: 8×8 px
- Shape: 8x8 Worn path, dark pebbles on a tan bed. Hue runs cooler in the dark step and warmer in the light step. Opposite edges match.
- Reference: Kenney Roguelike sheet cell (1, 0) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: static

## 26 stardew-hoedirt

- Game style: Stardew Valley
- Category: tiles
- Size: 8×8 px
- Shape: 8x8 Hoe rows: a dark line every third row. Hue runs cooler in the dark step and warmer in the light step. Opposite edges match.
- Reference: Kenney Roguelike sheet cell (2, 0) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: static

## 27 celeste-snow

- Game style: Celeste
- Category: tiles
- Size: 8×8 px
- Shape: 8x8 Snow ground in the cool Celeste palette. Hue runs cooler in the dark step and warmer in the light step. Opposite edges match.
- Reference: atlas cell (0, 2), a full ground tile
- Source URL: https://raw.githubusercontent.com/NoelFB/Celeste/master/Source/PICO-8/Graphics/atlas.png
- License of the reference pixels: NoelFB/Celeste PICO-8 graphics (review reference; MIT covers the code only)
- Animation: static

## 28 undertale-ruins

- Game style: Undertale
- Category: tiles
- Size: 8×8 px
- Shape: 8x8 Purple-grey brick courses. Hue runs cooler in the dark step and warmer in the light step. Opposite edges match.
- Reference: Kenney Roguelike sheet cell (5, 1) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: static

## 29 deadcells-brick

- Game style: Dead Cells
- Category: tiles
- Size: 8×8 px
- Shape: 8x8 Dark brick, teal mortar in the shadow step. Hue runs cooler in the dark step and warmer in the light step. Opposite edges match.
- Reference: Kenney Roguelike sheet cell (6, 1) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: static

## 30 octopath-cobble

- Game style: Octopath Traveler
- Category: tiles
- Size: 8×8 px
- Shape: 8x8 Cobbles with dark joints, warm highlights. Hue runs cooler in the dark step and warmer in the light step. Opposite edges match.
- Reference: Kenney Roguelike sheet cell (3, 0) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: static

## 31 terra-ice

- Game style: Terraria
- Category: tiles
- Size: 8×8 px
- Shape: 8x8 Pale ice with a diagonal crack. Hue runs cooler in the dark step and warmer in the light step. Opposite edges match.
- Reference: Terralistic base_game/resources/blocks/ice_block.png, first cell of the strip
- Source URL: https://raw.githubusercontent.com/Zorz42/Terralistic/master/base_game/resources/blocks/ice_block.png
- License of the reference pixels: GPL-3.0 (Zorz42/Terralistic art; reference only, not linked into the engine)
- Animation: static

## 32 lava-crust

- Game style: Terraria
- Category: tiles
- Size: 8×8 px
- Shape: 8x8 Dark crust with hot interior cells. Shimmer anim. Hue runs cooler in the dark step and warmer in the light step. Opposite edges match.
- Reference: Terralistic base_game/resources/blocks/Template_copper_ore.png, first cell of the strip
- Source URL: https://raw.githubusercontent.com/Zorz42/Terralistic/master/base_game/resources/blocks/Template_copper_ore.png
- License of the reference pixels: GPL-3.0 (Zorz42/Terralistic art; reference only, not linked into the engine)
- Animation: static

## 33 shaded-crate

- Game style: Octopath Traveler
- Category: props
- Size: 16×16 px
- Shape: Isometric crate, 16x16, three faces on three ramps so the right face stays darker than the lit top. Metal latch. This is the sprite version of a baked crate, without a normal map.
- Reference: Kenney Roguelike sheet cell (7, 6) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: static

## 34 barrel

- Game style: Stardew Valley
- Category: props
- Size: 16×16 px
- Shape: Barrel with two metal hoops.  Top-left light baked into the ramp, checker dither on the broad face.
- Reference: Kenney Roguelike sheet cell (8, 6) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: glint

## 35 signpost

- Game style: Stardew Valley
- Category: props
- Size: 16×16 px
- Shape: Sign board on a post.  Top-left light baked into the ramp, checker dither on the broad face.
- Reference: Kenney Roguelike sheet cell (10, 6) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: bob

## 36 torch

- Game style: Terraria
- Category: props
- Size: 16×16 px
- Shape: Wall torch, flame on a stick. Flame uses a hot ramp.  Top-left light baked into the ramp, checker dither on the broad face.
- Reference: Terralistic base_game/resources/blocks/torch.png, first cell of the strip
- Source URL: https://raw.githubusercontent.com/Zorz42/Terralistic/master/base_game/resources/blocks/torch.png
- License of the reference pixels: GPL-3.0 (Zorz42/Terralistic art; reference only, not linked into the engine)
- Animation: bob

## 37 lantern

- Game style: Octopath Traveler
- Category: props
- Size: 16×16 px
- Shape: Hanging lantern, warm glass, dark cap.  Top-left light baked into the ramp, checker dither on the broad face.
- Reference: Kenney Roguelike sheet cell (12, 5) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: bob

## 38 clay-pot

- Game style: Stardew Valley
- Category: props
- Size: 16×16 px
- Shape: Pot with a two-leaf sprout.  Top-left light baked into the ramp, checker dither on the broad face.
- Reference: Kenney Roguelike sheet cell (4, 8) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: glint

## 39 fence

- Game style: Stardew Valley
- Category: props
- Size: 16×16 px
- Shape: Three rails, two posts, one connected piece.  Top-left light baked into the ramp, checker dither on the broad face.
- Reference: Kenney Roguelike sheet cell (2, 4) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: glint

## 40 wooden-door

- Game style: Terraria
- Category: props
- Size: 16×24 px
- Shape: Door with a metal handle on the right.  Top-left light baked into the ramp, checker dither on the broad face.
- Reference: Kenney Roguelike sheet cell (0, 6) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: glint

## 41 bed

- Game style: Stardew Valley
- Category: props
- Size: 16×16 px
- Shape: Bed, pillow in cloth, frame in wood.  Top-left light baked into the ramp, checker dither on the broad face.
- Reference: Kenney Roguelike sheet cell (6, 8) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: glint

## 42 table

- Game style: Stardew Valley
- Category: props
- Size: 16×16 px
- Shape: Table top and two legs.  Top-left light baked into the ramp, checker dither on the broad face.
- Reference: Kenney Roguelike sheet cell (4, 7) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: glint

## 43 chair

- Game style: Stardew Valley
- Category: props
- Size: 16×16 px
- Shape: Chair, back post and two legs.  Top-left light baked into the ramp, checker dither on the broad face.
- Reference: Kenney Roguelike sheet cell (5, 7) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: glint

## 44 well

- Game style: Stardew Valley
- Category: props
- Size: 16×16 px
- Shape: Stone rim, water hole, wooden roof posts.  Top-left light baked into the ramp, checker dither on the broad face.
- Reference: Kenney Roguelike sheet cell (9, 5) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: glint

## 45 item-chest

- Game style: Terraria
- Category: items
- Size: 16×16 px
- Shape: Closed chest, 16x16. Lid is a lighter wood ramp than the body, lock is cool metal and does not move when the body bobs. Wood grain is the dither, not a noise tile.
- Reference: Kenney Roguelike sheet cell (8, 7) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: static

## 46 copper-sword

- Game style: Terraria
- Category: items
- Size: 16×16 px
- Shape: Straight sword, cool blade, warm grip. 16x16 icon, top-left light, dither on the largest face.
- Reference: Kenney Roguelike sheet cell (0, 10) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: glint

## 47 pickaxe

- Game style: Terraria
- Category: items
- Size: 16×16 px
- Shape: Pick, metal head, wooden haft. 16x16 icon, top-left light, dither on the largest face.
- Reference: Terralistic base_game/resources/items/pickaxe.png, first cell of the strip
- Source URL: https://raw.githubusercontent.com/Zorz42/Terralistic/master/base_game/resources/items/pickaxe.png
- License of the reference pixels: GPL-3.0 (Zorz42/Terralistic art; reference only, not linked into the engine)
- Animation: glint

## 48 hatchet

- Game style: Terraria
- Category: items
- Size: 16×16 px
- Shape: Hatchet, head on the left of the haft. 16x16 icon, top-left light, dither on the largest face.
- Reference: Terralistic base_game/resources/items/hatchet.png, first cell of the strip
- Source URL: https://raw.githubusercontent.com/Zorz42/Terralistic/master/base_game/resources/items/hatchet.png
- License of the reference pixels: GPL-3.0 (Zorz42/Terralistic art; reference only, not linked into the engine)
- Animation: glint

## 49 watering-can

- Game style: Stardew Valley
- Category: items
- Size: 16×16 px
- Shape: Can with a two-pixel spout drop. 16x16 icon, top-left light, dither on the largest face.
- Reference: Kenney Roguelike sheet cell (4, 11) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: glint

## 50 ruby

- Game style: Terraria
- Category: items
- Size: 16×16 px
- Shape: Gem, red ramp from a cool dark to a warm highlight. 16x16 icon, top-left light, dither on the largest face.
- Reference: Kenney Roguelike sheet cell (6, 10) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: glint

## 51 gold-coin

- Game style: Stardew Valley
- Category: items
- Size: 16×16 px
- Shape: Coin disc, warm metal ramp. 16x16 icon, top-left light, dither on the largest face.
- Reference: Kenney Roguelike sheet cell (8, 10) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: glint

## 52 iron-key

- Game style: Terraria
- Category: items
- Size: 16×16 px
- Shape: Key, bow and two teeth, one piece. 16x16 icon, top-left light, dither on the largest face.
- Reference: Kenney Roguelike sheet cell (2, 10) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: glint

## 53 health-potion

- Game style: Stardew Valley
- Category: items
- Size: 16×16 px
- Shape: Bottle, glass ramp and a green liquid. 16x16 icon, top-left light, dither on the largest face.
- Reference: Kenney Roguelike sheet cell (3, 11) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: glint

## 54 berry

- Game style: Stardew Valley
- Category: items
- Size: 16×16 px
- Shape: Berry with a leaf joined to the fruit. 16x16 icon, top-left light, dither on the largest face.
- Reference: Kenney Roguelike sheet cell (5, 12) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: glint

## 55 fish

- Game style: Stardew Valley
- Category: items
- Size: 16×16 px
- Shape: Side fish, tail joined to the body. 16x16 icon, top-left light, dither on the largest face.
- Reference: Kenney Roguelike sheet cell (7, 12) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: glint

## 56 spellbook

- Game style: Octopath Traveler
- Category: items
- Size: 16×16 px
- Shape: Closed book, cover and a page stripe. 16x16 icon, top-left light, dither on the largest face.
- Reference: Kenney Roguelike sheet cell (1, 11) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: glint

## 57 shortbow

- Game style: Terraria
- Category: items
- Size: 16×16 px
- Shape: Bow stave and a string. 16x16 icon, top-left light, dither on the largest face.
- Reference: Kenney Roguelike sheet cell (9, 10) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: glint

## 58 wooden-shield

- Game style: Terraria
- Category: items
- Size: 16×16 px
- Shape: Shield disc with a metal rib. 16x16 icon, top-left light, dither on the largest face.
- Reference: Kenney Roguelike sheet cell (10, 10) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: glint

## 59 crop-stages

- Game style: Stardew Valley
- Category: crops
- Size: 16×32 px
- Shape: Four growth stages on 16x32: soil and a short stem, then leaves, then a wider leaf, then fruit. Each stage keeps the soil band so the bbox stays in range. Adjacent stages change only the plant, under the 35% frame-diff cap. Leaves use a yellow-green highlight and a blue-green shadow.
- Reference: Kenney Roguelike sheet cell (2, 12) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: grow

## 60 orchard-tree

- Game style: Stardew Valley
- Category: crops
- Size: 32×40 px
- Shape: Tree, 32x40. Three canopy lobes instead of one circle, a three-pixel trunk, fruit pixels tucked into the leaves. Wind would be a row shift; bob is what stays inside the outline and the diff cap.
- Reference: Kenney Roguelike sheet cell (0, 8) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: bob

## 61 wheat

- Game style: Stardew Valley
- Category: crops
- Size: 16×24 px
- Shape: Wheat ear in the flower ramp. 16x24, soil band at the bottom, top-left light on the leaves, dither where the leaf is wide.
- Reference: Kenney Roguelike sheet cell (1, 12) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: bob

## 62 parsnip

- Game style: Stardew Valley
- Category: crops
- Size: 16×24 px
- Shape: Leaf pair and a root at the soil line. 16x24, soil band at the bottom, top-left light on the leaves, dither where the leaf is wide.
- Reference: Kenney Roguelike sheet cell (3, 12) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: bob

## 63 cauliflower

- Game style: Stardew Valley
- Category: crops
- Size: 16×24 px
- Shape: Broad pale head on a short stem. 16x24, soil band at the bottom, top-left light on the leaves, dither where the leaf is wide.
- Reference: Kenney Roguelike sheet cell (4, 12) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: bob

## 64 sunflower

- Game style: Stardew Valley
- Category: crops
- Size: 16×24 px
- Shape: Yellow head, green leaf. 16x24, soil band at the bottom, top-left light on the leaves, dither where the leaf is wide.
- Reference: Kenney Roguelike sheet cell (6, 12) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: bob

## 65 sapling

- Game style: Stardew Valley
- Category: crops
- Size: 16×24 px
- Shape: Thin stem and a small round crown. 16x24, soil band at the bottom, top-left light on the leaves, dither where the leaf is wide.
- Reference: Kenney Roguelike sheet cell (1, 8) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: bob

## 66 mushroom

- Game style: Terraria
- Category: crops
- Size: 16×24 px
- Shape: Red cap, pale stem. 16x24, soil band at the bottom, top-left light on the leaves, dither where the leaf is wide.
- Reference: Kenney Roguelike sheet cell (8, 12) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: bob

## 67 cactus

- Game style: Terraria
- Category: crops
- Size: 16×24 px
- Shape: Column with two arms. 16x24, soil band at the bottom, top-left light on the leaves, dither where the leaf is wide.
- Reference: Terralistic base_game/resources/blocks/cactus.png, first cell of the strip
- Source URL: https://raw.githubusercontent.com/Zorz42/Terralistic/master/base_game/resources/blocks/cactus.png
- License of the reference pixels: GPL-3.0 (Zorz42/Terralistic art; reference only, not linked into the engine)
- Animation: bob

## 68 pumpkin

- Game style: Stardew Valley
- Category: crops
- Size: 16×24 px
- Shape: Orange fruit sitting on the soil. 16x24, soil band at the bottom, top-left light on the leaves, dither where the leaf is wide.
- Reference: Kenney Roguelike sheet cell (9, 12) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: bob

## 69 tulip

- Game style: Stardew Valley
- Category: crops
- Size: 16×24 px
- Shape: Single bloom on a thin stem. 16x24, soil band at the bottom, top-left light on the leaves, dither where the leaf is wide.
- Reference: Kenney Roguelike sheet cell (10, 12) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: bob

## 70 berry-bush

- Game style: Stardew Valley
- Category: crops
- Size: 16×24 px
- Shape: Low bush with two berries. 16x24, soil band at the bottom, top-left light on the leaves, dither where the leaf is wide.
- Reference: Kenney Roguelike sheet cell (2, 8) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: bob

## 71 meadow-slime

- Game style: Stardew Valley
- Category: enemies
- Size: 16×16 px
- Shape: Flat slime, 16x16, desaturated green-grey rather than a saturated green. Highlight on the upper left, dark foot. The hop is a 1px bob.
- Reference: Kenney Pixel Platformer Tiles/Characters/tile_0014.png
- Source URL: https://opengameart.org/sites/default/files/kenney_pixel-platformer.zip
- License of the reference pixels: CC0 Kenney (https://kenney.nl, mirrored on OpenGameArt)
- Animation: static

## 72 cave-bat

- Game style: Terraria
- Category: enemies
- Size: 16×16 px
- Shape: Wings wider than the body, two ears. 16x16, top-left light, dark rim on the lower right.
- Reference: Kenney Pixel Platformer Tiles/Characters/tile_0015.png
- Source URL: https://opengameart.org/sites/default/files/kenney_pixel-platformer.zip
- License of the reference pixels: CC0 Kenney (https://kenney.nl, mirrored on OpenGameArt)
- Animation: bob

## 73 skeleton

- Game style: Terraria
- Category: enemies
- Size: 16×16 px
- Shape: Skull, rib ticks, two legs. 16x16, top-left light, dark rim on the lower right.
- Reference: Kenney Roguelike sheet cell (14, 14) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: bob

## 74 floating-eye

- Game style: Terraria
- Category: enemies
- Size: 16×16 px
- Shape: Round eye, iris, pupil. 16x16, top-left light, dark rim on the lower right.
- Reference: Kenney Roguelike sheet cell (15, 14) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: bob

## 75 zombie

- Game style: Terraria
- Category: enemies
- Size: 16×16 px
- Shape: Front humanoid in a sick green. 16x16, top-left light, dark rim on the lower right.
- Reference: Kenney Pixel Platformer Tiles/Characters/tile_0018.png
- Source URL: https://opengameart.org/sites/default/files/kenney_pixel-platformer.zip
- License of the reference pixels: CC0 Kenney (https://kenney.nl, mirrored on OpenGameArt)
- Animation: bob

## 76 bee

- Game style: Stardew Valley
- Category: enemies
- Size: 16×16 px
- Shape: Striped body, two wings, a sting. 16x16, top-left light, dark rim on the lower right.
- Reference: Kenney Pixel Platformer Tiles/Characters/tile_0016.png
- Source URL: https://opengameart.org/sites/default/files/kenney_pixel-platformer.zip
- License of the reference pixels: CC0 Kenney (https://kenney.nl, mirrored on OpenGameArt)
- Animation: bob

## 77 hermit-crab

- Game style: Stardew Valley
- Category: enemies
- Size: 16×16 px
- Shape: Shell and two claws. 16x16, top-left light, dark rim on the lower right.
- Reference: Kenney Roguelike sheet cell (12, 14) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: bob

## 78 ghost

- Game style: Terraria
- Category: enemies
- Size: 16×16 px
- Shape: Sheet with a wavy hem. 16x16, top-left light, dark rim on the lower right.
- Reference: Kenney Pixel Platformer Tiles/Characters/tile_0017.png
- Source URL: https://opengameart.org/sites/default/files/kenney_pixel-platformer.zip
- License of the reference pixels: CC0 Kenney (https://kenney.nl, mirrored on OpenGameArt)
- Animation: bob

## 79 earth-worm

- Game style: Stardew Valley
- Category: enemies
- Size: 16×16 px
- Shape: Segmented worm, one piece. 16x16, top-left light, dark rim on the lower right.
- Reference: Kenney Roguelike sheet cell (11, 14) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: bob

## 80 spike-ball

- Game style: Dead Cells
- Category: enemies
- Size: 16×16 px
- Shape: Core with eight spikes joined back to the body. 16x16, top-left light, dark rim on the lower right.
- Reference: Kenney Roguelike sheet cell (13, 15) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: bob

## 81 shroom-enemy

- Game style: Terraria
- Category: enemies
- Size: 16×16 px
- Shape: Cap and stem with eyes. 16x16, top-left light, dark rim on the lower right.
- Reference: Kenney Roguelike sheet cell (8, 12) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: bob

## 82 shade-wisp

- Game style: Dead Cells
- Category: enemies
- Size: 16×16 px
- Shape: Tall flame with a bright core. 16x16, top-left light, dark rim on the lower right.
- Reference: Kenney Roguelike sheet cell (16, 14) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: bob

## 83 ui-dialog

- Game style: Undertale
- Category: ui
- Size: 32×20 px
- Shape: Dialogue panel, 32x20. Paper ramp with a cooler shadow, a short ink word joined to a baseline, and a 3x3 cursor that swaps to the paper highlight. Not a 9-slice.
- Reference: Kenney Roguelike sheet cell (0, 18) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: static

## 84 heart-meter

- Game style: Undertale
- Category: ui
- Size: 32×16 px
- Shape: Three hearts over a shared baseline, two filled and one empty. Flat UI, two or three ramps, no faux bevel beyond the top-left rim.
- Reference: Terralistic base_game/resources/misc/hearts.png, first cell of the strip
- Source URL: https://raw.githubusercontent.com/Zorz42/Terralistic/master/base_game/resources/misc/hearts.png
- License of the reference pixels: GPL-3.0 (Zorz42/Terralistic art; reference only, not linked into the engine)
- Animation: glint

## 85 menu-button

- Game style: Terraria
- Category: ui
- Size: 24×12 px
- Shape: Button face with a mid ink bar. Flat UI, two or three ramps, no faux bevel beyond the top-left rim.
- Reference: Terralistic resources/play_button.png, first cell of the strip
- Source URL: https://raw.githubusercontent.com/Zorz42/Terralistic/master/resources/play_button.png
- License of the reference pixels: GPL-3.0 (Zorz42/Terralistic art; reference only, not linked into the engine)
- Animation: static

## 86 item-slot

- Game style: Stardew Valley
- Category: ui
- Size: 16×16 px
- Shape: Slot, outer face and a darker inset. Flat UI, two or three ramps, no faux bevel beyond the top-left rim.
- Reference: Kenney Roguelike sheet cell (2, 18) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: static

## 87 cursor-hand

- Game style: Stardew Valley
- Category: ui
- Size: 16×16 px
- Shape: Pointing hand, one connected shape. Flat UI, two or three ramps, no faux bevel beyond the top-left rim.
- Reference: Kenney Roguelike sheet cell (4, 19) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: glint

## 88 coin-counter

- Game style: Stardew Valley
- Category: ui
- Size: 24×16 px
- Shape: Coin plus a two-digit block. Flat UI, two or three ramps, no faux bevel beyond the top-left rim.
- Reference: Kenney Roguelike sheet cell (8, 10) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: static

## 89 nameplate

- Game style: Octopath Traveler
- Category: ui
- Size: 32×12 px
- Shape: Name plate, one ink bar for the name. Flat UI, two or three ramps, no faux bevel beyond the top-left rim.
- Reference: Kenney Roguelike sheet cell (1, 18) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: static

## 90 menu-arrow

- Game style: Undertale
- Category: ui
- Size: 16×16 px
- Shape: Right-pointing arrow. Flat UI, two or three ramps, no faux bevel beyond the top-left rim.
- Reference: Kenney Roguelike sheet cell (5, 19) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: static

## 91 stamina-bar

- Game style: Dead Cells
- Category: ui
- Size: 32×12 px
- Shape: Bar frame with a two-thirds fill. Flat UI, two or three ramps, no faux bevel beyond the top-left rim.
- Reference: Kenney Roguelike sheet cell (3, 18) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: static

## 92 map-marker

- Game style: Octopath Traveler
- Category: ui
- Size: 16×16 px
- Shape: Pin: disc over a short stem. Flat UI, two or three ramps, no faux bevel beyond the top-left rim.
- Reference: Kenney Roguelike sheet cell (6, 19) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: glint

## 93 soul-bullet

- Game style: Undertale
- Category: effects
- Size: 16×16 px
- Shape: Red heart, 16x16, four-step ramp from a cool dark red to a warm highlight, one glint pixel. A three-pixel chevron steps along the bottom across four frames. The heart does not spin.
- Reference: Terralistic base_game/resources/misc/hearts.png, first cell of the strip
- Source URL: https://raw.githubusercontent.com/Zorz42/Terralistic/master/base_game/resources/misc/hearts.png
- License of the reference pixels: GPL-3.0 (Zorz42/Terralistic art; reference only, not linked into the engine)
- Animation: soul

## 94 celeste-spikes

- Game style: Celeste
- Category: effects
- Size: 8×8 px
- Shape: Single peak on a base, 8x8. rotate90 builds right, down, and left. A two-frame glint swaps the highlight. No shade-before-rotate, so every direction keeps the same value pattern.
- Reference: atlas cell (1, 1), the upward spike pair
- Source URL: https://raw.githubusercontent.com/NoelFB/Celeste/master/Source/PICO-8/Graphics/atlas.png
- License of the reference pixels: NoelFB/Celeste PICO-8 graphics (review reference; MIT covers the code only)
- Animation: spike

## 95 hit-spark

- Game style: Dead Cells
- Category: effects
- Size: 16×16 px
- Shape: Four-point spark. 16x16. Motion is a 1px step so the frame diff stays inside the shift allowance.
- Reference: Kenney Roguelike sheet cell (10, 15) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: slide

## 96 puff-explosion

- Game style: Terraria
- Category: effects
- Size: 16×16 px
- Shape: Hot disc with a white core. 16x16. Motion is a 1px step so the frame diff stays inside the shift allowance.
- Reference: Kenney Roguelike sheet cell (11, 15) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: glint

## 97 bone-bullet

- Game style: Undertale
- Category: effects
- Size: 16×16 px
- Shape: Horizontal bone, knobs joined to the shaft. 16x16. Motion is a 1px step so the frame diff stays inside the shift allowance.
- Reference: Kenney Roguelike sheet cell (12, 15) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: slide

## 98 ring-bullet

- Game style: Undertale
- Category: effects
- Size: 16×16 px
- Shape: White ring, center punched out, still one component. 16x16. Motion is a 1px step so the frame diff stays inside the shift allowance.
- Reference: Kenney Roguelike sheet cell (13, 15) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: slide

## 99 slash-arc

- Game style: Dead Cells
- Category: effects
- Size: 16×16 px
- Shape: Diagonal slash, two pixels thick. 16x16. Motion is a 1px step so the frame diff stays inside the shift allowance.
- Reference: Kenney Roguelike sheet cell (14, 15) at 16px with a 1px gap
- Source URL: https://opengameart.org/sites/default/files/Roguelike%20pack.zip
- License of the reference pixels: CC0 Kenney roguelike/RPG pack (OpenGameArt)
- Animation: glint

## 100 dash-crystal

- Game style: Celeste
- Category: effects
- Size: 16×16 px
- Shape: Diamond crystal, cool gem ramp. 16x16. Motion is a 1px step so the frame diff stays inside the shift allowance.
- Reference: atlas cell (12, 1)
- Source URL: https://raw.githubusercontent.com/NoelFB/Celeste/master/Source/PICO-8/Graphics/atlas.png
- License of the reference pixels: NoelFB/Celeste PICO-8 graphics (review reference; MIT covers the code only)
- Animation: glint
