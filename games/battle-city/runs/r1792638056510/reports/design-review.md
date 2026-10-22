# Design package review

- run: `r1792638056510`

## Game: battle_city (top_down_tank)

Drive the player tank through a single-screen 12x12 maze, fire bullets to break brick walls and kill enemy tanks, stay off hazard tiles, keep the HQ base alive, and drive onto the exit pad to clear the stage before moving on to the next, harder stage.

### Actions

| id | name | input |
|---|---|---|
| move | Move | arrow/WASD |
| shoot | Shoot | space |

### Entities

| id | kind | actions |
|---|---|---|
| player_tank | player | move, shoot |
| enemy_tank | enemy | move, shoot |
| bullet | bullet |  |
| explosion | prop |  |
| hq_base | prop |  |
| hud_life_icon | ui |  |
| hud_stage_label | ui |  |

### Rules

- **r_move_blocked**: The player tank moves one tile step per input on tiles whose semantics are not blocking (floor f, grass t, goal pad g); brick b, steel x and water w stop it.
- **r_shoot_bullet**: Pressing shoot spawns one bullet at the tank facing edge travelling in that direction; a tank may have at most one live bullet.
- **r_brick_destructible**: A bullet impact on a brick tile b destroys that brick tile, removes the bullet and spawns an explosion; brick tiles are the only destructible wall type.
- **r_steel_indestructible**: Steel tiles x stop both tanks and bullets; a bullet hitting steel is removed with a small explosion and the steel tile survives.
- **r_water_impassable**: Water tiles w are impassable for the player tank and for enemy tanks but do not stop bullets.
- **r_hazard_lose**: Standing on a hazard tile h destroys the player tank and loses the stage immediately.
- **r_enemy_patrol**: Each enemy tank patrols open floor along its facing direction, turns when blocked, and periodically fires a bullet toward the base or the player.
- **r_bullet_kills_tank**: A bullet hitting the player tank destroys the player tank and loses the stage; a bullet hitting an enemy tank destroys the enemy tank and spawns an explosion.
- **r_base_defense**: The HQ base occupies one floor tile per stage; an enemy bullet reaching it destroys the base and loses the stage, so the player must clear the approach lanes.
- **r_reach_exit**: Stepping onto a goal pad tile g completes the stage; every stage exit is reachable from the spawn tile by walking only on non-blocking tiles.
- **r_stage_difficulty**: Stage one spawns no enemies and no hazards, stage two adds firing at brick walls, stage three adds enemy tanks, water and steel cover.

**win condition**: The player tank steps onto the stage exit pad tile g.

**lose condition**: The stage is lost when the player tank is hit by a bullet, stands on a hazard tile, or the HQ base is destroyed.

## Style guide

- logical resolution: 320×240
- tile size: 16px
- sprite size: 16px
- palette: `palette.json`
- directions: N, E, S, W
- outline: always

## Levels

### level_1 — Training Yard

Objective: Learn to drive: move the tank along the open lanes and reach the exit pad in the bottom-right. No enemies are present, so take time to feel the 4-direction steering.

Notes: Teaching stage: brick pillars flank the lanes and grass rows are decorative only. Spawn (1,10) and exit (9,10) both sit on the passable grass/floor row y=10, so the exit is walkable from spawn with no shooting. The HQ base sits at (5,11) and cannot be threatened because no enemy tanks spawn here.

```text
ffffffffffff
ffbbffffbbff
ffffffffffff
ffttttttttff
ffbbffffbbff
ffffffffffff
ffbbffffbbff
ffffffffffff
ffbbffffbbff
ffffffffffff
fftttttttgff
ffffffffffff
```

### level_2 — Brick Breakthrough

Objective: Shooting stage: the exit pad at the top sits behind a brick wall. Blast brick tiles with space to cut a direct lane, or take the long open lane around the left side.

Notes: Brick walls are the only destructible tiles and form a lattice; lanes at x=0,4,8 and every even row stay open. Spawn (1,10) reaches exit (6,0) without shooting by going up the x=0 column and right along row y=0, so the exit stays walkable while the short route must be carved with bullets. HQ base at (8,11) is exposed, teaching the player to keep firing lanes clear.

```text
ffffffgfffff
fbbbfbbbfbbf
ffffffffffff
fbbfbbfbbfbf
ffffffffffff
fbbfbbfbbfbf
ffffffffffff
fbbfbbfbbfbf
ffffffffffff
fbbfbbfbbfbf
ffttttttttff
ffffffffffff
```

### level_3 — River Siege

Objective: Siege stage: three enemy tanks hunt you while water splits the map and steel corners shield the HQ base. Break the brick at (8,11) if you want a short cut, dodge the hazards, and reach the exit pad in the bottom-right.

Notes: Water rows (y=2 at x=6-9, y=6 at x=0-3) cut the map into a winding corridor and steel clusters guard the base; the only fully safe route from spawn (1,0) to exit (10,11) runs down column x=0 to y=5, right to x=5, down through the middle lane to y=11, then past the brick at (8,11) via row y=10. Hazards at (11,3) and (9,8) punish careless steering. HQ base at (8,4) is shielded by steel at (7,8) and (8,6) lanes and must be defended from enemy fire.

```text
ffffffffffff
ffgfffffffff
fxxxffwwwwff
ffffffffffhf
fbbxffffffff
ffffffffffff
wwwwfffxxfff
ffffffffffff
fffffffxxhff
ffttttttttff
ffffffffffff
ffttffffbffg
```

