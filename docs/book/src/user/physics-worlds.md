# Physics worlds

A scene can hold more than one physics world. Each world has its own gravity and backend, and each world steps on its own. The ordinary play simulation is unchanged for objects that have not picked a world.

## Panel

Open **Window → Physics**.

**Worlds** lists `main` and any world you add. Gravity is a positive downward pull. Backend is **Built-in** or **Drift**. Drift slides dynamic bodies along X and does not apply gravity. **Add World** creates `preview`, then `preview-2`, and so on. **Remove** deletes a world other than `main` and clears that choice on objects.

**Object** edits the current selection.

- **Physics World** is None, `main`, or an added world. None leaves the object on the ordinary play simulation.
- **Collider** is None, Cylinder, Triangle Mesh, Heightfield or Convex Hulls.
- Cylinder uses **Radius** and **Half Height**. The cylinder stands on Y.
- Triangle Mesh and Convex Hulls use **Mesh**, a project-relative file of `v x y z` and `f i j k` lines. Empty is left unset.
- Convex Hulls also uses **Max Hulls** (1 to 64). The mesh is split into that many convex pieces, and those pieces cover the mesh vertices.
- Heightfield uses **Picture**, **Width**, **Height** and **Depth**. The picture is a png, a ppm (`P5` or `P6`), an uncompressed 24-bit bmp, or a text grid (`grid columns rows` and one brightness sample per pixel, 0 to 1). Brightness times Height is the ground height. Empty is left unset.

Edits are part of the scene file and undo with the scene.

## Graph nodes

These nodes talk to the physics server by handle. They do not create scene objects.

- `physics_body` makes a body in a named world at `x`, `y`, `z`. `dynamic` falls under that world's gravity. The output `body` is a handle.
- `physics_shape` attaches a shape to a body handle. `kind` is `cylinder`, `trimesh`, `heightfield` or `convex`.
- `physics_joint` joins two body handles. `kind` is `fixed`, `distance` or `hinge`. `length` of 0 keeps the distance they have now.
- `physics_step` steps that world by `dt` seconds. A world you name here, that is not one of the scene worlds, steps only when this node runs.
- `physics_position` reads a body handle's position.

Scene worlds listed on the Physics panel step during play. Objects assigned to one of them move with that world and are left out of the ordinary play simulation.
