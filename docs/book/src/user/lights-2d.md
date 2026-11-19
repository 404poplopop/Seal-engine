# 2D lights and canvas shaders

Flat scenes in the Front view can be lit, and a sprite can carry a canvas shader.

## Lights

Select an object and open **2D Light** in the Inspector.

- **Off** leaves the object unlit.
- **Global** lights the whole view evenly.
- **Point** fades to nothing at **Range**.
- **Spot** is a point light limited to a **Cone**.
- **Directional** shines along **Angle** (the object's Z rotation). Solid boxes cast long shadows downstream.
- **Freeform** fills a polygon. **Falloff** is how far the light fades outside the edge. **Points** lists the corners. In the Front view, drag the point handles to move them.

**Blend** mixes the light with what is already there: **Additive** adds it, **Multiply** darkens toward the light colour, **Alpha** fades toward the light colour. **Cast Shadows** lets solid boxes block Point, Spot, Directional and Freeform lights.

## Glow

**Window > Scene Settings > Glow** is off until you turn it on. It draws a soft halo behind bright sprites in the Front view.

- **Threshold** is how bright a sprite must be before it glows.
- **Intensity** is how strong the halo is.
- **Radius** is how far it spreads.

## Canvas shaders

A sprite's **2D Material** has **Blend** and **Light**.

- **Blend** is Alpha, Additive, Multiply or Premultiplied.
- **Light** is Lit (the sprite uses the lights), Unlit (the sprite ignores lights) or Light Only (the sprite shows the light).

**Canvas Shader** uses a `.shade2d` file. The Asset Editor opens that file. A bad line is reported as `line N`.

A shader can name three sections:

- `pixel` is the colour of each pixel (the default section).
- `vertex` moves the corners. Assign `x` and `y`.
- `light` is the colour this sprite makes from one light. It can read `light_r`, `light_g`, `light_b`, `light_amount`, `nx`, `ny`, `nz` and `n_dot`.

Two lines at the top set the material when the file is used:

```
blend = additive
light_mode = unlit
```
