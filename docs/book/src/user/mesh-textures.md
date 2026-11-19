# Mesh textures and sky

A mesh (cube, sphere, cylinder, cone, capsule or plane) can sample a picture or a `.texture` asset on the graphics card. The sample is multiplied by the object colour and the lighting. Without a graphics card the mesh keeps its flat colour.

## Inspector

Select a mesh. In the Inspector, open **Rendering**.

- **Base Colour Texture** — a picture, or a `.texture` file (array, volume or cubemap). Empty keeps the flat colour.
- **Layer** — array layer, shown for a texture array. The graphics card samples that layer.
- **Slice** — volume slice index, shown for a volume texture. The sample sits in the middle of that slice: w = (slice + 0.5) / depth, where depth is the number of slices. A later change can animate w.
- **UV Tile U** and **UV Tile V** — how many times the texture repeats across the mesh. 1, 1 is one copy.

A picture uses a flat sampler. A texture array uses an array sampler. A volume uses a volume sampler. A cubemap uses a cube sampler and the surface normal, with no reflection. There is no Face field on a cubemap material. Shading language 1.x and GLES 2 cannot name the array, volume or cube samplers, so those materials use a flat copy of one layer and the editor logs that once.

Block-compressed formats are uploaded compressed when the graphics card supports them. Otherwise they are decoded to RGBA8 first. Anisotropic filtering uses the **Anisotropy** value in Project Settings on every mesh texture.

## Scene Settings

**Scene Settings** → **Sky Cubemap** takes a cubemap `.texture`. The perspective view and play draw it behind the scene, including when the view looks through the play camera. Empty draws no sky image.

## Asset Editor

Open a `.texture` file in the Asset Editor. When a graphics card is available the preview is the uploaded texture for the selected layer, slice or face. Otherwise the preview is the source picture.
