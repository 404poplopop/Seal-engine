# Script core

Quaternions, planes, clocks, text encoding, line layout, typed JSON, and Timeline time warping.

## Graph nodes

In the Graph editor, pure nodes include `quat_euler`, `quat_axis_angle`, `quat_mul`, `quat_slerp`, `quat_rotate_vec`, `plane_point_normal`, `plane_distance`, `plane_ray_hit`, `utf8_to_utf16`, `utf16_to_utf8`, `layout_lines`, `read_typed_json`, `write_typed_json`, `unscaled_time`, `unscaled_dt`, and `sim_tick`. Types `quat` and `plane` are four-number values.

## Timeline time warp

Open **Window → Timeline**, pick a sequence, scrub to a clock time, enter **Content time**, and click **Set Time Warp Key**. **Clear Time Warp** removes the curve.

## Data types and JSON

Create Data Type and Data assets in the Asset Browser. `read_typed_json` fills fields from JSON text; `write_typed_json` exports a Data asset to JSON with type checking.