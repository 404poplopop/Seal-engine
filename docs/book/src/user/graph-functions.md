# Functions

A graph can hold named functions next to its events. A function has inputs, outputs, local variables and its own nodes. The event graph is unchanged.

Open a graph in the Graph panel. The bar above the canvas lists **Event graph** and one button per function. **+ Function** adds a function and opens it. **Delete** removes the function that is open. Closing the panel does not drop the function: it is saved in the graph file, and opening the graph again shows the same function.

The second row edits that function:

| Field | What it stores |
| --- | --- |
| Name | The function name. Letters, digits and underscores, starting with a letter. |
| Inputs | Name and type of each value the caller passes in. |
| Outputs | Name and type of each value the function hands back. |
| Locals | Name and type of each variable that belongs to this function. |

Types are `bool`, `int`, `float`, `str`, `vec2`, `vec3`, `entity` and `list<bool>`, `list<int>`, `list<float>`, `list<str>`, `list<vec2>`, `list<vec3>`, `list<entity>`.

The body starts with an **entry** node. Its output pins are the inputs. An **fn_return** node ends the function; its input pins are the outputs. Right-click the canvas to add nodes. Each function is listed there as **Call** followed by its name. The Call node's pins match that function. A Call node can sit on the event graph or inside another function.

A local starts from its default on every call, so a later call does not see the value left by an earlier one. Graph variables are shared and keep their values.

A function cannot call itself, including through another function. The graph reports that and will not run until the cycle is gone.

Renaming a function, or renaming one of its pins, updates every Call node that uses it, and the links on those pins. Changing a pin's type leaves a link that no longer matches in place; the graph reports it as a problem.

Undo and Redo include these edits.
