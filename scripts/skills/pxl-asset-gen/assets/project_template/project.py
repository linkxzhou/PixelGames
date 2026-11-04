"""Starter project for build_library.py — copy to <out_dir>/tools/project.py.

Author every sprite as glyphs on a Grid; the pxl-asset-gen binary rasterises.
Split big packs into sibling modules (tools/nature.py, tools/people.py, ...)
that each expose populate(lib) and call them from populate() below.
"""
from art_kit import Grid

T16 = [0, 0, 16, 16]  # ascii canvases are 32/64/128/256; 16px art sits top-left

CONFIG = dict(
    id='starter',
    title='Starter 像素素材库',
    eyebrow='PIXELGAMES / ASSET LIBRARY',
    subtitle='示例素材包：地块、物品、道具与角色，全部由 ASCII 网格经 pxl-asset-gen 构建。',
    note='16×16 素材位于 32×32 画布左上角；接入时以 manifest.json 的 source_rect 为准。',
    license_note='原创素材',
    seed=1104,
    scope='模板示例，非完整资源',
    deferred=['角色完整动作动画', '音频'],
    brief={'view': 'top-down three-quarter', 'light': 'upper-left', 'tile_size': 16, 'transparent_index': 0},
    palette=dict(
        id='starter-soft',
        glyphs='.#btocwfglaBusreq',
        colors=['#00000000', '#30313d', '#59403e', '#986349', '#c58c54', '#edc78c', '#fff0d0',
                '#294f46', '#4d7951', '#83aa62', '#a7ca80', '#31516d', '#4e88a2', '#85bdc6',
                '#93464e', '#cd6260', '#e4bc57'],
        ramps={'wood': [2, 3, 4, 5], 'grass': [7, 8, 9, 10], 'water': [11, 12, 13], 'warm': [14, 15, 16]},
        special={'transparent': 0, 'outline': 1, 'highlight': 6}),
    max_colors=16,
    preview_scale=3,
    # board_cell={'buildings': 64},  # optional per-category preview cell
    # theme={'bg': '#1c2926', 'accent': '#e4bc57'},  # optional gallery colours
)


def terrain(lib):
    grass = Grid().rect(0, 0, 16, 16, 'g')
    for x, y in [(3, 4), (9, 2), (12, 9), (5, 11), (7, 7), (11, 13)]:
        grass.mark(x, y, 'l').mark(x, y + 1, 'f')
    lib.add('tile_grass', '草地', 'terrain', grass, region=T16, tile=True)
    for frame in range(3):
        water = Grid().rect(0, 0, 16, 16, 'u')
        for x, y in [(2, 3), (8, 6), (4, 10), (10, 12)]:
            water.rect(x - frame, y, 3, 1, 's').mark(x + 1 - frame, y + 1, 'B')
        water_id = 'tile_water' if frame == 0 else f'tile_water_{frame}'
        lib.add(water_id, f'水波·{frame + 1}', 'terrain', water, region=T16,
                sequence='water', frame=frame, duration_ms=240)
    dirt = Grid().rect(0, 0, 16, 16, 't')
    for x, y in [(4, 3), (10, 5), (6, 9), (12, 11), (2, 13)]:
        dirt.mark(x, y, 'b').mark(x + 1, y, 'o')
    lib.add('tile_dirt', '泥土', 'terrain', dirt, region=T16, tile=True)


def items(lib):
    gem = Grid().stamp(4, 4, '''
..####..
.#ssus#.
#swssuu#
#ssuuuB#
.#suuB#.
..#uB#..
...##...''')
    lib.add('item_gem', '宝石', 'items', gem, region=T16, tags=['loot'])
    berry = Grid().stamp(4, 3, '''
....gg..
...g....
.##..##.
#ewe#ee#
#eer#er#
.#r##r#.
..#..#..''')
    lib.add('item_berry', '浆果', 'items', berry, region=T16, tags=['food'])


def props(lib):
    chest = Grid().stamp(2, 3, '''
.##########.
#ocooooooco#
#tttttttttt#
############
#ottt#qtttt#
#tttt##tttb#
#bbbbbbbbbb#
############''')
    lib.add('prop_chest', '木箱', 'props', chest, region=T16, tags=['container'])


def characters(lib):
    slime = Grid().stamp(2, 5, '''
....####....
..##laal##..
.#lawwalgg#.
#lag#gg#ggf#
#gggggggggf#
#fggggggfff#
.##########.''')
    lib.add('char_slime', '史莱姆', 'characters', slime, region=T16, tags=['monster'])


def populate(lib):
    for part in (terrain, items, props, characters):
        part(lib)


def scene(lib):
    """Optional hero image: compose existing assets only."""
    g = Grid(64)
    for y in range(0, 64, 16):
        for x in range(0, 64, 16):
            lib.place(g, 'tile_water' if x == 48 else 'tile_dirt' if y == 32 else 'tile_grass', x, y)
    for key, x, y in [('prop_chest', 8, 8), ('char_slime', 28, 16), ('item_gem', 20, 36), ('item_berry', 36, 40)]:
        lib.place(g, key, x, y)
    return g
