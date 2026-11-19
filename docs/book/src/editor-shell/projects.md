# Projects

`file.new-project` writes `project.seal`, a `.gitignore` entry for `.seal-editor/`, and a `.gitattributes` rule for `*.bin`. `file.open-project` reads that manifest. A manifest that does not parse is left in place and the error says repair is available. `file.repair-project` moves the bad file aside, restores the `.bak` when that copy parses, and otherwise writes a new manifest named after the folder. Each project has an id, recorded with its folder in `project-index.seal` in the user state folder. Opening a folder whose id belongs to another project folder that still exists (a copied folder) gives the copy a new id and shows a notice; the original keeps its id. A moved project, whose old folder no longer holds that id, opens with its id unchanged.

Recent projects are stored in `recent.seal`. Opening a project switches the layout file and the scratch journal to `.seal-editor/` inside that folder. Closing the project returns to the per-user layout.
