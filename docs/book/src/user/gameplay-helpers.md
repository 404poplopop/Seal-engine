# Volumes, input timing, tick order and players

## Volumes

Create > Areas, and the toolbar add menu under Areas or Volumes, add four volumes. Each one is a fixed trigger box with an outline.

- Kill Zone. A player who enters is destroyed, or moved to the last checkpoint. With no checkpoint, the player goes to an object tagged `player_start`, or is destroyed when there is none. The volume's event is `killed` unless you name another.
- Damage Zone. While a player is inside, the named variable loses points at Damage Per Second. The default variable is `health` and the default rate is 10. The same amount is taken from the score of that name, and from a kept value of that name when the kept value is a number. The event is `damaged` unless you name another.
- Checkpoint. A player who enters stores this position. A later Kill Zone set to Respawn uses it. The event is `checkpoint` unless you name another.
- Trigger Volume. Other objects fire enter, stay and exit events. Tag Filter lists the tags that count, separated by commas. An empty filter counts every tag. The event name is the prefix: an empty event uses `trigger_enter`, `trigger_stay` and `trigger_exit`. A name `door` uses `door_enter`, `door_stay` and `door_exit`.

Select the volume to edit it in the Inspector Volume section. The scene file stores the volume only while its kind is set.

A player is an object tagged `player`, or the object a player currently controls.

## Input modifiers

Open `input.sealinput` in the Asset Editor. Each binding has a modifier:

- None. The action is down while the button is down.
- Hold. The action is down only after the button has been held for the given seconds.
- Double Tap. The action is down when a second press lands within the given seconds of the first, and while that second press is held.
- Toggle. Each press turns the action on or off. Releasing the button does not.

The file writes a modifier only when one is set, so a bindings file with none of them is unchanged. Changes save at once.

Graph nodes:

- Action Held Time. Seconds the action has been held after its modifier. 0 while it is up.
- Action Released. Whether the action went up this step.
- Mouse Delta. Pointer movement this step, in screen pixels. x is to the right, y is down.
- Mouse Wheel. Positive is away from the user.
- Pointer World. Where the pointer is in the flat scene, in scene units.

## Tick order

A graph can listen for `fixed_tick` and `late_tick` the same way it listens for `tick`. Each passes `dt`, the seconds of that step.

`fixed_tick` runs at the physics step. `late_tick` runs after physics and after animation for that step. `tick` stays where it is today. A graph that does not listen for `fixed_tick` or `late_tick` does not receive them.

The Inspector Script section has Order. Lower values run first. 0 keeps the scene's own order, and equal values keep that order too. The scene file stores Order only when it is not 0.

## Players

- Spectate. Points the play camera at an object without changing who a player controls. Turn `on` off to follow the usual target again.
- Unpossess. Player `index` (0 is the first) lets go of the object it controls.
- Press Start to Join. In Scene settings, under Physics. While play runs, the Start button on a gamepad that has not joined assigns the next free player. That player controls the next object tagged `player`, when there is one. The first gamepad is player 0's pad. The choice lasts until play stops.
