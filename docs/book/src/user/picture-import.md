# Picture import

The Asset Editor shows import settings for a picture. The same settings are stored in the sidecar next to the file (`name.png.import.seal`) and are kept when the editor is closed.

## Formats

The editor, the Asset Editor and an exported game read PNG, JPEG, BMP, GIF (the first frame), TGA, WebP, Radiance HDR (`.hdr`), EXR (`.exr`), SVG (`.svg`), KTX2 (`.ktx2`) and DDS (`.dds`).

HDR and EXR keep the linear samples. The byte copy used for display is tone-mapped: a picture whose samples are all at or below 1 keeps those values, and a brighter sample is pulled toward white so the dimmer samples in the same picture stay visible. With no colour space in the sidecar, those files draw as Linear.

SVG is drawn as a picture. **SVG Size** is the longer side in pixels, a power of two from 32 to 4096. The default is 256. Changing it saves the sidecar and draws the file again at the new size.

KTX2 and DDS are read uncompressed, or block-compressed in the BC1, BC2, BC3, BC4, BC5, BC6 and BC7 forms, and shown as straight pixels. A file that is not one of these, or that is damaged, shows the unreadable-picture message and does not stop the editor.

## Compression

**Compression** is None, Lossless, Block Low, Block Medium or Block High. None keeps the file. The other choices are applied by Export Game: the package writes a packed picture beside the source, and the game reads that packed picture. Lossless restores the same pixels. The three block levels store a smaller picture; Block High keeps the most detail and Block Low the least. The source file stays in the package as well.

## Sprite mesh

**Sprite Mesh** is Full Rect or Tight. Full Rect draws the whole rectangle, which is how every picture was drawn before this setting. Tight builds an outline around the opaque pixels and draws that outline, so empty corners are not filled. The outline is saved beside the picture and used in the viewport and in an exported game. A picture that is opaque across its whole rectangle stays a rectangle.
