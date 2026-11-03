# Animation

## Keys first

Block key poses as separate frames/ASCII blocks. In-betweens only after keys read in a filmstrip.

## Timing

- Walk/run: asymmetric spacing; avoid identical ms on every frame.
- Idle: tiny volume change or secondary motion; long holds OK.
- Attack: anticipation → hit → settle.

## Loops

First/last frame continuity matters more than middle polish. GIF loop-check before sheet export.

## Tags

Name tags for engines (`idle`, `walk`, `hurt`). Use `ase tag add` or recipe `anims[].name`. Keep ranges inclusive and non-overlapping unless nested on purpose (`--play-subtags` when exporting).

## Don'ts

- Moving the light between frames.
- Sliding a static cel without squash/recontact when foot plants matter.
