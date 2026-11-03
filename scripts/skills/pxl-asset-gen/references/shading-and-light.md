# Shading and light

## Hue shift

- **Shadow:** cooler (toward blue/purple), slightly desaturated.
- **Light:** warmer (toward yellow/orange), slightly more saturated.
- Pure black multiply and pure white screen flatten materials — prefer ramp steps.

## Form

- Soft materials: broader value transitions.
- Hard materials: abrupt steps + small specular.
- Avoid pillow shading (dark contour all around, bright center) unless the form is truly spherical and lit that way.

## Consistency

Same light vector on every frame of a cycle. Relit frames read as flicker.

## Practice in-repo

Build ramps with `palette ramp --hue-shift`, then assign indices in ASCII legends / shapes / recipes. Treat filter brightness as a last-resort ase tweak, not the primary shade method.
