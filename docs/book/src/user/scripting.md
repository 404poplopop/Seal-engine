# Scripting with graphs

Objects get their behaviour from node graphs. Select an object and press **Open Graph** in the Inspector, or use **New Graph** from the Edit menu. The graph opens in the Graph panel.

- Right-click empty space to search for a node and add it.
- Drag from a pin to another pin to link them. Pins are coloured by type, and only matching types connect. Dropping a wire on empty space opens the node search and links the new node for you.
- Values that are not linked can be typed straight into the node.
- Right-click a node to delete it or remove its links. **Frame all** brings the whole graph into view.

Every change saves on its own, and the graph has its own Undo and Redo.

## Debugging

With the Graph panel focused during play:

| Key | Action |
| --- | ------ |
| F9 | Toggle a breakpoint on the selected node |
| F5 | Continue after a breakpoint |
| F10 | Run one node and stop again |

While stopped, the panel shows the current values of the graph's variables.
