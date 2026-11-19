# State machines

A graph can carry one state machine. Open the graph in the Graph panel and press **States**. The same button switches back to **Nodes**.

States are boxes. The initial state is marked and is the one entered on the first step. Each state has three events on the node canvas: On Enter, On Update and On Exit. On Update runs every step. A transition runs On Exit, then On Enter, including a transition back to the same state.

Drag from the port on the right of a state to another state to add a transition. A transition has a priority and one condition:

| Condition | Fires when |
| --- | --- |
| Flag | A bool variable is true |
| Compare | A variable compares to a value you type |
| Event | That event name was noted this step |
| After | The state has been current for at least that many seconds |

When several transitions could fire, the highest priority wins. Equal priorities keep the order they were added. Only one transition runs in a step.

The nodes **Set State**, **Get Current State**, **Time In State** and **Trigger** work with the machine. Set State changes state immediately. Trigger notes an event name for an Event condition. During play the current state is drawn with a gold outline, and the panel names it.

Edits save with the graph and undo with the graph's Undo.
