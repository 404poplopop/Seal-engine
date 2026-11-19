# 2D areas and surfaces

An area effect is a trigger region. Every dynamic body that overlaps it, and whose collision layer is included in the area's layer mask, receives the effect on each physics step. Effects from several overlapping areas add together. A gravity override uses the area with the highest priority. When two gravity areas share that priority, the one with the lower object id wins.

## Creating one

Open the Create panel and use the Areas section, or use the toolbar add menu and choose Areas. The entries are:

- Wind Area. A directional acceleration along +X.
- Water Area. A buoyancy region whose fluid surface sits on the area's position.
- Gravity Area. Replaces gravity with none inside the box.
- Attractor. Pulls bodies toward the centre, weakening in a straight line out to its range.

Each entry adds a fixed trigger with an outline in the flat 2D view. Select it to edit it.

## Inspector

The Area section is on any object that is not a camera or a light.

Mode chooses the effect. None turns the area off. The other fields change with the mode:

- Directional. Layers, priority, force (acceleration or force), direction, strength, and variation (0 to 1).
- Point. Layers, priority, pull (attract or repel), falloff (none, linear, or inverse square), strength, variation, and range. A range of 0 does not cut the effect off by distance.
- Buoyancy. Layers, priority, fluid surface (a height above the area), fluid density, linear drag, angular drag, and flow.
- Gravity. Layers, priority, and either a replacement gravity vector or a scale. A scale of 0 is zero gravity.
- Drag. Layers, priority, extra linear damping, and extra angular damping.

Surface Speed is always shown. A non-zero value makes a solid collider carry bodies that stand on it, along the collider's local +X, including the platformer character. The area mode can stay None when only the surface is used.

## Flat view

In the flat orthographic 2D view, a selected area draws its direction, its range, its fluid surface, or its surface direction.

## Scripts

An area's graph can use the events `body_entered` and `body_exited`. The parameter is `body`, the dynamic body that started or stopped overlapping. The Bodies Inside node lists the dynamic bodies currently inside the area given on its entity pin, lowest id first.
