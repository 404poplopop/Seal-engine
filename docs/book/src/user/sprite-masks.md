# Sprite masks

A sprite mask hides parts of sprites. The shape is a rectangle, a circle, or the opaque parts of a picture. The mask follows the object's position, rotation and scale, including while a timeline moves that object.

Open the Create panel and choose **Sprite Mask**. The new object is a rectangle two units wide. Select it. The Inspector shows a **Sprite Mask** section:

- **Shape** is Rectangle, Circle or Picture. None removes the mask.
- **Picture** and **Alpha Cutoff** appear when the shape is a picture. A pixel counts as inside when its alpha is at least the cutoff.
- **Layer From**, **Layer To**, **Order From** and **Order To** choose which sprites the mask affects. A sprite outside that range is drawn whole.

A sprite has **Mask Interaction** in its Sprite section:

- **None** draws the sprite whole.
- **Visible Inside Mask** keeps the sprite only where at least one mask covers it.
- **Visible Outside Mask** keeps the sprite only where no mask covers it.

Several masks add together. A pixel is inside when any mask in range covers it. The flat view, play and an exported game clip sprites the same way. A selected mask draws its outline in the viewport.
