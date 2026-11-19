# Composite textures

Composite texture files use the `.texture` extension. They combine several pictures into one GPU texture: a **2D array**, a **volume** (stack of slices), or a **cubemap** (six faces).

## Create

In the **Asset Browser**, open the folder where the file should live. Choose **+ Add** → **Texture Array**, **Volume Texture**, or **Cubemap**. The new file opens in the **Asset Editor**.

## Fields

- **GPU format** — `rgba8` or a block format (`bc1`–`bc7`, `etc2`, `astc4x4`). On export, the encoder keeps the block format when the build target supports it; otherwise pictures stay as RGBA8 at load time.
- **Stream mips within memory budget** — large textures may load low mips first and raise detail while the texture covers more screen pixels, capped by **Memory budget (MB)**.
- **Layer**, **Slice**, or **Face** — slider to preview each picture in the Asset Editor.
- **Picture** rows — project-relative paths to source pictures. For a cubemap, assign six faces in +X, −X, +Y, −Y, +Z, −Z order.

**Encode quality** is Fast, Balanced or Maximum. **Encode for export** compresses every layer picture to the GPU format, writes a `.layerN.ktx2` file beside the asset, and shows the packed byte size and worst PSNR in decibels.

When a picture’s import **Compression** is not None, **Export Game** also writes a `.ktx2` next to the picture. The preset’s **Target platform** picks the block format (block compression on desktop, ETC2 or ASTC on mobile and web). The game loader prefers the `.ktx2` sidecar over the PNG when it is present. A `.texture` used as a mesh texture uploads in its GPU format when the graphics card supports it: it uses the `.layerN.ktx2` files from **Encode for export** when they are newer than their pictures, otherwise it encodes the layers once at load with Fast quality; cards without that format get RGBA8. Projects with compression set to None keep the same exported bytes as before.

## Runtime loading

While a scene or game runs, viewport pictures decode on background threads. Until a file is ready, the engine draws a neutral semi-transparent tile sized from the file header (when known). **Project Settings → Rendering → Texture memory budget (MB)** caps uploaded sprite data; least recently drawn pictures drop to coarser mips first under pressure. **Window → Statistics** shows texture memory used versus the budget and how many decodes are still pending.
