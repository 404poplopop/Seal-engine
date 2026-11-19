# Physics 2D

## Project Settings

Open **Project Settings** and choose the **Physics 2D** category. Fields control the fixed step rate, the most substeps per frame, solver iterations, gravity scale, render interpolation between fixed steps, and sleeping thresholds. Values are stored in the project and apply when you press Play.

**Platformer** scenes (a player without its own 2D body, with side-view ground or coins) use the fixed step rate, max substeps per frame, and render interpolation from this category. Solver iterations, gravity scale, and sleeping affect other scenes that simulate 2D bodies on objects; they do not change the built-in platformer stepper.

## Body properties

Select an object with a 2D body. In the Inspector, open **2D Physics**. Besides shape and layers you can set density, extra mass, centre of mass offset, gravity scale, linear and angular damping, sleep and rotation lock, and constant force and torque.

## Joints

With **Joint** set to something other than **None**, the **2D Physics** section lists joint fields.

- **Friction** resists relative linear and angular motion. **Max Force** and **Max Torque** cap how strongly it pushes back.
- **Relative** keeps a fixed linear **Offset** and angular value in **Limits** (lower number, degrees) to the other body. **Correction** is how strongly it snaps (0 to 1). **Max Force** and **Max Torque** limit correction.
- **Target** pulls this object's **Anchor** toward **Target Point** in the world. **Stiffness** is pull frequency, **Damping** slows oscillation, and **Max Force** caps the pull. In a logic graph, **Set Joint Target 2D** moves the pull point at run time.
- **Break Force** and **Break Torque** (on every joint kind) remove the joint when the solver reaction exceeds the value; zero means the joint never breaks. Wire the **joint_break** event on that object to react in a graph.

## Physics 2D Profiler

While play is running, open **Window → Physics 2D Profiler**. The panel shows body, collider, contact, and joint counts and the last step time in milliseconds, with a short history of step times.