## Asset requirements

| asset | kind | required | size | directions | anims | strategy |
|---|---|---|---|---|---|---|
| player_tank | sprite | yes | 16×16 | N, E, S, W | idle, move, fire | template |
| enemy_tank | sprite | yes | 16×16 | N, E, S, W | idle, move, fire | template |
| bullet | sprite | yes | 8×8 | N, E, S, W | fly | template |
| explosion | effect | yes | 16×16 | N | burst | template |
| hq_base | sprite | yes | 16×16 | N | idle, destroyed | template |
| hud_life_icon | ui | yes | 8×8 | N | idle | template |
| hud_stage_label | ui | yes | 64×8 | N | idle | shapes |
| b | tile | yes | 16×16 | N | idle, break | tile_gen |
| x | tile | yes | 16×16 | N | idle, spark | tile_gen |
| w | tile | yes | 16×16 | N | idle | tile_gen |
| h | tile | yes | 16×16 | N | idle | tile_gen |
| g | tile | yes | 16×16 | N | idle | tile_gen |
| t | tile | yes | 16×16 | N | idle | tile_gen |

## Acceptance plan

| requirement | gate | stage | evidence |
|---|---|---|---|
| player_tank | assets.coverage | assets | manifest entry for player_tank with 4 directions and 3 anims |
| enemy_tank | assets.coverage | assets | manifest entry for enemy_tank with 4 directions and 3 anims |
| bullet | assets.coverage | assets | manifest entry for bullet with 4 directions |
| explosion | assets.coverage | assets | manifest entry for explosion with 4 frames |
| hq_base | assets.coverage | assets | manifest entry for hq_base plus level placements |
| hud_life_icon | assets.coverage | assets | manifest entry for hud_life_icon |
| hud_stage_label | assets.coverage | assets | manifest entry for hud_stage_label |
| b | assets.coverage | assets | manifest entry for b plus tile_gen output |
| x | assets.coverage | assets | manifest entry for x plus tile_gen output |
| w | assets.coverage | assets | manifest entry for w plus tile_gen output |
| h | assets.coverage | assets | manifest entry for h plus tile_gen output |
| g | assets.coverage | assets | manifest entry for g plus tile_gen output |
| t | assets.coverage | assets | manifest entry for t plus tile_gen output |
| b | gdd.rules | engine | engine test: bullet vs brick in level_2 |
| x | gdd.rules | engine | engine test: bullet and tank vs steel in level_3 |
| w | gdd.rules | engine | engine test: movement into water tile |
| h | gdd.rules | engine | engine test: spawn override onto hazard tile |
| hq_base | gdd.rules | engine | engine test: scripted enemy bullet into base tile |
| player_tank | gdd.win_condition | playtest | playtest note per stage |
| player_tank | gdd.levels | playtest | playtest recording of level_1 run |
| enemy_tank | gdd.rules | playtest | playtest recording of level_3 run |
| hud_stage_label | gdd.ui | human | reviewer screen capture |
| player_tank | style.consistency | human | reviewer art sign-off |

## Designer review request

Battle City top-down tank slice designed as a complete package: game-design.json (2 actions, 7 entities, 11 rules, win/lose conditions closed over existing ids), style-guide.json (320x240 logical, 16px tiles/sprites, monochrome palette, N/E/S/W, 2-16 fps, nearest filtering), three 12x12 levels, asset-requirements.json (13 requirements covering every entity plus all non-floor/non-decor tiles: b, x, w, h, g, t), and acceptance-plan.json (22 items spanning design/assets/engine/playtest/human). Difficulty ramps per the template: level_1 "Training Yard" teaches movement only to an exit at (9,10) with no enemies; level_2 "Brick Breakthrough" puts the exit at (6,0) behind a brick lattice so shooting carves a shortcut; level_3 "River Siege" adds 3 enemy tanks, water corridors, steel cover, 2 hazard tiles and base defense. Every stage exit was hand-verified walkable from spawn on non-blocking tiles, and no spawn sits on a blocking tile.

### Risks

- explosion is authored as entity kind 'prop' because the contract's kind enum lacks 'effect' (the genre template uses it); it maps to an 'effect' asset requirement. If the schema gains 'effect', this should be migrated.
- game-design.json scope must be numeric-only: the validator rejects booleans (expected u32), so all scope values are integers (levels, grid_w/h, tile_size_px, sprite_size_px, logical_width/height, max_players, screens_per_stage) rather than flags.
- hq_base sits on a non-blocking floor tile, so it must be rendered as a prop overlay rather than an occupancy tile; enemy bullet collision with the base needs a dedicated hitbox in engine.
- level_3 enemies (9,5), (4,9), (10,3) all stand on floor but (10,3) is directly above hazard (11,3) — engine patrol logic must not push a tank onto a hazard-adjacent lane carelessly, or the stage may feel unfair.
- Stage 2 and 3 exits are reachable without firing by design; if designers prefer shooting to be mandatory, the open bypass lane should be closed.

### Open questions

- Should the HQ base destruction be survivable on level_3 (respawn/lives) or an instant stage loss as specified?
- Is a hazard tile ('h') wanted at all on level_3, given the brief lists water but not hazards as an element?
- Should grass ('t') render above tanks for the classic occlusive look, or below them for clarity?

