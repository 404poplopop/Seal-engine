# Particle effects

Particle effects are text files ending in `.fx`. To make one, open **+ Add** in the Asset Browser and choose **Particle Effect**: it writes `effect.fx` with default settings in the folder shown (`fx/` at the top) and opens it. To use one, select an object, find the **Particles** section in the Inspector and choose the effect in **Effect**, or open the file in the **Asset Editor** from the Asset Browser.

## Core settings

```text
rate 40                       # particles per second
lifetime 1.2                  # seconds each particle lives
velocity 0 2 0                # starting speed x y z
spread 0.5                    # random variation of the speed
gravity 0 -3 0
size 0.2 0.05                 # size at birth, size at death
color 1 0.9 0.4 1  1 0.3 0.1 0  # colour at birth (r g b a), colour at death
max 200                       # most particles alive at once
seed 7                        # same seed, same pattern every run
```

Size, opacity and colour can use SealText **curves** and **gradients** (`size_curve`, `alpha_curve`, `color_gradient`).

## Bursts and emit shape

Optional lines (omitted in older files):

- `burst time count` — spawn `count` particles at `time` seconds (repeat for multiple bursts). The Asset Editor and Inspector list every burst as a row you can edit or remove; **Burst repeat interval** still repeats the whole list.
- `burst_repeat seconds` — repeat the burst list every interval.
- `shape point` | `shape circle radius` | `shape ring inner outer` | `shape box half_x half_y` | `shape edge x0 y0 x1 y1` | `shape cone direction half_angle` | `shape sphere radius volume|surface` | `shape mesh primitive_or_path surface|vertices`

## Motion

- `velocity_curve` — SealText curve multiplier over life (0–1).
- `rotation min max` and `rotation_rate min max` — start angle range and angular speed range in degrees.
- `rotation_curve` — multiplier over life for rotation speed.
- `drag` — slows base velocity over time.
- `noise strength frequency` — turbulence; deterministic from `seed`.
- `orbital speed` — orbit around the emitter in the horizontal plane.

## Renderer

- `renderer billboard` (default), `renderer stretched factor`, `renderer flipbook path life|random`, `renderer trail points width_end`, `renderer mesh primitive_or_path`.
- `soft_fade distance` — in the 2D view, alpha fades when the particle depth is within `distance` of opaque draws behind it (by sort depth).
- `billboard_material sheet columns rows loop|once` — billboard particles play a sheet frame over each particle’s lifetime.

On sprites, the **2D Material** effect **particle_anim** uses the same sheet layout fields (`mat_shader` picture, columns, rows, loop).
- Flipbook `path` is a `.frames` animation or a picture holding a row of square cells. In the Asset Editor, **Flipbook Picture** picks it and **Frame Choice** sets **Over Lifetime** or **Random**. Choosing Flipbook starts from the first picture or animation in the project.
- `lit 1` — 2D lights affect particles; `lit 0` or omitted draws them unlit.

The Asset Editor shows a live preview and every field. The Inspector **Particles** section lists the same advanced fields when an effect is assigned.

If the file is missing or has a mistake, the Console reports it and play keeps running without that effect.