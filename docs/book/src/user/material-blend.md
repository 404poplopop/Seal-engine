# 2D material blend

A sprite material can set **Blend** in the Inspector under the material block. Choices are **Alpha**, **Additive**, **Multiply**, **Premultiplied**, and **Screen**.

In the viewport and in Play, sprites with **Alpha** draw through the normal picture path. Other blends composite with whatever is already drawn behind the sprite in draw order (other sprites and shapes).

In the editor and in an exported game on OpenGL, non-alpha blends use the graphics card. With no OpenGL, **Additive** still matches through the picture path; **Multiply** and **Screen** are approximations.

**Light** mode on the material (Lit, Unlit, Light only) is unchanged for every blend choice.