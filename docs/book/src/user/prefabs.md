# Prefabs

A prefab is a saved object, with everything under it, that you can place many times.
Seal Hop's coin or a platform with its tiles are good prefabs.

## Make a prefab

Select the object, then right-click it in the viewport and choose **Create Prefab**
(also in the Scene menu). Seal saves it as `prefabs/<Name>.prefab` in your project and
turns the object into the first copy. An existing file is never overwritten; a second
prefab with the same name becomes `<Name> 2`.

## Place copies

Click a prefab in the **Prefabs** list of the Create panel, or drag a `.prefab` file from the
Asset Browser into the viewport, where the copy lands at the drop point. You can also run
**Place Prefab** with the prefab's path, for example `prefabs/Coin.prefab`. A clicked copy goes
under the selected object, or at the top of the scene when nothing is selected.

## Change one copy

Each copy keeps its own position, rotation, scale, visibility and lock. Any other field
you change on a copy in the Inspector, such as its role or sprite, is remembered as that
copy's own change.

To keep or drop those changes, open the prefab button in the Inspector header of a copy.
**Apply Changes to Prefab** writes the copy's own changes into the prefab file and updates
every copy. **Revert to Prefab** drops all of them, and the list below it has a
**Revert** button for each changed field on its own. Both are one undo step.

## Update copies from the file

When a scene opens, every copy is rebuilt from its prefab file. You can also run
**Update Prefab Copies** at any time. Each copy takes the file's values and its
children come fresh from the file, while the copy's own changes and placement stay.
The update is one undo step.

Objects you added under a copy by hand are replaced by the prefab's children when
it updates, so add extra objects next to a copy rather than inside it.

## Edit a prefab

Double-click a `.prefab` file in the Asset Browser, click the **Edit** link in a copy's
Type column in the Outliner, or select a copy and pick
**Edit Prefab** from the prefab button in the Inspector. The level steps aside and only the
prefab is shown, with a banner across the viewport. Every change is saved to the prefab
file straight away, and undo works as usual while you edit.

Click **Done Editing Prefab** in the Inspector to go back. The level returns as you left it,
and every copy is updated from the file. That update is one undo step in the level.
