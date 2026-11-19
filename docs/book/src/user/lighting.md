# Lighting in the viewport

In **Lit** mode the viewport lights 3D shapes with the sun, any extra lights you place,
and their shadows.

## Bounced light

With **Show > Global Illumination** on (the default), light also bounces off surfaces.
The shaded sides of objects pick up a soft glow from a bright floor next to them, and a
coloured wall tints what stands close to it. Walls block the bounce, so a closed room
stays darker than an open yard.

Seal works this out on a grid of light probes spread through the scene. Each probe sends
rays in every direction and sums what they hit. The grid is recalculated when shapes,
colours or lights change. Turn the option off to see direct light only, which can make it easier to judge shadows.

Bounced light is calculated on the processor on every machine. The graphics card's
compute kernels do not support the loops this calculation needs.
