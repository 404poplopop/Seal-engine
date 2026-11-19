# Test Runner

Window > Test Runner lists every `.sealtest` file in the open project. Each file is a test script: a scene path, a frame count, and assertion nodes. Run all plays each scene headless for that many frames (object scripts run), then checks the nodes. A row shows Pass or Fail. A failed row shows the assertion message.

Fields on a test script, in the Asset Editor:

| Field | Meaning |
| --- | --- |
| Name | Name shown in the Test Runner. |
| Scene | Scene file inside the project. Empty until you set one. |
| Frames | How many frames to play. 1 to 100000. |
| Object | Object name to read after play. |
| Property | X, Y, Z, Name, Tag, or Hidden. |
| Compare | Equal, not equal, less than, less or equal, greater than, or greater or equal. |
| Value | The value the property should match. |

Asset Browser > + Add > Test creates a script in the folder you are viewing. The scene field starts empty.

Write report, with a report file name, writes JUnit XML for the run into the project. An empty report file name does not write a file. The filter, the report file name, and the last results are kept with the other panel state, and the script file is saved on each edit.

A packaged game runs the same scripts without opening the window:

```
game --run-tests report.xml
```

`report.xml` is optional. The process exits with a failure code when any assertion fails.

Coverage for this repository, when `cargo llvm-cov` is installed:

```
python3 tools/coverage.py
```

That writes HTML under `target/coverage` and LCOV at `target/coverage/lcov.info`. `tools/cargo_junit.py` turns `cargo test` text into a JUnit file. The CI workflow builds, tests, and uploads both reports.
