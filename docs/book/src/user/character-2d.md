# Character 2D

Character 2D moves an object by sliding it along solid colliders. It is off until you turn it on, and it does not replace the side-view player used by the platformer template.

## Creating one

Open the Create panel, section Scene, and choose Character 2D. The same entry is in the toolbar add menu, under Basic. The new object starts with the component on, standing on the ground line.

You can also select an existing object that is not a camera or a light and turn Character 2D on in the inspector.

## Inspector

The Character 2D section lists:

- On. Turns the component on. While it is off, the object is not moved by this component.
- Mode. Walk, Fly, Swim, or Climb.
- Speed, Fly Speed, Swim Speed, and Climb Speed. Units per second at full input.
- Jump Speed. Upward speed of a jump.
- Gravity. Downward acceleration while walking in the air.
- Max Floor Angle. Slopes at or under this angle, in degrees, count as a floor. Steeper slopes count as walls.
- Step Height. How high a ledge the body will step onto.
- Floor Snap. How far below the feet still counts as a floor, so the body stays down slopes and over small bumps.
- Coyote Time. Seconds after leaving a floor during which a jump still counts.
- Jump Buffer. Seconds a jump press is remembered before the feet find a floor.
- Size. Full width and height while standing, before the object's scale.
- Crouch Height. Full height while crouching. It cannot be taller than Size's height.
- Crouch. Shrinks the body and keeps the feet in place. Standing back up is refused while a ceiling is in the way.

Walk uses the horizontal part of the move input, jumps, and is carried by a floor that moves. Fly uses both axes and ignores gravity, water, and climb areas. Swim is used inside a water area (a buoyancy area, or an object tagged water) unless the mode is Fly. Climb is used inside an object tagged climb, again unless the mode is Fly. Leaving the area returns Walk to walking.

A floor contact, a wall contact, and a ceiling contact are reported after each step. One-way colliders block from above only.

## Graphs

These nodes read and drive the component on the object wired to Entity:

- Move Character. Slides by Wish this step. Each axis is usually from -1 to 1. Walk uses the horizontal axis.
- Jump. Asks for a jump. A press just before landing, or just after leaving a floor, still counts.
- Is On Floor. Whether the body stands on a walkable floor.
- Is On Wall. Whether the body is touching a wall or a slope too steep to walk.
- Set Movement Mode. Sets the mode to walk, fly, swim, or climb. A water area or a climb area can still switch Walk.
- Get Velocity. Velocity in scene units per second.

The queries report the previous step. Move and Jump take effect on the next step.
