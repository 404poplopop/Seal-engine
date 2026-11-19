# Text, string tables and sockets

A label or a button can show a picture in the middle of its text. Write the picture as `[img=path]` or, for one clip of a sprite animation, `[img=path#clip]`. The picture is as tall as the text. A tag the file does not know stays as a picture-sized gap. A tag that is missing its closing bracket stays as the text you typed.

A string table is a file you add from the Asset Browser: open the Add menu and choose String Table. Each row is a key and a line of text. A placeholder is written `{name}`. On a screen, set String table to that file. A label whose text starts with `@key` shows the line for that key. Extra words on the label fill placeholders, as in `@coin count=3 name="Ada B"`. A key the table does not have shows the key, and the Console warns once.

The same lookup is a graph node called Get String. It takes the table file, the key, and the placeholder values.

Typewriter, Wave and Shake are properties of a label or a button.

- Typewriter is how many characters appear each second. Zero shows the whole line. The graph node Typewriter restarts that label, or skips to the end when Skip is on. When the line finishes, the graph receives `typewriter_` followed by the label's name and `_finished`.
- Wave is an amount in pixels and a speed in cycles per second. Each character moves up and down.
- Shake is an amount and a speed. Each character moves a little on both axes.

A sprite animation can name a point on each frame. In the animation's Asset Editor, Add Socket, then drag the point on the preview. Pause the preview to edit one frame. An object's inspector has Attach to Socket. The list is the socket names on its parent's sprite animation. While the game is playing, the object sits on that point, including when the picture is mirrored.
