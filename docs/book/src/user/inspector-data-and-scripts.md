# Data assets and script inspector layouts

## Data in the Inspector

When a **Data** file (`.data`) is selected in the **Asset Browser**, the **Inspector** lists every field from its **Data Type**. Each row uses the matching control: number, text, yes/no checkbox, choice dropdown, colour, asset reference with the asset picker, or list.

Changes are saved to the file at once and can be undone with **Edit > Undo**.

## Custom inspector on a script

Open a logic graph in the **Graph** panel. Choose **Inspector layout** on the toolbar to edit how the **Inspector** shows objects that use that script:

- **Field** — which entity property the row controls
- **Order** — sort order among custom rules
- **Hidden** — hide the row
- **Min** and **Max** — slider range for number fields
- **Section** — section heading name
- **Help line** — muted hint shown with the row

Objects without a script, or with a graph that has no layout block, keep the default inspector.