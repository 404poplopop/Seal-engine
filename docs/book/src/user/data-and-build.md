# Data assets and build settings

## Data Type and Data assets

A **Data Type** (`.datatype`) defines named fields: number, text, bool, colour, asset, or list.

A **Data** asset (`.data`) holds values for one type. Create a Data Type from a folder context menu in the Asset Browser (**New Data Type**), then **New Data** in the same folder.

Open either file in the Asset Editor to edit fields and values. Values are checked against the type.

Select a **Data** file in the **Asset Browser** to edit its values in the **Inspector** as well (see [Data and script inspector layouts](inspector-data-and-scripts.md)).

Object scripts can use **Load Data**, **Get Field**, **Read CSV Cell**, and **Read JSON Value** graph nodes to read `.data` files, CSV, and JSON in the project.

## Prefab variants

Right-click a prefab in the Asset Browser and choose **Create Variant**. The variant stores only changes from its base prefab. Editing the base updates variants except on overridden fields.

## Build scene list

Use **File > Build Scene List** to open the **Build** panel. The ordered list is stored in project settings and used by **Export Game**. The first scene is the start scene when the game is packaged.

## User project templates

**File > Save Project as Template** copies the open project into your templates folder with a title and description. The preview uses a project image when available.

The **New Project** wizard lists saved templates with the built-in starters.