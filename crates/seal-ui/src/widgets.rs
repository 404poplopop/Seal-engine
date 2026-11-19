//! The widgets panels are built from. One place decides how a row, a field or a list looks.

use crate::crisp::CrispCombo as _;
use egui::{Align2, Color32, Sense, Stroke, TextStyle};

/// One frame of a panel body. The concrete toolkit stays inside this crate.
pub struct PanelFrame<'a> {
    ui: &'a mut egui::Ui,
}

impl<'a> PanelFrame<'a> {
    /// Wrap a row inside a popup so the same property widgets can run there.
    pub(crate) fn from_ui(ui: &'a mut egui::Ui) -> Self {
        Self { ui }
    }
}

/// Location / vector edit space shown beside the row label.
#[derive(Clone, Copy, Debug)]
pub struct VectorRowMode<'a> {
    pub current: &'a str,
    pub options: &'a [(&'a str, &'a str)],
}

/// One tile in [`PanelFrame::file_browser`].
#[derive(Clone, Debug, PartialEq)]
pub struct FileTile {
    /// Text under the tile; a search result keeps its subfolder (`art/hero.png`).
    pub label: String,
    pub folder: bool,
    /// Where the file is, for its picture preview.
    pub path: Option<std::path::PathBuf>,
    /// Asset type id (`scene`, `picture`, `sound`, ...), shown as a coloured strip.
    pub kind: &'static str,
    /// Asset type name shown under the label.
    pub kind_label: String,
    /// Project path carried when the tile is dragged onto an asset field.
    pub drag_path: String,
    /// Cached thumbnail on disk (`.seal-editor/thumbs/`).
    pub thumb_disk: Option<std::path::PathBuf>,
}

/// What the user did in [`PanelFrame::file_browser`].
#[derive(Clone, Debug, PartialEq)]
pub enum FileHit {
    Select(usize),
    Open(usize),
    Rename(usize),
    Delete(usize),
    Up,
    Menu(usize, usize),
    /// Entry of the menu shown when right-clicking empty space in the grid.
    Background(usize),
    RenameEdit(String),
    RenameCommit(String),
    RenameCancel,
}

/// Width of the slot before the X field (space picker, scale lock, or blank).
const VECTOR_PREFIX_PX: f32 = 22.0;

/// Value change per dragged pixel for a vector row.
fn vector_drag_step(style: &VectorRowStyle<'_>) -> f32 {
    if style.unit_suffix == Some("°") {
        0.5
    } else if style.uniform_scale_locked.is_some() {
        0.01
    } else {
        0.05
    }
}

/// Optional chrome on a three-component vector row.
#[derive(Clone, Debug, Default)]
pub struct VectorRowStyle<'a> {
    pub mode: Option<VectorRowMode<'a>>,
    pub unit_suffix: Option<&'a str>,
    pub uniform_scale_locked: Option<bool>,
    pub uniform_scale_tip: Option<&'a str>,
}

/// Inspector checkbox value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PropertyCheckbox {
    #[default]
    Off,
    On,
    Mixed,
}

/// Drag lifecycle for an inspector number row (undo coalescing).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NumberDragSignals {
    pub drag_started: bool,
    pub drag_stopped: bool,
    pub escape_cancel: bool,
    pub commit_focus: bool,
}

/// Remaining panel body under any chrome already drawn in this frame.
pub fn panel_body_rect(ui: &egui::Ui) -> egui::Rect {
    let clip = ui.clip_rect();
    let max = ui.max_rect();
    let cursor = ui.cursor().min;
    egui::Rect::from_min_max(
        cursor,
        egui::pos2(clip.right().min(max.right()), clip.bottom().min(max.bottom())),
    )
    .intersect(clip)
}

/// Tile rectangles for multi-viewport layouts.
pub fn viewport_tile_rects(outer: egui::Rect, preset: &str, gap: f32) -> Vec<egui::Rect> {
    let gap = gap.max(2.0);
    if outer.width() < 8.0 || outer.height() < 8.0 {
        return vec![outer];
    }
    match preset {
        "two_cols" => {
            let w = (outer.width() - gap) * 0.5;
            vec![
                egui::Rect::from_min_size(outer.min, egui::vec2(w, outer.height())),
                egui::Rect::from_min_size(
                    egui::pos2(outer.min.x + w + gap, outer.min.y),
                    egui::vec2(w, outer.height()),
                ),
            ]
        }
        "two_rows" => {
            let h = (outer.height() - gap) * 0.5;
            vec![
                egui::Rect::from_min_size(outer.min, egui::vec2(outer.width(), h)),
                egui::Rect::from_min_size(
                    egui::pos2(outer.min.x, outer.min.y + h + gap),
                    egui::vec2(outer.width(), h),
                ),
            ]
        }
        "three" => {
            let w = (outer.width() - gap) * 0.58;
            let w2 = outer.width() - w - gap;
            let h = (outer.height() - gap) * 0.5;
            vec![
                egui::Rect::from_min_size(outer.min, egui::vec2(w, outer.height())),
                egui::Rect::from_min_size(
                    egui::pos2(outer.min.x + w + gap, outer.min.y),
                    egui::vec2(w2, h),
                ),
                egui::Rect::from_min_size(
                    egui::pos2(outer.min.x + w + gap, outer.min.y + h + gap),
                    egui::vec2(w2, h),
                ),
            ]
        }
        "four" => {
            let w = (outer.width() - gap) * 0.5;
            let h = (outer.height() - gap) * 0.5;
            vec![
                egui::Rect::from_min_size(outer.min, egui::vec2(w, h)),
                egui::Rect::from_min_size(
                    egui::pos2(outer.min.x + w + gap, outer.min.y),
                    egui::vec2(w, h),
                ),
                egui::Rect::from_min_size(
                    egui::pos2(outer.min.x, outer.min.y + h + gap),
                    egui::vec2(w, h),
                ),
                egui::Rect::from_min_size(
                    egui::pos2(outer.min.x + w + gap, outer.min.y + h + gap),
                    egui::vec2(w, h),
                ),
            ]
        }
        _ => vec![outer],
    }
}

pub fn paint_viewport_pane_border(ui: &egui::Ui, rect: egui::Rect, active: bool, play_frame: bool) {
    if play_frame {
        let color = ui.visuals().warn_fg_color;
        ui.painter().rect_stroke(rect, 0.0, Stroke::new(2.0, color), egui::StrokeKind::Outside);
        return;
    }
    if !active {
        return;
    }
    let color = ui.visuals().selection.stroke.color;
    ui.painter().rect_stroke(rect, 0.0, Stroke::new(2.0, color), egui::StrokeKind::Outside);
}

pub struct TreeRow<'a> {
    pub depth: usize,
    pub has_children: bool,
    pub open: bool,
    pub label: &'a str,
    pub detail: &'a str,
    pub selected: bool,
    pub hidden: bool,
    pub locked: bool,
    pub menu: &'a [String],
    pub drag_id: Option<&'a str>,
    /// When set, drag supplies `place:<kind>` for viewport drop instead of outliner reparent.
    pub place_kind: Option<&'a str>,
    pub rename_text: Option<&'a mut String>,
    /// Child row under an expanded entity (component attachment).
    pub component_row: bool,
    /// Image file shown as a small preview before the label.
    pub thumb: Option<&'a str>,
    /// Show the visibility and lock toggles (scene rows); files have neither.
    pub marks: bool,
}

pub enum TreeClick {
    None,
    Select,
    /// Add or remove this row from the multi-selection.
    SelectExtend,
    /// Shift+click: select every visible row between the anchor and this row.
    SelectRange,
    Toggle,
    Eye,
    /// Alt+click on the eye column: hide everything except this entity.
    EyeSolo,
    Lock,
    /// Optional-column click toggles whether the row can be picked.
    Unselectable,
    /// Index into the context menu entries the row was given.
    Menu(usize),
    Reparent {
        child: String,
        parent: String,
    },
    Reorder {
        child: String,
        before: Option<String>,
        after: Option<String>,
    },
    SpawnPlace {
        parent: String,
        kind: String,
    },
    Rename(String),
    /// Second slow click on a label; start in-place rename for that row.
    BeginRename,
    /// Esc while editing a row label.
    RenameCancel,
    /// Double-click a leaf row to frame it in the viewport.
    FrameEntity,
    /// Type-column `Edit <asset>` link on prefab or script rows.
    EditAsset {
        kind: String,
        asset: String,
    },
    ContextPick(crate::ContextMenuPick),
}

/// One thing drawn in the grid viewport, in scene units.
#[derive(Clone)]
pub struct GridMark<'m> {
    pub x: f32,
    pub y: f32,
    pub half: [f32; 2],
    pub round: bool,
    pub billboard: Option<crate::icons::Icon>,
    pub selected: bool,
    /// Drawn pale, for a surface seen face-on that would hide everything else.
    pub faint: bool,
    pub label: &'m str,
    pub tint: [u8; 3],
    /// Rotation of the drawn shape, in radians, in the plane of the view.
    pub angle: f32,
    /// Distance along the view axis; lower values draw in front when marks overlap.
    pub depth: f32,
    /// Half-size on X, Y, and Z in scene units for lit mesh drawing.
    pub size: [f32; 3],
    pub mesh: Option<crate::viewport_mesh::MeshPrimitive>,
    /// When false, the mark is drawn but ignored for click, drag, and marquee pick.
    pub pickable: bool,
    /// Image file drawn over the mark's x and y extent instead of a shape.
    pub sprite: Option<String>,
    /// Sprite draw order: higher layers draw on top of lower ones.
    pub layer: i32,
    /// Texture rectangle of the sprite frame to show: u0, v0, u1, v1 (v down).
    pub uv: [f32; 4],
    /// Sprite draw order within its layer: higher draws on top.
    pub order: i32,
    /// Colour and opacity multiplied into the sprite picture.
    pub sprite_tint: [u8; 4],
    /// Nine-slice borders in picture pixels (left, right, top, bottom); zero draws whole.
    pub slice: [f32; 4],
    /// Nine-slice edge mode: `stretch` or `tile`.
    pub slice_mode: &'m str,
    /// World width and height when slicing; zero uses half extents.
    pub slice_size: [f32; 2],
    /// Pivot inside the sprite: 0,0 is the bottom left, 0.5,0.5 the centre.
    pub pivot: [f32; 2],
    /// Normal map file shaping how 2D lights fall on the sprite.
    pub normal_map: Option<String>,
    /// A drawn 2D line or filled shape, in scene units on the view plane.
    pub shape2d: Option<ShapeMark>,
    /// Spline terrain on the view plane, when the object is a sprite shape.
    pub sprite_shape: Option<crate::SpriteShapeMark>,
    /// The sprite's material, its shader path already made absolute.
    pub material: Option<seal_editor_core::material2d::Material2d>,
    /// 0 draws the sprite whole, 1 keeps the inside of masks, 2 keeps the outside.
    pub mask_mode: u8,
    /// 0 is not a mask. 1 rectangle, 2 circle, 3 picture alpha. Selected masks draw an outline.
    pub mask_kind: u8,
    /// Base colour texture for a lit mesh. Empty keeps the flat colour.
    pub surface: crate::gpu_textures::MeshSurface<'m>,
}

/// A 2D line or filled shape to draw, its points already placed on the view plane.
#[derive(Clone, Debug, PartialEq)]
pub struct ShapeMark {
    pub points: Vec<[f32; 2]>,
    /// Line thickness in scene units.
    pub width: f32,
    pub color: [u8; 4],
    pub filled: bool,
    pub closed: bool,
    /// Draw a handle on each point so the polygon can be dragged.
    pub edit_points: bool,
    /// Picture coordinates, one per point. Empty keeps the flat colour.
    pub uvs: Vec<[f32; 2]>,
    /// Picture file. Empty keeps the flat colour.
    pub texture: String,
    /// A styled line, already placed on the view plane, with a colour per corner.
    pub ribbon: Vec<([f32; 2], [u8; 4])>,
    pub ribbon_tris: Vec<[usize; 3]>,
}

/// Pointer data for the Measure tool, collected in the same frame as the viewport grid.
#[derive(Clone, Copy, Debug)]
pub struct MeasurePointer {
    pub pos: Option<egui::Pos2>,
    pub primary_pressed: bool,
    pub primary_released: bool,
    pub primary_down: bool,
    pub double_clicked: bool,
    pub shift: bool,
    pub rect: egui::Rect,
    pub origin: egui::Pos2,
    pub cell: f32,
}

pub struct GridHit {
    pub clicked: bool,
    pub key: Option<String>,
    pub pick_shift: bool,
    pub pick_ctrl: bool,
    /// Mark that was clicked or grabbed.
    pub picked: Option<usize>,
    /// Mark being dragged, and where its center now is in scene units.
    pub moved: Option<(usize, f32, f32)>,
    /// True on the first frame of a drag, so the editor can record one undo step.
    pub began: bool,
    /// Right-click menu: the mark under the pointer, if any, and the chosen command.
    pub menu: Option<(Option<usize>, crate::ContextMenuPick)>,
    /// Marks fully inside the marquee, when the user boxed an area on empty space.
    pub marquee: Option<Vec<usize>>,
    /// Marks inside a lasso polygon (Alt+drag on empty space).
    pub lasso: Option<Vec<usize>>,
    /// Pan, zoom, or orbit changed and should be saved.
    pub camera_dirty: bool,
    /// Camera speed changed from RMB + wheel in perspective fly mode.
    pub camera_speed: Option<f32>,
    pub gizmo_move: Option<(usize, [f32; 3])>,
    pub gizmo_rotate: Option<(usize, [f32; 3])>,
    pub gizmo_scale: Option<(usize, [f32; 3])>,
    /// First frame of Alt+drag on an entity in move mode should duplicate then move.
    pub alt_drag: bool,
    /// Ctrl+LMB/RMB/both entity drag along one world axis without the gizmo.
    pub move_axis: Option<(usize, u8, [f32; 3])>,
    /// `End` pressed while the viewport has focus.
    pub floor_key: bool,
    /// Create panel or content drop onto the floor: kind and scene coordinates.
    pub place_drop: Option<(String, f32, f32)>,
    /// Sprite brush: cell centres (scene units) the pointer pressed or dragged over this frame.
    pub paint_cells: Vec<(f32, f32)>,
    /// The same cells as grid coordinates on the brush's grid.
    pub paint_tiles: Vec<(i32, i32)>,
    /// Cells committed when the user finished one stroke (one undo step).
    pub paint_stroke_commit: Option<Vec<(i32, i32)>>,
    /// Cells to draw as a semi-transparent overlay before they land.
    pub paint_preview: Vec<(i32, i32)>,
    /// Set when the Measure tool is active so the panel can finish a ruler stroke.
    pub measure: Option<MeasurePointer>,
    /// A sprite-shape handle, insert, delete or corner change.
    pub spline: Option<crate::SplineEdit>,
    /// A freeform light point being dragged.
    pub poly: Option<crate::PolyEdit>,
}

#[derive(Clone)]
struct EntityGrab {
    index: usize,
    offset: egui::Vec2,
    start_trans: [f32; 3],
}

#[derive(Clone)]
struct PivotDrag {
    offset: [f32; 3],
}

#[derive(Clone)]
struct GizmoGrab {
    index: usize,
    handle: crate::viewport_gizmo::MoveHandle,
    start_trans: [f32; 3],
    start_pointer: egui::Pos2,
}

#[derive(Clone)]
struct RotateGrab {
    index: usize,
    handle: crate::viewport_gizmo::RotateHandle,
    start_rot: [f32; 3],
    start_pointer: egui::Pos2,
    start_angle: f32,
}

#[derive(Clone)]
struct ScaleGrab {
    index: usize,
    handle: crate::viewport_gizmo::ScaleHandle,
    start_scale: [f32; 3],
    start_pointer: egui::Pos2,
}

/// Pan, zoom, and orbit of the 2D viewport grid.
#[derive(Clone, Copy, Debug)]
pub struct ViewportCamera {
    pub pan_x: f32,
    pub pan_y: f32,
    pub zoom: f32,
    pub yaw_deg: f32,
    pub pitch_deg: f32,
    pub distance: f32,
}

/// Optional viewport chrome. Stats and names stay off unless the session turns them on.
pub struct ViewportOnScreenWarning<'a> {
    pub title: &'a str,
    pub hint: &'a str,
    /// When false the banner uses warning yellow instead of error red.
    pub error: bool,
}

pub struct GridViewOptions<'a> {
    pub stats_overlay: Option<&'a str>,
    /// Corner for the stats/FPS overlay text.
    pub stats_overlay_edge: seal_editor_core::viewport_overlays::Edge,
    pub scene_caption: Option<&'a str>,
    /// A packaged game: no axis lines, axis gizmo or focus frame.
    pub game_view: bool,
    /// While the world runs: the badge text and whether it is playing (green) or paused (amber).
    pub play_badge: Option<(&'a str, bool)>,
    /// Text the playing game's on-screen widgets show, one line each.
    pub game_ui: &'a [String],
    /// The playing game's on-screen controls, in pixels of the view (picture paths absolute).
    pub game_screens: &'a [seal_editor_core::gameui::Draw],
    pub warnings: &'a [ViewportOnScreenWarning<'a>],
    pub show_names: bool,
    pub wireframe: bool,
    pub shade: &'a str,
    pub perspective: bool,
    pub camera_speed: f32,
    pub orbit_pivot: Option<(f32, f32)>,
    pub light_dir: [f32; 3],
    /// HDR headroom for lit faces. 1 keeps the 8-bit colour.
    pub light_gain: f32,
    /// Cast directional shadows in the lit perspective view.
    pub shadows: bool,
    /// Shadow map texel width for the lit perspective view.
    pub shadow_map_size: usize,
    /// Extra lights baked into irradiance probes for coloured fill light; empty for none.
    pub fill_lights: &'a [seal_editor_core::probes::Light],
    /// Probes also carry light bounced off the scene (global illumination).
    pub gi: bool,
    /// Redraw the world X, Y and Z lines semi-transparent over everything, so they
    /// stay visible behind sprites and solid shapes (Show > Axis Lines on Top).
    pub axes_on_top: bool,
    pub gizmo_pivot: Option<usize>,
    pub gizmo_world: Option<[f32; 3]>,
    pub gizmo_start_rot: Option<[f32; 3]>,
    pub gizmo_start_scale: Option<[f32; 3]>,
    pub gizmo_space: &'a str,
    pub vertex_snap: bool,
    /// The Create panel's active sprite brush (`sprite:<path>`); left-drag paints it.
    pub paint_brush: Option<&'a str>,
    /// Grid the brush snaps to: `square`, `iso` or `hex`.
    pub paint_grid: &'a str,
    /// The brush removes tiles instead of placing them.
    pub paint_erase: bool,
    /// Active brush tool id: brush, rectangle, line, fill, picker, eraser, random, pattern, pattern_save.
    pub paint_tool: &'a str,
    /// Layer used for bucket-fill matching in the panel.
    pub paint_layer: i32,
    /// 2D lights in the view's plane; empty draws sprites unlit.
    pub lights2d: &'a [seal_editor_core::light2d::Light],
    /// Solid boxes in the view's plane that cast 2D shadows.
    pub blockers2d: &'a [seal_editor_core::light2d::Blocker],
    /// Light every sprite gets when 2D lights are on.
    pub ambient2d: [f32; 3],
    /// Sprite masks in the view plane. Empty leaves sprites whole.
    pub masks2d: &'a [seal_editor_core::clip2d::Mask],
    /// Banner text while a prefab is open on its own.
    pub prefab_banner: Option<&'a str>,
    /// When true, primary clicks build ruler lines instead of selecting or dragging entities.
    pub measure_mode: bool,
    /// When set, draws sprites in this order instead of layer, order and depth.
    pub sprite_order: Option<&'a [usize]>,
    /// Glow on the flat view: threshold, intensity and radius. Absent leaves sprites as they are.
    pub glow: Option<[f32; 3]>,
    /// Project setting for mesh texture anisotropy, from 1 to 16.
    pub anisotropy: u32,
    /// Render scale in percent for the lit 3D scene (100 is full size).
    pub render_scale: u32,
    /// Cubemap `.texture` drawn behind the perspective view. Empty draws none.
    pub sky_cubemap: &'a str,
    /// Project folder that texture paths are relative to.
    pub texture_root: Option<&'a std::path::Path>,
}

pub struct ViewportBarState<'a> {
    pub space: &'a str,
    pub surface_snap: bool,
    pub grid_snap: bool,
    pub grid_step: f32,
    pub angle_snap: bool,
    pub angle_deg: f32,
    pub scale_snap: bool,
    pub scale_step: f32,
    pub camera_speed: f32,
    pub speed_popup: bool,
}

#[derive(Default)]
pub struct ViewportToolbarHit {
    pub view: Option<String>,
    pub mode: Option<String>,
    pub tool: Option<String>,
    pub maximize: bool,
    pub space_toggle: bool,
    pub surface_toggle: bool,
    pub grid_toggle: bool,
    pub grid_step: Option<f32>,
    pub angle_toggle: bool,
    pub angle_step: Option<f32>,
    pub scale_toggle: bool,
    pub scale_step: Option<f32>,
    pub speed_toggle: bool,
    pub speed: Option<f32>,
    pub show_defaults: bool,
    pub show_layer: Option<(String, bool)>,
    pub show_menu_toggle: bool,
    pub hamburger_toggle: bool,
    pub hamburger_pick: Option<String>,
    pub hamburger_fov: Option<f32>,
    pub hamburger_far: Option<f32>,
    pub hamburger_screen_pct: Option<f32>,
    /// The compact toolbar strip was clicked to expand the full toolbar.
    pub toolbar_expand: bool,
}

pub struct ViewportHamburgerMenu<'a> {
    pub menu_open: bool,
    pub tooltip: &'a str,
    pub realtime: bool,
    pub show_stats: bool,
    pub show_fps: bool,
    pub show_toolbar: bool,
    pub cinematic: bool,
    pub fov: f32,
    pub far_plane: f32,
    pub screen_pct: f32,
    pub lbl_realtime: &'a str,
    pub lbl_show_stats: &'a str,
    pub lbl_show_fps: &'a str,
    pub lbl_show_toolbar: &'a str,
    pub lbl_fov: &'a str,
    pub lbl_far_plane: &'a str,
    pub lbl_screen_pct: &'a str,
    pub lbl_cinematic: &'a str,
    pub lbl_game_view: &'a str,
    pub lbl_immersive: &'a str,
    pub lbl_bookmarks: &'a str,
    pub lbl_bookmark_store_heading: &'a str,
    pub lbl_bookmark_jump_heading: &'a str,
    pub lbl_create_camera: &'a str,
    pub lbl_screenshot: &'a str,
    pub lbl_layouts: &'a str,
    pub lbl_advanced: &'a str,
    pub lbl_isolate: &'a str,
    pub lbl_overlays: &'a str,
    pub lbl_overlay_collapse: &'a str,
    pub lbl_stats_edge: &'a str,
    pub overlay_edges: &'a [(&'a str, &'a str)],
    pub toolbar_collapsed: bool,
    pub bookmark_store: &'a [(&'a str, &'a str)],
    pub bookmark_jump: &'a [(&'a str, &'a str)],
    pub layout_entries: &'a [(&'a str, &'a str, bool)],
}

/// Viewport right-click menus for a picked entity and for empty space.
pub struct GridMenus<'a> {
    pub entity: crate::ViewportContextMenu<'a>,
    pub empty: crate::ViewportContextMenu<'a>,
}

const DEFAULT_ORBIT_DISTANCE: f32 = 10.0;

const PAD: i8 = 8;
const LEVEL_WIDTH: f32 = 44.0;
const TILE: egui::Vec2 = egui::vec2(92.0, 112.0);

/// Shorten `text` with an ellipsis so it fits `width`.
fn elide_to_width(ui: &egui::Ui, text: &str, font: &egui::FontId, width: f32) -> String {
    let fits = |t: &str| {
        ui.painter().layout_no_wrap(t.to_string(), font.clone(), Color32::WHITE).size().x <= width
    };
    if fits(text) {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    for keep in (1..chars.len()).rev() {
        let candidate: String = chars[..keep].iter().collect::<String>() + "…";
        if fits(&candidate) {
            return candidate;
        }
    }
    "…".to_string()
}

/// Strip colour of each asset type on its tile.
pub fn asset_kind_color(kind: &str) -> Color32 {
    match kind {
        "scene" => Color32::from_rgb(0xE0, 0x8A, 0x3C),
        "prefab" => Color32::from_rgb(0x4A, 0x90, 0xD9),
        "picture" => Color32::from_rgb(0x9B, 0x6B, 0xD6),
        "sound" => Color32::from_rgb(0xD9, 0xC9, 0x4A),
        "soundset" => Color32::from_rgb(0xC9, 0xA0, 0x4A),
        "effect" => Color32::from_rgb(0xE0, 0x5A, 0x9B),
        "animation" => Color32::from_rgb(0x4A, 0xC0, 0x8C),
        "ui" => Color32::from_rgb(0x4A, 0xB4, 0xD0),
        "text" => Color32::from_rgb(0x8A, 0xA0, 0xB4),
        "folder" => Color32::from_rgb(0xC4, 0x9A, 0x5C),
        _ => Color32::from_rgb(0x88, 0x88, 0x88),
    }
}

/// A tan folder filling `area`, like a file manager's large icon.
pub fn paint_folder_tile(painter: &egui::Painter, area: egui::Rect) {
    let w = area.width().min(area.height() * 1.3);
    let h = w / 1.3;
    let body = egui::Rect::from_center_size(area.center(), egui::vec2(w, h * 0.82));
    let body = egui::Rect::from_min_max(
        egui::pos2(body.left(), body.top() + h * 0.08),
        egui::pos2(body.right(), body.bottom() + h * 0.08),
    );
    let tab = egui::Rect::from_min_size(
        egui::pos2(body.left(), body.top() - h * 0.12),
        egui::vec2(w * 0.4, h * 0.16),
    );
    let back = Color32::from_rgb(0x9A, 0x74, 0x40);
    let front_top = Color32::from_rgb(0xC9, 0xA0, 0x62);
    let front = Color32::from_rgb(0xB0, 0x86, 0x4C);
    painter.rect_filled(tab, 1.0, back);
    painter.rect_filled(body, 1.0, back);
    let lip = egui::Rect::from_min_max(egui::pos2(body.left(), body.top() + h * 0.14), body.max);
    painter.rect_filled(lip, 1.0, front);
    let shine = egui::Rect::from_min_max(lip.min, egui::pos2(lip.right(), lip.top() + 3.0));
    painter.rect_filled(shine, 0.0, front_top);
    painter.rect_stroke(
        lip,
        1.0,
        Stroke::new(1.0, Color32::from_rgb(0x6E, 0x52, 0x2C)),
        egui::StrokeKind::Inside,
    );
}

/// A dark asset card with a strip in the asset type's colour; `glyph` also draws
/// the type's icon in the middle.
fn paint_asset_card(
    painter: &egui::Painter,
    area: egui::Rect,
    kind: &str,
    visuals: &egui::Visuals,
    glyph: bool,
) {
    let card = egui::Rect::from_center_size(
        area.center(),
        egui::vec2(area.width().min(area.height()), area.height()),
    );
    painter.rect_filled(card, 1.0, visuals.extreme_bg_color);
    painter.rect_stroke(
        card,
        1.0,
        Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color),
        egui::StrokeKind::Inside,
    );
    let color = asset_kind_color(kind);
    let strip = egui::Rect::from_min_max(egui::pos2(card.left(), card.bottom() - 3.0), card.max);
    painter.rect_filled(strip, 0.0, color);
    if glyph {
        let icon = match kind {
            "scene" => crate::icons::Icon::ToolWorld,
            "prefab" => crate::icons::Icon::ViewTiles,
            "sound" => crate::icons::Icon::Play,
            "soundset" => crate::icons::Icon::AssetSound,
            "effect" => crate::icons::Icon::Simulate,
            "animation" => crate::icons::Icon::AssetAnimation,
            "text" => crate::icons::Icon::FileNew,
            _ => crate::icons::Icon::File,
        };
        crate::icons::paint_icon(painter, card.center() - egui::vec2(0.0, 2.0), icon, 26.0, color);
    }
}

pub struct LogLineView<'a> {
    pub level: &'a str,
    pub message: &'a str,
    pub multiline: bool,
    pub folded: bool,
}

pub enum LogListHit {
    None,
    Select(usize),
    SelectExtend(usize),
    Menu(usize, usize),
    ToggleFold(usize),
}

/// Search field at the top of a context menu. The returned indexes are into `labels`.
pub(crate) fn filtered_menu(ui: &mut egui::Ui, labels: &[String]) -> Vec<usize> {
    ui.set_min_width(180.0);
    let store = ui.id().with("seal-ctx-search");
    let mut query = ui.data(|data| data.get_temp::<String>(store).unwrap_or_default());
    let hint = ui.ctx().data(|data| {
        data.get_temp::<String>(egui::Id::new("seal-menu-search-hint")).unwrap_or_default()
    });
    let field = crate::crisp::well_edit(
        ui,
        egui::TextEdit::singleline(&mut query)
            .hint_text(hint)
            .desired_width(ui.available_width().max(160.0)),
    );
    let armed = ui.id().with("seal-ctx-search-armed");
    let focused = ui.data(|data| data.get_temp::<bool>(armed).unwrap_or(false));
    if !focused {
        field.request_focus();
        ui.data_mut(|data| data.insert_temp(armed, true));
    }
    ui.data_mut(|data| data.insert_temp(store, query.clone()));
    let needle = query.trim().to_lowercase();
    let shown: Vec<usize> = if needle.is_empty() {
        (0..labels.len()).collect()
    } else {
        labels
            .iter()
            .enumerate()
            .filter(|(_, label)| label.to_lowercase().contains(&needle))
            .map(|(index, _)| index)
            .collect()
    };
    if shown.is_empty() {
        let miss = ui.ctx().data(|data| {
            data.get_temp::<String>(egui::Id::new("seal-menu-miss"))
                .unwrap_or_else(|| "menu-no-matches".to_string())
        });
        ui.label(egui::RichText::new(miss).weak());
    }
    shown
}

#[allow(clippy::too_many_arguments)]
impl<'a> PanelFrame<'a> {
    /// Size of the visible remainder of this panel body, as `grid_view` fills it.
    pub fn view_size(&self) -> [f32; 2] {
        let clip = self.ui.clip_rect();
        let max = self.ui.max_rect();
        let cursor = self.ui.cursor().min;
        [
            (clip.right().min(max.right()) - cursor.x).max(32.0),
            (clip.bottom().min(max.bottom()) - cursor.y).max(32.0),
        ]
    }

    pub fn new(ui: &'a mut egui::Ui) -> Self {
        Self { ui }
    }

    pub fn ui_mut(&mut self) -> &mut egui::Ui {
        self.ui
    }

    /// Body with the standard panel margin.
    pub fn padded(&mut self, body: impl FnOnce(&mut PanelFrame)) {
        egui::Frame::new()
            .inner_margin(egui::Margin::same(PAD))
            .show(self.ui, |ui| body(&mut PanelFrame::new(ui)));
    }

    /// 22 px status strip at the bottom of a panel.
    pub fn footer(&mut self, body: impl FnOnce(&mut PanelFrame)) {
        let fill = self.ui.visuals().extreme_bg_color;
        let line = self.ui.visuals().widgets.noninteractive.bg_stroke;
        let response = egui::Frame::new()
            .fill(fill)
            .inner_margin(egui::Margin::symmetric(8, 0))
            .show(self.ui, |ui| {
                ui.set_min_height(crate::ROW_PX);
                ui.set_max_height(crate::ROW_PX);
                ui.horizontal(|ui| {
                    ui.set_min_height(crate::ROW_PX);
                    body(&mut PanelFrame::new(ui));
                });
            })
            .response;
        self.ui.painter().hline(response.rect.x_range(), response.rect.top(), line);
    }

    /// Strip of controls at the top of a panel, with a line under it.
    pub fn bar(&mut self, body: impl FnOnce(&mut PanelFrame)) {
        let fill = self.ui.visuals().extreme_bg_color;
        let line = self.ui.visuals().widgets.noninteractive.bg_stroke;
        let response = egui::Frame::new()
            .fill(fill)
            .inner_margin(egui::Margin::symmetric(6, 2))
            .show(self.ui, |ui| {
                let width = ui.available_width();
                ui.set_width(width);
                ui.set_max_width(width);
                // Wrap onto the next line instead of drawing past the panel edge.
                ui.horizontal_wrapped(|ui| body(&mut PanelFrame::new(ui)));
            })
            .response;
        self.ui.painter().hline(response.rect.x_range(), response.rect.bottom(), line);
    }

    pub fn row(&mut self, body: impl FnOnce(&mut PanelFrame)) {
        self.ui.horizontal(|ui| body(&mut PanelFrame::new(ui)));
    }

    /// Vertical scroll area that fills the rest of the panel.
    pub fn scroll(&mut self, body: impl FnOnce(&mut PanelFrame)) {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(self.ui, |ui| body(&mut PanelFrame::new(ui)));
    }

    pub fn space(&mut self, amount: f32) {
        self.ui.add_space(amount);
    }

    pub fn logo(&mut self, size: f32) {
        crate::logo::show_logo(self.ui, size);
    }

    pub fn title(&mut self, text: &str) {
        self.ui.label(egui::RichText::new(text).strong());
    }

    pub fn label(&mut self, text: &str) {
        self.ui.label(text);
    }

    pub fn muted(&mut self, text: &str) {
        self.ui.weak(text);
    }

    /// Centered note for a panel that has nothing to show.
    pub fn empty(&mut self, text: &str) {
        let weak = egui::RichText::new(text).weak();
        self.ui.centered_and_justified(|ui| ui.label(weak));
    }

    /// Centered empty state with a hint line and action buttons; returns the
    /// index of the clicked action.
    pub fn empty_with_actions(
        &mut self,
        text: &str,
        hint: &str,
        actions: &[&str],
    ) -> Option<usize> {
        let mut picked = None;
        let height = self.ui.available_height();
        self.ui.vertical_centered(|ui| {
            ui.add_space((height * 0.5 - 48.0).max(8.0));
            ui.label(egui::RichText::new(text).strong());
            ui.add_space(4.0);
            ui.label(egui::RichText::new(hint).weak());
            ui.add_space(10.0);
            for (index, action) in actions.iter().enumerate() {
                if crate::a11y::named_button(ui, action).clicked() {
                    picked = Some(index);
                }
            }
        });
        picked
    }

    pub fn button(&mut self, label: &str) -> bool {
        crate::a11y::named_button(self.ui, label).clicked()
    }

    pub fn link(&mut self, label: &str) -> bool {
        let response = self.ui.link(label);
        crate::a11y::named_control(self.ui, response, egui::WidgetType::Link, label).clicked()
    }

    /// Button that stays lit while `on` is true.
    pub fn toggle(&mut self, label: &str, on: bool) -> bool {
        self.chip(label, on)
    }

    /// A full-width row with a picture preview and its name; lit while `on`.
    pub fn picture_toggle(&mut self, label: &str, picture: &str, on: bool) -> bool {
        let width = self.ui.available_width().max(60.0);
        let (rect, response) =
            self.ui.allocate_exact_size(egui::vec2(width, 40.0), egui::Sense::click());
        let visuals = self.ui.visuals().clone();
        let painter = self.ui.painter().with_clip_rect(rect);
        let fill = if on {
            visuals.selection.bg_fill
        } else if response.hovered() {
            visuals.widgets.hovered.weak_bg_fill
        } else {
            visuals.widgets.inactive.weak_bg_fill
        };
        painter.rect_filled(rect, 0.0, fill);
        let well =
            egui::Rect::from_min_size(rect.min + egui::vec2(4.0, 4.0), egui::vec2(32.0, 32.0));
        painter.rect_filled(well, 0.0, visuals.extreme_bg_color);
        if let Some((texture, size)) = crate::sprites::thumbnail(self.ui.ctx(), picture) {
            crate::sprites::paint_fitted(&painter, texture, size, well.shrink(2.0));
        }
        painter.text(
            egui::pos2(well.right() + 8.0, rect.center().y),
            Align2::LEFT_CENTER,
            label,
            TextStyle::Body.resolve(self.ui.style()),
            if on { visuals.selection.stroke.color } else { visuals.text_color() },
        );
        crate::a11y::named_control(self.ui, response, egui::WidgetType::Button, label).clicked()
    }

    /// True on the frame `key` was pressed.
    pub fn key_pressed(&self, key: egui::Key) -> bool {
        self.ui.input(|input| input.key_pressed(key))
    }

    /// Hover text for the rest of a toolbar row.
    pub fn hint(&mut self, text: &str) {
        let width = self.ui.available_width().max(1.0);
        let response = self.ui.allocate_response(egui::vec2(width, 1.0), egui::Sense::hover());
        response.on_hover_text(text);
    }

    /// Thin divider between groups on a toolbar.
    pub fn rule(&mut self) {
        let height = self.ui.spacing().interact_size.y;
        let (rect, _) = self.ui.allocate_exact_size(egui::vec2(9.0, height), egui::Sense::hover());
        let x = rect.center().x;
        let color = self.ui.visuals().widgets.noninteractive.bg_stroke.color;
        self.ui.painter().vline(x, rect.y_range().shrink(3.0), egui::Stroke::new(1.0, color));
    }

    /// Compact toolbar chip. Selected chips use the accent as an edge, not a filled pill.
    pub fn chip(&mut self, label: &str, on: bool) -> bool {
        let ui = &mut *self.ui;
        let font = egui::FontId::proportional(12.0);
        let galley = ui.painter().layout_no_wrap(label.to_string(), font, Color32::WHITE);
        let size = egui::vec2(galley.size().x + 20.0, 22.0);
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        let rect = crate::crisp::snap(rect);
        let visuals = ui.visuals().clone();
        let state = if response.is_pointer_button_down_on() {
            crate::crisp::Raised::Pressed
        } else if on {
            crate::crisp::Raised::Active
        } else if response.hovered() {
            crate::crisp::Raised::Hover
        } else {
            crate::crisp::Raised::Normal
        };
        let mut painter = ui.painter().clone();
        let p = &mut painter;
        crate::crisp::raised(&mut &*p, rect, &visuals, state);
        let slot = egui::Rect::from_min_max(
            egui::pos2(rect.left() + 6.0, rect.top() + 3.0),
            egui::pos2(rect.right() - 6.0, rect.top() + 6.0),
        );
        crate::crisp::led_slot(&mut &*p, slot, on, visuals.selection.stroke.color);
        let ink = if on { Color32::WHITE } else { visuals.text_color() };
        let pos = egui::pos2(
            (rect.center().x - galley.size().x / 2.0).round(),
            (rect.center().y - galley.size().y / 2.0 + 1.5).round(),
        );
        p.galley_with_override_text_color(pos, galley, ink);
        paint_focus_ring(ui, &response);
        crate::a11y::named_control(ui, response, egui::WidgetType::Button, label).clicked()
    }

    /// Search field that takes the rest of the row.
    pub fn search(&mut self, text: &mut String, hint: &str) -> bool {
        let field = crate::crisp::well_edit(
            self.ui,
            egui::TextEdit::singleline(text).hint_text(hint).desired_width(f32::INFINITY),
        );
        let response = help_focus_ring(self.ui, field, hint);
        response.changed()
    }

    /// Tool-row search with magnifier and a clear control when `text` is non-empty.
    pub fn panel_search(&mut self, text: &mut String, hint: &str, clear: &str) -> bool {
        self.panel_search_leaving(text, hint, clear, 0.0)
    }

    /// Like [`Self::panel_search`], but leaves `leave` px on the right for controls that
    /// follow on the same row, so they stay beside the field instead of wrapping below.
    pub fn panel_search_leaving(
        &mut self,
        text: &mut String,
        hint: &str,
        clear: &str,
        leave: f32,
    ) -> bool {
        let mut changed = false;
        let row_h = crate::TOOL_ROW_PX - 4.0;
        let side = row_h - 2.0;
        self.ui.horizontal(|ui| {
            ui.set_min_height(row_h);
            let (mag_rect, _) =
                ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
            let ink = ui.visuals().weak_text_color();
            crate::icons::paint_icon(
                ui.painter(),
                mag_rect.center(),
                crate::icons::Icon::EditSearch,
                crate::ICON_PX,
                ink,
            );
            let gap = ui.spacing().item_spacing.x;
            let clear_room = if text.is_empty() { 0.0 } else { side + gap };
            let width = (ui.available_width() - leave - clear_room).max(48.0);
            let field = crate::crisp::well_edit(
                ui,
                egui::TextEdit::singleline(text).hint_text(hint).desired_width(width),
            );
            let field = help_focus_ring(ui, field, hint);
            changed |= field.changed();
            if !text.is_empty() {
                let (clear_rect, clear_resp) =
                    ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::click());
                crate::icons::paint_icon(
                    ui.painter(),
                    clear_rect.center(),
                    crate::icons::Icon::ViewClose,
                    12.0,
                    ink,
                );
                let clear_resp =
                    crate::a11y::named_control(ui, clear_resp, egui::WidgetType::Button, clear);
                if clear_resp.clicked() {
                    text.clear();
                    changed = true;
                }
            }
        });
        changed
    }

    /// Name field that reports each edit and when Enter commits it.
    pub fn draft_field(&mut self, label: &str, text: &mut String) -> (bool, bool) {
        let mut changed = false;
        let mut committed = false;
        self.ui.horizontal(|ui| {
            let name = property_name(ui, label, false, None);
            let response = crate::crisp::value_well(
                ui,
                egui::TextEdit::singleline(text).id_salt(label).desired_width(f32::INFINITY),
                6,
            )
            .labelled_by(name)
            .on_hover_text(label);
            paint_focus_ring(ui, &response);
            changed = response.changed();
            committed =
                response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            paint_row_rule(ui);
        });
        (changed, committed)
    }

    pub fn text_edit(&mut self, text: &mut String) -> bool {
        let response = crate::crisp::well_edit(
            self.ui,
            egui::TextEdit::singleline(text).desired_width(f32::INFINITY),
        );
        paint_focus_ring(self.ui, &response);
        response.changed()
    }

    /// Collapsible group with a filled header.
    pub fn section(&mut self, title: &str, body: impl FnOnce(&mut PanelFrame)) {
        let _ = self.section_keyed(title, title, true, body);
    }

    /// Collapsible group whose open state is owned by the caller.
    pub fn section_keyed(
        &mut self,
        id: &str,
        title: &str,
        open: bool,
        body: impl FnOnce(&mut PanelFrame),
    ) -> Option<bool> {
        self.section_keyed_with_highlight(id, title, open, body, None)
    }

    /// Like [`section_keyed`] but paints a match highlight on the header when the query hits.
    pub fn section_keyed_with_highlight(
        &mut self,
        id: &str,
        title: &str,
        open: bool,
        body: impl FnOnce(&mut PanelFrame),
        highlight_query: Option<&str>,
    ) -> Option<bool> {
        let mut header = egui::RichText::new(title).strong().size(crate::SECTION_PX);
        if let Some(q) = highlight_query {
            let ql = q.to_lowercase();
            if !ql.is_empty()
                && (title.to_lowercase().contains(&ql) || id.to_lowercase().contains(&ql))
            {
                header = header.background_color(self.ui.visuals().selection.bg_fill);
            }
        }
        let _ = header;
        let ui = &mut *self.ui;
        let width = ui.available_width();
        let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 22.0), Sense::click());
        let visuals = ui.visuals().clone();
        let state = if response.is_pointer_button_down_on() {
            crate::crisp::Raised::Pressed
        } else if response.hovered() {
            crate::crisp::Raised::Hover
        } else {
            crate::crisp::Raised::Normal
        };
        let painter = ui.painter().clone();
        crate::crisp::raised(&mut &painter, rect, &visuals, state);
        crate::crisp::arrow(
            &mut &painter,
            egui::pos2(rect.left() + 11.0, rect.center().y),
            open,
            visuals.strong_text_color(),
        );
        let hit = highlight_query.is_some_and(|q| {
            let ql = q.to_lowercase();
            !ql.is_empty()
                && (title.to_lowercase().contains(&ql) || id.to_lowercase().contains(&ql))
        });
        let text_pos = egui::pos2(rect.left() + 22.0, rect.center().y);
        let galley = painter.layout_no_wrap(
            title.to_string(),
            egui::FontId::proportional(crate::SECTION_PX),
            visuals.strong_text_color(),
        );
        if hit {
            let r = egui::Rect::from_min_size(
                text_pos - egui::vec2(2.0, galley.size().y / 2.0),
                galley.size() + egui::vec2(4.0, 0.0),
            );
            painter.rect_filled(r, 0.0, visuals.selection.bg_fill);
        }
        painter.galley(
            text_pos - egui::vec2(0.0, galley.size().y / 2.0),
            galley,
            visuals.strong_text_color(),
        );
        let response = response.on_hover_text(title);
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::CollapsingHeader, true, title)
        });
        paint_focus_ring(ui, &response);
        let toggled = response.clicked();
        if open {
            ui.add_space(2.0);
            ui.push_id(id, |ui| body(&mut PanelFrame::new(ui)));
            ui.add_space(4.0);
        }
        if toggled {
            Some(!open)
        } else {
            None
        }
    }

    /// Enables per-row label highlighting for property editors below.
    pub fn begin_search_highlight(&mut self, query: &str) {
        let id = self.ui.id().with("seal_property_search_highlight");
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            self.ui.data_mut(|data| data.remove::<String>(id));
        } else {
            self.ui.data_mut(|data| data.insert_temp(id, q));
        }
    }

    /// Name on the left, editable value on the right.
    pub fn property(&mut self, label: &str, text: &mut String) -> bool {
        let mut changed = false;
        self.ui.horizontal(|ui| {
            let name = property_name(ui, label, false, None);
            let response = crate::crisp::well_edit(
                ui,
                egui::TextEdit::singleline(text).id_salt(label).desired_width(f32::INFINITY),
            )
            .labelled_by(name)
            .on_hover_text(label);
            paint_focus_ring(ui, &response);
            changed = response.changed();
            paint_row_rule(ui);
        });
        changed
    }

    /// Name on the left, a drop-down of `(id, title)` on the right. Returns the picked id.
    pub fn choice(
        &mut self,
        label: &str,
        current: &str,
        options: &[(&str, String)],
    ) -> Option<String> {
        let mut picked = None;
        self.ui.horizontal(|ui| {
            let name = property_name(ui, label, false, None);
            let shown = options
                .iter()
                .find(|(id, _)| *id == current)
                .map_or(current, |(_, title)| title.as_str());
            let response = egui::ComboBox::from_id_salt(label)
                .selected_text(shown)
                .width(ui.available_width())
                .show_crisp(ui, |ui| {
                    for (id, title) in options {
                        if ui.selectable_label(*id == current, title).clicked() {
                            picked = Some((*id).to_string());
                        }
                    }
                })
                .response
                .labelled_by(name)
                .on_hover_text(label);
            paint_focus_ring(ui, &response);
            paint_row_rule(ui);
        });
        picked
    }

    pub fn tree_list(
        &mut self,
        count: usize,
        initial_scroll_y: Option<f32>,
        mut row: impl FnMut(&mut PanelFrame, usize) -> TreeClick,
    ) -> TreeClick {
        let height = self.ui.spacing().interact_size.y;
        let mut click = TreeClick::None;
        self.ui.spacing_mut().item_spacing.y = 0.0;
        // Leave room for the panel footer so the count strip is never pushed off the bottom.
        let room = (self.ui.available_height() - crate::ROW_PX - 1.0).max(height);
        let mut area = egui::ScrollArea::vertical().auto_shrink([false, false]).max_height(room);
        if let Some(offset) = initial_scroll_y {
            area = area.vertical_scroll_offset(offset);
        }
        area.show_rows(self.ui, height, count, |ui, range| {
            for index in range {
                let hit = row(&mut PanelFrame::new(ui), index);
                if !matches!(hit, TreeClick::None) {
                    click = hit;
                }
            }
        });
        click
    }

    /// One outliner row. The disclosure mark and the name are separate clicks.
    pub fn tree_row(&mut self, row: TreeRow<'_>) -> TreeClick {
        let TreeRow {
            depth,
            has_children,
            open,
            label,
            detail,
            selected,
            hidden,
            locked,
            menu,
            drag_id,
            place_kind,
            rename_text,
            component_row: _,
            thumb,
            marks,
        } = row;
        let height = self.ui.spacing().interact_size.y;
        let (rect, response) = self.ui.allocate_exact_size(
            egui::vec2(self.ui.available_width(), height),
            Sense::click_and_drag(),
        );
        let response = help_focus_ring(self.ui, response, label);
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, label)
        });
        let visuals = self.ui.visuals().clone();
        if let Some(kind) = place_kind {
            if response.dragged() {
                response.dnd_set_drag_payload(format!("place:{kind}"));
            }
        } else if let Some(id) = drag_id {
            response.dnd_set_drag_payload(id.to_string());
            if response.dnd_hover_payload::<String>().is_some() {
                let stroke = Stroke::new(1.0, visuals.selection.bg_fill);
                self.ui.painter().rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Inside);
            }
            if let Some(child) = response.dnd_release_payload::<String>() {
                if child.as_str() != id {
                    return TreeClick::Reparent {
                        child: child.to_string(),
                        parent: id.to_string(),
                    };
                }
            }
        }
        let ink = row_fill(self.ui, rect, &response, selected, true);
        let mut click = TreeClick::None;
        let mark = egui::Rect::from_center_size(
            egui::pos2(rect.left() + f32::from(PAD) + depth as f32 * 14.0 + 6.0, rect.center().y),
            egui::vec2(12.0, 12.0),
        );
        if has_children {
            let toggle =
                self.ui.interact(mark.expand(3.0), response.id.with("mark"), Sense::click());
            let openness = if open { 1.0 } else { 0.0 };
            let clicked = toggle.clicked();
            egui::collapsing_header::paint_default_icon(
                self.ui,
                openness,
                &toggle.with_new_rect(mark),
            );
            if clicked {
                click = TreeClick::Toggle;
            }
        }
        if let Some(text) = rename_text {
            let width = (rect.right() - mark.right() - 70.0).max(40.0);
            let edit_id = egui::Id::new(("outliner-rename", drag_id.unwrap_or(label)));
            let selected_all = begin_rename_edit(self.ui.ctx(), edit_id, text);
            let edit = egui::TextEdit::singleline(text).id(edit_id).desired_width(width);
            let edit_resp = crate::crisp::put_well(
                self.ui,
                egui::Rect::from_center_size(
                    egui::pos2(mark.right() + 6.0 + width * 0.5, rect.center().y),
                    egui::vec2(width, height),
                ),
                edit,
            );
            if selected_all {
                paint_rename_selection(self.ui, edit_resp.rect, text, TextStyle::Body, false);
            }
            if self.ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                return TreeClick::RenameCancel;
            }
            if edit_resp.lost_focus() && !self.ui.input(|input| input.key_pressed(egui::Key::Enter))
            {
                // Clicking elsewhere keeps the typed name, like pressing Enter.
                return TreeClick::Rename(text.to_string());
            }
            if self.ui.input(|input| input.key_pressed(egui::Key::Enter))
                && (edit_resp.has_focus() || edit_resp.lost_focus())
            {
                return TreeClick::Rename(text.to_string());
            }
        } else {
            let mut text_x = mark.right() + 6.0;
            if let Some((texture, size)) =
                thumb.and_then(|path| crate::sprites::thumbnail(self.ui.ctx(), path))
            {
                let area = egui::Rect::from_min_size(
                    egui::pos2(text_x, rect.center().y - 8.0),
                    egui::vec2(16.0, 16.0),
                );
                crate::sprites::paint_fitted(
                    &self.ui.painter().with_clip_rect(rect),
                    texture,
                    size,
                    area,
                );
                text_x += 22.0;
            }
            self.ui.painter().with_clip_rect(rect).text(
                egui::pos2(text_x, rect.center().y),
                Align2::LEFT_CENTER,
                label,
                TextStyle::Body.resolve(self.ui.style()),
                ink.unwrap_or(visuals.text_color()),
            );
        }
        if marks {
            let eye = egui::Rect::from_center_size(
                egui::pos2(rect.right() - 36.0, rect.center().y),
                egui::vec2(14.0, 14.0),
            );
            let lock = egui::Rect::from_center_size(
                egui::pos2(rect.right() - 16.0, rect.center().y),
                egui::vec2(14.0, 14.0),
            );
            let eye_hit = self.ui.interact(eye, response.id.with("eye"), Sense::click());
            let lock_hit = self.ui.interact(lock, response.id.with("lock"), Sense::click());
            let mark_ink = ink.unwrap_or(visuals.weak_text_color());
            crate::icons::paint_icon(
                self.ui.painter(),
                eye.center(),
                if hidden { crate::icons::Icon::ViewEyeOff } else { crate::icons::Icon::ViewEye },
                14.0,
                if hidden { mark_ink.gamma_multiply(0.4) } else { mark_ink },
            );
            crate::icons::paint_icon(
                self.ui.painter(),
                lock.center(),
                if locked { crate::icons::Icon::EditLock } else { crate::icons::Icon::EditUnlock },
                14.0,
                if locked { visuals.hyperlink_color } else { mark_ink },
            );
            if eye_hit.clicked() {
                click = TreeClick::Eye;
            } else if lock_hit.clicked() {
                click = TreeClick::Lock;
            }
        }
        self.ui.painter().with_clip_rect(rect).text(
            egui::pos2(rect.right() - if marks { 52.0 } else { 10.0 }, rect.center().y),
            Align2::RIGHT_CENTER,
            detail,
            TextStyle::Small.resolve(self.ui.style()),
            ink.unwrap_or(visuals.weak_text_color()),
        );
        if !menu.is_empty() {
            response.context_menu(|ui| {
                for index in filtered_menu(ui, menu) {
                    let entry = &menu[index];
                    let item = ui.button(entry).on_hover_text(entry.as_str());
                    paint_focus_ring(ui, &item);
                    if item.clicked() {
                        click = TreeClick::Menu(index);
                        ui.close();
                    }
                }
            });
        }
        if matches!(click, TreeClick::None) {
            if response.double_clicked() && has_children {
                click = TreeClick::Toggle;
            } else if response.double_clicked() {
                click = TreeClick::FrameEntity;
            } else if response.clicked() {
                let (shift, ctrl) =
                    self.ui.input(|input| (input.modifiers.shift, input.modifiers.ctrl));
                click = if shift && !ctrl {
                    TreeClick::SelectRange
                } else if ctrl {
                    TreeClick::SelectExtend
                } else {
                    TreeClick::Select
                };
            }
        }
        click
    }

    pub fn log_list(
        &mut self,
        lines: &[LogLineView<'_>],
        selected: &[usize],
        focus: usize,
        menu: &[String],
        stick_to_bottom: bool,
    ) -> LogListHit {
        let mut hit = LogListHit::None;
        let font = TextStyle::Monospace.resolve(self.ui.style());
        let height = self.ui.fonts_mut(|fonts| fonts.row_height(&font)) + 4.0;
        self.ui.spacing_mut().item_spacing.y = 0.0;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(stick_to_bottom)
            .show_rows(self.ui, height, lines.len(), |ui, range| {
                let visuals = ui.visuals().clone();
                for index in range {
                    let line = &lines[index];
                    let shown = if line.multiline && line.folded {
                        line.message.lines().next().unwrap_or(line.message)
                    } else {
                        line.message
                    };
                    let (rect, response) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), height),
                        Sense::click(),
                    );
                    response.context_menu(|ui| {
                        for action in filtered_menu(ui, menu) {
                            let entry = &menu[action];
                            let item = ui.button(entry).on_hover_text(entry.as_str());
                            paint_focus_ring(ui, &item);
                            if item.clicked() {
                                hit = LogListHit::Menu(index, action);
                                ui.close();
                            }
                        }
                    });
                    let painter = ui.painter().with_clip_rect(rect);
                    if selected.contains(&index) || index == focus {
                        painter.rect_filled(
                            rect,
                            0.0,
                            visuals.selection.bg_fill.gamma_multiply(0.35),
                        );
                    } else if index % 2 == 1 {
                        painter.rect_filled(rect, 0.0, visuals.faint_bg_color);
                    }
                    let (tag, ink) = match line.level {
                        "error" => (visuals.error_fg_color, visuals.error_fg_color),
                        "warn" => (visuals.warn_fg_color, visuals.warn_fg_color),
                        "info" => (visuals.hyperlink_color, visuals.text_color()),
                        _ => (visuals.weak_text_color(), visuals.weak_text_color()),
                    };
                    let left = rect.left() + f32::from(PAD);
                    painter.text(
                        egui::pos2(left, rect.center().y),
                        Align2::LEFT_CENTER,
                        line.level.to_uppercase(),
                        font.clone(),
                        tag,
                    );
                    let mut text_left = left + LEVEL_WIDTH;
                    if line.multiline {
                        let fold = egui::Rect::from_center_size(
                            egui::pos2(text_left + 4.0, rect.center().y),
                            egui::vec2(12.0, 12.0),
                        );
                        let fold_hit = ui.interact(fold, response.id.with("fold"), Sense::click());
                        painter.text(
                            egui::pos2(fold.center().x, rect.center().y),
                            Align2::CENTER_CENTER,
                            if line.folded { ">" } else { "v" },
                            font.clone(),
                            visuals.weak_text_color(),
                        );
                        if fold_hit.clicked() {
                            hit = LogListHit::ToggleFold(index);
                        }
                        text_left += 14.0;
                    }
                    painter.text(
                        egui::pos2(text_left, rect.center().y),
                        Align2::LEFT_CENTER,
                        shown,
                        font.clone(),
                        ink,
                    );
                    if matches!(hit, LogListHit::None) && response.clicked() {
                        let (shift, ctrl) =
                            ui.input(|input| (input.modifiers.shift, input.modifiers.ctrl));
                        hit = if shift || ctrl {
                            LogListHit::SelectExtend(index)
                        } else {
                            LogListHit::Select(index)
                        };
                    }
                }
            });
        hit
    }

    /// Asset-browser grid: click selects, double-click or Enter opens, right-click
    /// selects and shows `menu(item)`, right-click on empty space shows `background`,
    /// F2 renames, Delete deletes, Backspace goes up.
    /// While `renaming` names an item, that tile shows an inline name editor.
    #[allow(clippy::too_many_arguments)]
    pub fn file_browser(
        &mut self,
        items: &[FileTile],
        selected: Option<usize>,
        menu: &dyn Fn(usize) -> Vec<String>,
        background: &[String],
        renaming: Option<(usize, &mut String)>,
    ) -> Option<FileHit> {
        let mut hit = None;
        let visuals = self.ui.visuals().clone();
        let folder_ink = visuals.warn_fg_color;
        let file_ink = visuals.weak_text_color();
        let (rename_index, mut rename_text) = match renaming {
            Some((index, text)) => (Some(index), Some(text)),
            None => (None, None),
        };
        let area = egui::ScrollArea::vertical().auto_shrink([false, false]).show(self.ui, |ui| {
            // Registered before the tiles so a tile under the pointer wins its clicks.
            let empty = ui.interact(
                ui.max_rect().union(ui.clip_rect()),
                ui.id().with("file-browser-empty"),
                Sense::click(),
            );
            egui::Frame::new().inner_margin(egui::Margin::same(PAD)).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
                    for (index, tile) in items.iter().enumerate() {
                        let name = &tile.label;
                        let folder = &tile.folder;
                        let (rect, response) =
                            ui.allocate_exact_size(TILE, Sense::click_and_drag());
                        if !folder && response.dragged() {
                            if let Some(kind) = content_drag_kind(name) {
                                let payload = if tile.drag_path.is_empty() {
                                    format!("place:{kind}")
                                } else {
                                    seal_editor_core::asset_drag_payload(&kind, &tile.drag_path)
                                };
                                response.dnd_set_drag_payload(payload);
                            } else if !tile.drag_path.is_empty() {
                                response.dnd_set_drag_payload(format!("asset:{}", tile.drag_path));
                            }
                        }
                        let is_selected = selected == Some(index);
                        if response.double_clicked() {
                            hit = Some(FileHit::Open(index));
                        } else if (response.clicked() || response.secondary_clicked())
                            && !is_selected
                        {
                            hit = Some(FileHit::Select(index));
                        }
                        let response = response.on_hover_text(name.as_str());
                        response.context_menu(|ui| {
                            let entries = menu(index);
                            for action in filtered_menu(ui, &entries) {
                                let entry = &entries[action];
                                let item = ui.button(entry).on_hover_text(entry.as_str());
                                paint_focus_ring(ui, &item);
                                if item.clicked() {
                                    hit = Some(FileHit::Menu(index, action));
                                    ui.close();
                                }
                            }
                        });
                        response.widget_info(|| {
                            egui::WidgetInfo::selected(
                                egui::WidgetType::SelectableLabel,
                                true,
                                is_selected,
                                name,
                            )
                        });
                        row_fill(ui, rect, &response, is_selected, true);
                        if is_selected {
                            ui.painter().rect_stroke(
                                rect.shrink(0.5),
                                0.0,
                                Stroke::new(1.0, visuals.selection.stroke.color),
                                egui::StrokeKind::Inside,
                            );
                        }
                        let painter = ui.painter().with_clip_rect(rect.shrink(2.0));
                        let art = egui::Rect::from_min_max(
                            egui::pos2(rect.left() + 6.0, rect.top() + 6.0),
                            egui::pos2(rect.right() - 6.0, rect.bottom() - 34.0),
                        );
                        let thumb = tile
                            .thumb_disk
                            .as_ref()
                            .and_then(|path| {
                                crate::sprites::thumbnail(ui.ctx(), &path.to_string_lossy())
                            })
                            .or_else(|| {
                                (!*folder && crate::sprites::is_image_name(name))
                                    .then_some(tile.path.as_ref())
                                    .flatten()
                                    .and_then(|path| {
                                        crate::sprites::thumbnail(ui.ctx(), &path.to_string_lossy())
                                    })
                            });
                        match (folder, thumb) {
                            (true, _) => paint_folder_tile(&painter, art),
                            (false, Some((texture, size))) => {
                                paint_asset_card(&painter, art, tile.kind, &visuals, false);
                                crate::sprites::paint_fitted(
                                    &painter,
                                    texture,
                                    size,
                                    art.shrink(5.0).with_max_y(art.bottom() - 6.0),
                                );
                            }
                            (false, None) => {
                                paint_asset_card(&painter, art, tile.kind, &visuals, true);
                                let _ = (folder_ink, file_ink);
                            }
                        }
                        if rename_index == Some(index) {
                            if let Some(text) = rename_text.as_deref_mut() {
                                let edit_rect = egui::Rect::from_min_max(
                                    egui::pos2(rect.left() + 2.0, rect.bottom() - 20.0),
                                    egui::pos2(rect.right() - 2.0, rect.bottom() - 2.0),
                                );
                                let edit_id = ui.id().with(("file-rename", index));
                                let selected_all = begin_rename_edit(ui.ctx(), edit_id, text);
                                let edit = crate::crisp::put_well(
                                    ui,
                                    edit_rect,
                                    egui::TextEdit::singleline(text)
                                        .id(edit_id)
                                        .font(TextStyle::Small)
                                        .horizontal_align(egui::Align::Center),
                                );
                                if selected_all {
                                    paint_rename_selection(
                                        ui,
                                        edit.rect,
                                        text,
                                        TextStyle::Small,
                                        true,
                                    );
                                }
                                if edit.changed() {
                                    hit = Some(FileHit::RenameEdit(text.clone()));
                                }
                                if edit.lost_focus() {
                                    let cancel = ui.input(|i| i.key_pressed(egui::Key::Escape));
                                    hit = Some(if cancel {
                                        FileHit::RenameCancel
                                    } else {
                                        FileHit::RenameCommit(text.clone())
                                    });
                                }
                                continue;
                            }
                        }
                        let short = name.rsplit('/').next().unwrap_or(name);
                        let font = TextStyle::Small.resolve(ui.style());
                        let shown = elide_to_width(ui, short, &font, rect.width() - 8.0);
                        painter.text(
                            egui::pos2(rect.center().x, rect.bottom() - 19.0),
                            Align2::CENTER_BOTTOM,
                            shown,
                            font.clone(),
                            visuals.text_color(),
                        );
                        if !*folder {
                            painter.text(
                                egui::pos2(rect.center().x, rect.bottom() - 5.0),
                                Align2::CENTER_BOTTOM,
                                &tile.kind_label,
                                font,
                                visuals.weak_text_color(),
                            );
                        }
                    }
                });
            });
            if !background.is_empty() {
                empty.context_menu(|ui| {
                    for action in filtered_menu(ui, background) {
                        let entry = &background[action];
                        let item = ui.button(entry).on_hover_text(entry.as_str());
                        paint_focus_ring(ui, &item);
                        if item.clicked() {
                            hit = Some(FileHit::Background(action));
                            ui.close();
                        }
                    }
                });
            }
        });
        // Keys act on the grid when the pointer is over it and no text field is being typed in.
        let over = self.ui.rect_contains_pointer(area.inner_rect);
        let typing = self.ui.ctx().memory(|m| m.focused().is_some());
        if hit.is_none() && over && !typing && rename_index.is_none() {
            let key = |k| self.ui.input(|i| i.key_pressed(k));
            if key(egui::Key::Backspace) {
                hit = Some(FileHit::Up);
            } else if let Some(index) = selected {
                if key(egui::Key::F2) {
                    hit = Some(FileHit::Rename(index));
                } else if key(egui::Key::Delete) {
                    hit = Some(FileHit::Delete(index));
                } else if key(egui::Key::Enter) {
                    hit = Some(FileHit::Open(index));
                }
            }
        }
        hit
    }

    /// Files and folders as `(name, is_folder)`, drawn as a list or as wrapped tiles.
    /// A right-click chooses one of `menu` and returns `(item, entry)`.
    pub fn file_items(
        &mut self,
        items: &[(String, bool)],
        list: bool,
        menu: &[String],
    ) -> Option<(usize, usize)> {
        let mut picked = None;
        let visuals = self.ui.visuals().clone();
        let body = TextStyle::Body.resolve(self.ui.style());
        let folder_ink = visuals.warn_fg_color;
        let file_ink = visuals.weak_text_color();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(self.ui, |ui| {
            if list {
                ui.spacing_mut().item_spacing.y = 0.0;
                let height = ui.spacing().interact_size.y;
                for (index, (name, folder)) in items.iter().enumerate() {
                    let (rect, response) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), height),
                        Sense::click_and_drag(),
                    );
                    if !folder && response.dragged() {
                        if let Some(kind) = Self::place_kind_from_label(name) {
                            response.dnd_set_drag_payload(format!("place:{kind}"));
                        }
                    }
                    let response = response.on_hover_text(name.as_str());
                    response.context_menu(|ui| {
                        for action in filtered_menu(ui, menu) {
                            let entry = &menu[action];
                            let item = ui.button(entry).on_hover_text(entry.as_str());
                            paint_focus_ring(ui, &item);
                            if item.clicked() {
                                picked = Some((index, action));
                                ui.close();
                            }
                        }
                    });
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::Label, true, name)
                    });
                    row_fill(ui, rect, &response, false, true);
                    let painter = ui.painter().with_clip_rect(rect);
                    let left = rect.left() + f32::from(PAD);
                    let icon = egui::Rect::from_center_size(
                        egui::pos2(left + 7.0, rect.center().y),
                        egui::vec2(14.0, 12.0),
                    );
                    file_icon(&painter, icon, *folder, folder_ink, file_ink);
                    painter.text(
                        egui::pos2(left + 22.0, rect.center().y),
                        Align2::LEFT_CENTER,
                        name,
                        body.clone(),
                        visuals.text_color(),
                    );
                }
                return;
            }
            egui::Frame::new().inner_margin(egui::Margin::same(PAD)).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (index, (name, folder)) in items.iter().enumerate() {
                        let (rect, response) =
                            ui.allocate_exact_size(TILE, Sense::click_and_drag());
                        if !folder && response.dragged() {
                            if let Some(kind) = Self::place_kind_from_label(name) {
                                response.dnd_set_drag_payload(format!("place:{kind}"));
                            }
                        }
                        let response = response.on_hover_text(name.as_str());
                        response.context_menu(|ui| {
                            for action in filtered_menu(ui, menu) {
                                let entry = &menu[action];
                                let item = ui.button(entry).on_hover_text(entry.as_str());
                                paint_focus_ring(ui, &item);
                                if item.clicked() {
                                    picked = Some((index, action));
                                    ui.close();
                                }
                            }
                        });
                        response.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::WidgetType::Label, true, name)
                        });
                        row_fill(ui, rect, &response, false, true);
                        let painter = ui.painter().with_clip_rect(rect.shrink(2.0));
                        let icon = egui::Rect::from_center_size(
                            egui::pos2(rect.center().x, rect.top() + 26.0),
                            egui::vec2(36.0, 30.0),
                        );
                        file_icon(&painter, icon, *folder, folder_ink, file_ink);
                        painter.text(
                            egui::pos2(rect.center().x, rect.bottom() - 6.0),
                            Align2::CENTER_BOTTOM,
                            name,
                            TextStyle::Small.resolve(ui.style()),
                            visuals.text_color(),
                        );
                    }
                });
            });
        });
        picked
    }

    /// Name on the left, three numbers on the right, each with its axis color.
    /// Returns the index of the field that changed.
    pub fn property3(&mut self, label: &str, values: &mut [String; 3]) -> Option<usize> {
        let mut menu_pick = None;
        self.property3_row(label, values, false, None, None, None, &mut menu_pick).0
    }

    /// Like [`property3`] with bright labels and optional per-axis reset icons on the right.
    #[allow(clippy::too_many_arguments)]
    pub fn property3_row(
        &mut self,
        label: &str,
        values: &mut [String; 3],
        label_bright: bool,
        resets: Option<(&[bool; 3], &[&str; 3])>,
        label_help: Option<&str>,
        row_menu: Option<&[crate::ContextMenuEntry]>,
        menu_pick: &mut Option<crate::ContextMenuPick>,
    ) -> (Option<usize>, Option<usize>, bool) {
        let (changed, reset, commit_focus, _, _) = self.vector_row(
            label,
            values,
            label_bright,
            resets,
            label_help,
            row_menu,
            menu_pick,
            VectorRowStyle::default(),
        );
        (changed, reset, commit_focus)
    }

    /// Rotation components with a degree suffix on each field.
    #[allow(clippy::too_many_arguments)]
    pub fn rotation_row(
        &mut self,
        label: &str,
        values: &mut [String; 3],
        label_bright: bool,
        resets: Option<(&[bool; 3], &[&str; 3])>,
        label_help: Option<&str>,
        row_menu: Option<&[crate::ContextMenuEntry]>,
        menu_pick: &mut Option<crate::ContextMenuPick>,
        unit_suffix: &str,
    ) -> (Option<usize>, Option<usize>, bool) {
        let (changed, reset, commit_focus, _, _) = self.vector_row(
            label,
            values,
            label_bright,
            resets,
            label_help,
            row_menu,
            menu_pick,
            VectorRowStyle { unit_suffix: Some(unit_suffix), ..VectorRowStyle::default() },
        );
        (changed, reset, commit_focus)
    }

    /// Scale row with optional uniform-scale lock icon.
    #[allow(clippy::too_many_arguments)]
    pub fn scale_row(
        &mut self,
        label: &str,
        values: &mut [String; 3],
        label_bright: bool,
        resets: Option<(&[bool; 3], &[&str; 3])>,
        label_help: Option<&str>,
        row_menu: Option<&[crate::ContextMenuEntry]>,
        menu_pick: &mut Option<crate::ContextMenuPick>,
        uniform_locked: bool,
        lock_tip: &str,
    ) -> (Option<usize>, Option<usize>, bool, bool) {
        let (changed, reset, commit_focus, _, lock_toggle) = self.vector_row(
            label,
            values,
            label_bright,
            resets,
            label_help,
            row_menu,
            menu_pick,
            VectorRowStyle {
                uniform_scale_locked: Some(uniform_locked),
                uniform_scale_tip: Some(lock_tip),
                ..VectorRowStyle::default()
            },
        );
        (changed, reset, commit_focus, lock_toggle)
    }

    /// Three-component vector row with optional absolute/relative mode on the label.
    #[allow(clippy::too_many_arguments)]
    pub fn vector_row(
        &mut self,
        label: &str,
        values: &mut [String; 3],
        label_bright: bool,
        resets: Option<(&[bool; 3], &[&str; 3])>,
        label_help: Option<&str>,
        row_menu: Option<&[crate::ContextMenuEntry]>,
        menu_pick: &mut Option<crate::ContextMenuPick>,
        style: VectorRowStyle<'_>,
    ) -> (Option<usize>, Option<usize>, bool, Option<String>, bool) {
        const AXES: [Color32; 3] = [crate::AXIS_X, crate::AXIS_Y, crate::AXIS_Z];
        let mut changed = None;
        let mut reset = None;
        let mut commit_focus = false;
        let mut mode_pick = None;
        let mut lock_toggle = false;
        self.ui.horizontal(|ui| {
            let name = property_name(ui, label, label_bright, label_help);
            // Fixed column grid from the label split to the right edge, so the X/Y/Z
            // fields of Location, Rotation and Scale line up to the pixel no matter
            // which prefix control or reset icon a row shows.
            let h = ui.spacing().interact_size.y;
            let left = ui.cursor().left();
            let right = ui.max_rect().right();
            let top = ui.cursor().top();
            let row = egui::Rect::from_min_max(egui::pos2(left, top), egui::pos2(right, top + h));
            const GAP: f32 = 4.0;
            let prefix = egui::Rect::from_min_size(row.min, egui::vec2(VECTOR_PREFIX_PX, h));
            let fields_left = prefix.right() + GAP;
            let fields_right = row.right();
            let cell_w = ((fields_right - fields_left - GAP * 2.0) / 3.0).max(28.0);
            let place =
                |ui: &mut egui::Ui, rect: egui::Rect, add: &mut dyn FnMut(&mut egui::Ui)| {
                    ui.scope_builder(
                        egui::UiBuilder::new()
                            .max_rect(rect)
                            .layout(egui::Layout::left_to_right(egui::Align::Center)),
                        |ui| add(ui),
                    );
                };
            if let Some(mode) = style.mode {
                place(ui, prefix, &mut |ui| mode_pick = paint_vector_space_combo(ui, label, mode));
            } else if let (Some(locked), Some(tip)) =
                (style.uniform_scale_locked, style.uniform_scale_tip)
            {
                place(ui, prefix, &mut |ui| {
                    lock_toggle = paint_scale_uniform_lock(ui, label, locked, tip)
                });
            }
            let step = vector_drag_step(&style);
            let small = TextStyle::Small.resolve(ui.style());
            let body = TextStyle::Body.resolve(ui.style());
            for (index, value) in values.iter_mut().enumerate() {
                let x = fields_left + index as f32 * (cell_w + GAP);
                let cell = egui::Rect::from_min_max(
                    egui::pos2(x, row.top()),
                    egui::pos2(x + cell_w, row.bottom()),
                );
                let show_reset = resets.is_some_and(|(show, _)| show[index]);
                let reset_w = if show_reset { crate::crisp::RESET_PX + 4.0 } else { 0.0 };
                let unit_w = style
                    .unit_suffix
                    .map(|u| {
                        ui.painter()
                            .layout_no_wrap(u.to_string(), small.clone(), Color32::GRAY)
                            .size()
                            .x
                            + 8.0
                    })
                    .unwrap_or(0.0);
                // A narrow cell keeps the number readable and drops the unit instead of
                // letting the two overlap.
                let text_left = cell.left() + 1.0 + crate::crisp::LED_BAR_PX + 6.0;
                let value_w = ui
                    .painter()
                    .layout_no_wrap(value.clone(), body.clone(), Color32::GRAY)
                    .size()
                    .x
                    + 6.0;
                let unit_w = if cell.right() - 4.0 - reset_w - unit_w - text_left < value_w {
                    0.0
                } else {
                    unit_w
                };
                let slot = ui.painter().add(egui::Shape::Noop);
                let field_rect = egui::Rect::from_min_max(
                    egui::pos2(cell.left() + 1.0 + crate::crisp::LED_BAR_PX + 6.0, row.top()),
                    egui::pos2(
                        (cell.right() - 4.0 - unit_w - reset_w).max(cell.left() + 24.0),
                        row.bottom(),
                    ),
                );
                let edit_id = ui.id().with(("vector-field", label, index));
                // While typing, show what the user typed, not the model's re-formatted
                // value; otherwise "0." collapses to "0" and the next digit lands wrong.
                let draft_id = edit_id.with("draft");
                if ui.memory(|mem| mem.has_focus(edit_id)) {
                    if let Some(draft) = ui.data(|data| data.get_temp::<String>(draft_id)) {
                        *value = draft;
                    }
                }
                let field = ui
                    .put(
                        field_rect,
                        egui::TextEdit::singleline(value)
                            .id(edit_id)
                            .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(0, 2)))
                            .vertical_align(egui::Align::Center)
                            .desired_width(f32::INFINITY),
                    )
                    .labelled_by(name)
                    .on_hover_text(label);
                let focused = field.has_focus();
                {
                    let visuals = ui.visuals().clone();
                    let mut shapes: Vec<egui::Shape> = Vec::new();
                    crate::crisp::sunken(&mut shapes, cell, &visuals, focused, field.hovered());
                    crate::crisp::led_bar(&mut shapes, cell, AXES[index], 0.18);
                    if focused {
                        let tw = ui
                            .painter()
                            .layout_no_wrap(value.clone(), body.clone(), Color32::WHITE)
                            .size()
                            .x;
                        let back = egui::Rect::from_min_max(
                            egui::pos2(field_rect.left() - 3.0, cell.top() + 4.0),
                            egui::pos2(
                                (field_rect.left() + tw + 7.0).min(field_rect.right()),
                                cell.bottom() - 4.0,
                            ),
                        );
                        crate::crisp::backing(&mut shapes, back, &visuals);
                        if unit_w > 0.0 {
                            let ux = cell.right() - 4.0 - reset_w;
                            let back = egui::Rect::from_min_max(
                                egui::pos2(ux - unit_w + 2.0, cell.top() + 4.0),
                                egui::pos2(ux + 2.0, cell.bottom() - 4.0),
                            );
                            crate::crisp::backing(&mut shapes, back, &visuals);
                        }
                    }
                    crate::crisp::place_behind(ui.painter(), slot, shapes);
                }
                if field.changed() {
                    changed = Some(index);
                }
                if field.has_focus() {
                    let typed = value.clone();
                    ui.data_mut(|data| data.insert_temp(draft_id, typed));
                } else {
                    ui.data_mut(|data| data.remove::<String>(draft_id));
                }
                commit_focus |= field.lost_focus();
                // Drag left/right on a field to scrub its value; a plain click types.
                if !focused {
                    let scrub =
                        ui.interact(field.rect, edit_id.with("scrub"), Sense::click_and_drag());
                    if scrub.hovered() || scrub.dragged() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                    }
                    if scrub.dragged() {
                        let dx = scrub.drag_delta().x;
                        if dx != 0.0 {
                            if let Some(current) =
                                crate::number_field::eval_number_expression(value)
                            {
                                let speed = crate::number_field::drag_speed(step, ui);
                                *value = crate::number_field::format_number(current + dx * speed);
                                changed = Some(index);
                            }
                        }
                    }
                    if scrub.drag_stopped() {
                        commit_focus = true;
                    }
                    if scrub.clicked() {
                        // A click selects the whole value, so typing replaces it
                        // instead of appending to it.
                        crate::number_field::focus_select_all(ui, edit_id);
                    }
                }
                if let Some(suffix) = style.unit_suffix.filter(|_| unit_w > 0.0) {
                    // Unit sits inside the field's right edge, left of the reset button.
                    ui.painter().text(
                        egui::pos2(cell.right() - 6.0 - reset_w, cell.center().y),
                        Align2::RIGHT_CENTER,
                        suffix,
                        small.clone(),
                        ui.visuals().weak_text_color(),
                    );
                }
                if let Some((show, tips)) = resets {
                    if show[index] {
                        let r = egui::Rect::from_center_size(
                            egui::pos2(
                                cell.right() - 3.0 - crate::crisp::RESET_PX / 2.0,
                                cell.center().y,
                            ),
                            egui::vec2(crate::crisp::RESET_PX, crate::crisp::RESET_PX),
                        );
                        if crate::crisp::reset_button(ui, r, edit_id.with("reset"), tips[index]) {
                            reset = Some(index);
                        }
                    }
                }
            }
            ui.expand_to_include_rect(row);
            ui.advance_cursor_after_rect(row);
            attach_property_row_context(ui, label, row_menu, menu_pick, None);
            paint_row_rule(ui);
            let _ = name;
        });
        // Clicking a reset arrow also takes focus from the field; the reset wins.
        if reset.is_some() {
            commit_focus = false;
        }
        (changed, reset, commit_focus, mode_pick, lock_toggle)
    }

    /// Property row with a 14 px checkbox; mixed selection shows a dash.
    #[allow(clippy::too_many_arguments)]
    pub fn property_checkbox_row(
        &mut self,
        label: &str,
        value: PropertyCheckbox,
        label_bright: bool,
        label_help: Option<&str>,
        row_menu: Option<&[crate::ContextMenuEntry]>,
        menu_pick: &mut Option<crate::ContextMenuPick>,
        field_key: Option<&str>,
    ) -> Option<PropertyCheckbox> {
        let mut picked = None;
        self.ui.horizontal(|ui| {
            let name = property_name(ui, label, label_bright, label_help);
            let box_size = egui::vec2(14.0, 14.0);
            // Checkbox starts the value column, where every other value starts.
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add_space(4.0);
                let (rect, hit) = ui.allocate_exact_size(box_size, Sense::click());
                let visuals = ui.visuals().clone();
                let painter = ui.painter().clone();
                crate::crisp::checkbox(
                    &mut &painter,
                    rect,
                    &visuals,
                    value == PropertyCheckbox::On,
                    value == PropertyCheckbox::Mixed,
                    hit.hovered(),
                );
                let hit = hit.labelled_by(name).on_hover_text(label);
                hit.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Checkbox, true, label)
                });
                paint_focus_ring(ui, &hit);
                if hit.clicked() {
                    picked = Some(match value {
                        PropertyCheckbox::Mixed | PropertyCheckbox::Off => PropertyCheckbox::On,
                        PropertyCheckbox::On => PropertyCheckbox::Off,
                    });
                }
            });
            attach_property_row_context(ui, label, row_menu, menu_pick, field_key);
            paint_row_rule(ui);
            let _ = name;
        });
        picked
    }

    /// Three joined segments; the active value uses the accent fill.
    #[allow(clippy::too_many_arguments)]
    pub fn motion_mode_row(
        &mut self,
        label: &str,
        current: &str,
        options: &[(&str, &str)],
        label_bright: bool,
        label_help: Option<&str>,
        row_menu: Option<&[crate::ContextMenuEntry]>,
        menu_pick: &mut Option<crate::ContextMenuPick>,
        field_key: Option<&str>,
    ) -> Option<String> {
        let mut picked = None;
        self.ui.horizontal(|ui| {
            let name = property_name(ui, label, label_bright, label_help);
            let h = ui.spacing().interact_size.y;
            let count = options.len().max(1) as f32;
            let seg_gap = ui.spacing().item_spacing.x;
            let seg_w =
                ((ui.available_width() - seg_gap * (count - 1.0)) / count).max(24.0).floor();
            let visuals = ui.visuals().clone();
            let stroke_color = visuals.widgets.noninteractive.bg_stroke.color;
            let sel_fill = visuals.selection.bg_fill;
            let inactive_fill = visuals.widgets.inactive.bg_fill;
            let strong_ink = visuals.strong_text_color();
            let weak_ink = visuals.weak_text_color();
            let stroke = Stroke::new(1.0, stroke_color);
            for (index, (id, title)) in options.iter().enumerate() {
                let selected = *id == current;
                let (rect, response) = ui.allocate_exact_size(egui::vec2(seg_w, h), Sense::click());
                let rounding = egui::CornerRadius::ZERO;
                let _ = index;
                let _ = (rounding, sel_fill, inactive_fill, stroke, weak_ink);
                let text_ink = if selected { Color32::WHITE } else { visuals.text_color() };
                let _ = strong_ink;
                let state = if response.is_pointer_button_down_on() {
                    crate::crisp::Raised::Pressed
                } else if selected {
                    crate::crisp::Raised::Active
                } else if response.hovered() {
                    crate::crisp::Raised::Hover
                } else {
                    crate::crisp::Raised::Normal
                };
                let painter = ui.painter().clone();
                crate::crisp::raised(&mut &painter, rect, &visuals, state);
                let slot = egui::Rect::from_min_max(
                    egui::pos2(rect.left() + 6.0, rect.top() + 3.0),
                    egui::pos2(rect.right() - 6.0, rect.top() + 6.0),
                );
                crate::crisp::led_slot(
                    &mut &painter,
                    slot,
                    selected,
                    visuals.selection.stroke.color,
                );
                let rect = rect.translate(egui::vec2(0.0, 2.0));
                paint_focus_ring(ui, &response);
                // Fall back to the small font when a segment is too narrow, so
                // labels like "Mixed" are never clipped.
                let body = TextStyle::Body.resolve(ui.style());
                let fits = ui
                    .painter()
                    .layout_no_wrap((*title).to_string(), body.clone(), text_ink)
                    .size()
                    .x
                    <= rect.width() - 6.0;
                let font = if fits { body } else { TextStyle::Small.resolve(ui.style()) };
                ui.painter().text(rect.center(), Align2::CENTER_CENTER, *title, font, text_ink);
                let response = if fits { response } else { response.on_hover_text(*title) };
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Button,
                        true,
                        format!("{label} {title}"),
                    )
                });
                if response.clicked() && !selected {
                    picked = Some((*id).to_string());
                }
            }
            attach_property_row_context(ui, label, row_menu, menu_pick, field_key);
            paint_row_rule(ui);
            let _ = name;
        });
        picked
    }

    /// Name on the left, a number that can be typed or dragged on the right.
    pub fn number(&mut self, label: &str, value: &mut f32, step: f32) -> bool {
        let mut menu_pick = None;
        self.number_row(label, value, step, false, None, None, None, &mut menu_pick).0
    }

    /// Like [`number`] with a bright label and an optional reset icon.
    #[allow(clippy::too_many_arguments)]
    pub fn number_row(
        &mut self,
        label: &str,
        value: &mut f32,
        step: f32,
        label_bright: bool,
        reset_tip: Option<&str>,
        label_help: Option<&str>,
        row_menu: Option<&[crate::ContextMenuEntry]>,
        menu_pick: &mut Option<crate::ContextMenuPick>,
    ) -> (bool, bool, NumberDragSignals) {
        let mut changed = false;
        let mut reset = false;
        let mut drag = NumberDragSignals::default();
        self.ui.horizontal(|ui| {
            let name = property_name(ui, label, label_bright, label_help);
            let response = ui
                .add(egui::DragValue::new(value).speed(step))
                .labelled_by(name)
                .on_hover_text(label);
            paint_focus_ring(ui, &response);
            changed = response.changed();
            drag.drag_started = response.drag_started();
            drag.drag_stopped = response.drag_stopped();
            drag.commit_focus = response.lost_focus();
            drag.escape_cancel =
                response.dragged() && ui.input(|input| input.key_pressed(egui::Key::Escape));
            if let Some(tip) = reset_tip {
                reset = paint_row_reset_icon(ui, tip);
            }
            attach_property_row_context(ui, label, row_menu, menu_pick, None);
            paint_row_rule(ui);
        });
        (changed, reset, drag)
    }

    /// Number row with expressions, optional unit suffix, range fill bar, and Ctrl/Shift drag step.
    #[allow(clippy::too_many_arguments)]
    pub fn number_field_row(
        &mut self,
        label: &str,
        value: &mut f32,
        step: f32,
        label_bright: bool,
        reset_tip: Option<&str>,
        label_help: Option<&str>,
        row_menu: Option<&[crate::ContextMenuEntry]>,
        menu_pick: &mut Option<crate::ContextMenuPick>,
        spec: &crate::number_field::NumberFieldSpec,
        field_key: Option<&str>,
    ) -> (bool, bool, NumberDragSignals) {
        let mut changed = false;
        let mut reset = false;
        let mut drag = NumberDragSignals::default();
        self.ui.horizontal(|ui| {
            let name = property_name(ui, label, label_bright, label_help);
            let (value_changed, drag_signals, reset_hit) =
                crate::number_field::paint_number_field_value(
                    ui, label, value, step, spec, reset_tip,
                );
            changed = value_changed;
            drag = drag_signals;
            reset = reset_hit;
            let _ = name;
            attach_property_row_context(ui, label, row_menu, menu_pick, field_key);
            paint_row_rule(ui);
        });
        (changed, reset, drag)
    }

    /// Name on the left, text on the right, and an optional validation line that never blocks typing.
    pub fn property_validated(
        &mut self,
        label: &str,
        text: &mut String,
        error: Option<&str>,
    ) -> bool {
        let mut menu_pick = None;
        self.property_validated_row(
            label,
            text,
            error,
            false,
            None,
            None,
            None,
            &mut menu_pick,
            None,
        )
        .0
    }

    /// Like [`property_validated`] with bright modified labels and an optional reset icon.
    #[allow(clippy::too_many_arguments)]
    pub fn property_validated_row(
        &mut self,
        label: &str,
        text: &mut String,
        error: Option<&str>,
        label_bright: bool,
        reset_tip: Option<&str>,
        label_help: Option<&str>,
        row_menu: Option<&[crate::ContextMenuEntry]>,
        menu_pick: &mut Option<crate::ContextMenuPick>,
        field_key: Option<&str>,
    ) -> (bool, bool, bool) {
        let mut changed = false;
        let mut reset = false;
        let mut commit_focus = false;
        self.ui.horizontal(|ui| {
            let name = property_name(ui, label, label_bright, label_help);
            // Reset sits inside the field's right end, like the number fields.
            let right = if reset_tip.is_some() { crate::crisp::RESET_PX as i8 + 8 } else { 6 };
            // Keep the typed text while focused so live edits don't re-format it under
            // the cursor ("0." would snap back to "0").
            let edit_id = ui.make_persistent_id(label);
            let draft_id = edit_id.with("draft");
            if ui.memory(|mem| mem.has_focus(edit_id)) {
                if let Some(draft) = ui.data(|data| data.get_temp::<String>(draft_id)) {
                    *text = draft;
                }
            }
            let response = crate::crisp::value_well(
                ui,
                egui::TextEdit::singleline(text).id(edit_id).desired_width(f32::INFINITY),
                right,
            )
            .labelled_by(name)
            .on_hover_text(label);
            paint_focus_ring(ui, &response);
            if response.has_focus() {
                let typed = text.clone();
                ui.data_mut(|data| data.insert_temp(draft_id, typed));
            } else {
                ui.data_mut(|data| data.remove::<String>(draft_id));
            }
            changed = response.changed();
            commit_focus = response.lost_focus();
            if let Some(tip) = reset_tip {
                let r = egui::Rect::from_center_size(
                    egui::pos2(
                        response.rect.right() - 3.0 - crate::crisp::RESET_PX / 2.0,
                        response.rect.center().y,
                    ),
                    egui::vec2(crate::crisp::RESET_PX, crate::crisp::RESET_PX),
                );
                reset = crate::crisp::reset_button(ui, r, response.id.with("reset"), tip);
                if reset {
                    commit_focus = false;
                }
            }
            attach_property_row_context(ui, label, row_menu, menu_pick, field_key);
            paint_row_rule(ui);
        });
        if let Some(error) = error {
            self.ui.small(egui::RichText::new(error).color(self.ui.visuals().error_fg_color));
        }
        (changed, reset, commit_focus)
    }

    /// Like [`choice`] with bright label and optional reset.
    #[allow(clippy::too_many_arguments)]
    pub fn choice_row(
        &mut self,
        label: &str,
        current: &str,
        options: &[(&str, String)],
        label_bright: bool,
        reset_tip: Option<&str>,
        label_help: Option<&str>,
        row_menu: Option<&[crate::ContextMenuEntry]>,
        menu_pick: &mut Option<crate::ContextMenuPick>,
        field_key: Option<&str>,
    ) -> (Option<String>, bool) {
        let mut picked = None;
        let mut reset = false;
        self.ui.horizontal(|ui| {
            let name = property_name(ui, label, label_bright, label_help);
            let shown = options
                .iter()
                .find(|(id, _)| *id == current)
                .map_or(current, |(_, title)| title.as_str());
            let reset_w = if reset_tip.is_some() { 18.0 } else { 0.0 };
            let response = egui::ComboBox::from_id_salt(label)
                .selected_text(shown)
                .width((ui.available_width() - reset_w).max(48.0))
                .show_crisp(ui, |ui| {
                    for (id, title) in options {
                        if ui.selectable_label(*id == current, title).clicked() {
                            picked = Some((*id).to_string());
                        }
                    }
                })
                .response
                .labelled_by(name)
                .on_hover_text(label);
            paint_focus_ring(ui, &response);
            if let Some(tip) = reset_tip {
                reset = paint_row_reset_icon(ui, tip);
            }
            attach_property_row_context(ui, label, row_menu, menu_pick, field_key);
            paint_row_rule(ui);
        });
        (picked, reset)
    }

    /// Bit-set row with All / None and named layers in a dropdown.
    #[allow(clippy::too_many_arguments)]
    pub fn flags_field_row(
        &mut self,
        label: &str,
        bits: u32,
        bit_count: u32,
        layer_labels: &[String],
        all_label: &str,
        none_label: &str,
        label_bright: bool,
        reset_tip: Option<&str>,
        label_help: Option<&str>,
        row_menu: Option<&[crate::ContextMenuEntry]>,
        menu_pick: &mut Option<crate::ContextMenuPick>,
        field_key: Option<&str>,
    ) -> (Option<u32>, bool) {
        let mut picked = None;
        let mut reset = false;
        self.ui.horizontal(|ui| {
            let name = property_name(ui, label, label_bright, label_help);
            if let Some(next) = crate::flags_field::paint_flags_value(
                ui,
                label,
                bits,
                bit_count,
                layer_labels,
                all_label,
                none_label,
            ) {
                picked = Some(next);
            }
            let _ = name;
            if let Some(tip) = reset_tip {
                reset = paint_row_reset_icon(ui, tip);
            }
            attach_property_row_context(ui, label, row_menu, menu_pick, field_key);
            paint_row_rule(ui);
        });
        (picked, reset)
    }

    /// RGB with a swatch and channel editors. Returns the channel index that changed.
    pub fn color_rgb(
        &mut self,
        label: &str,
        rgb: &mut [f32; 3],
        mixed: bool,
        mixed_hint: &str,
    ) -> Option<usize> {
        self.color_rgb_with_resets(label, rgb, mixed, mixed_hint, None).0
    }

    /// [`color_rgb`] with a reset button inside each channel that differs from its
    /// default. Returns (changed channel, reset channel).
    pub fn color_rgb_with_resets(
        &mut self,
        label: &str,
        rgb: &mut [f32; 3],
        mixed: bool,
        mixed_hint: &str,
        resets: Option<(&[bool; 3], &[&str; 3])>,
    ) -> (Option<usize>, Option<usize>) {
        let mut changed = None;
        let mut reset = None;
        self.ui.horizontal(|ui| {
            let name = property_name(ui, label, false, None);
            let side = ui.spacing().interact_size.y;
            let (rect, response) = ui.allocate_exact_size(egui::vec2(side, side), Sense::click());
            let response = response.labelled_by(name).on_hover_text(label);
            paint_focus_ring(ui, &response);
            if !mixed {
                let chip = rect.shrink(3.0);
                let fill = Color32::from_rgb(
                    (rgb[0].clamp(0.0, 1.0) * 255.0) as u8,
                    (rgb[1].clamp(0.0, 1.0) * 255.0) as u8,
                    (rgb[2].clamp(0.0, 1.0) * 255.0) as u8,
                );
                ui.painter().rect_filled(chip, 0.0, fill);
                let mut popup = [rgb[0], rgb[1], rgb[2], 1.0];
                let mut recent = crate::colour_picker::recent_now(ui);
                let mut colour_change =
                    crate::colour_picker::ColourChange { changed: false, commit: false };
                egui::Popup::menu(&response).show(|ui| {
                    colour_change = crate::colour_picker::edit(
                        ui,
                        &mut popup,
                        &mut recent,
                        &crate::colour_picker::ColourLabels {
                            hex: "Hex".into(),
                            alpha: "Alpha".into(),
                            recent: "Recent".into(),
                        },
                    );
                });
                crate::colour_picker::store_recent(ui, recent);
                if colour_change.commit {
                    rgb[0] = popup[0];
                    rgb[1] = popup[1];
                    rgb[2] = popup[2];
                    changed = Some(0);
                }
            }
            // Same fixed grid as the vector rows: three equal cells from the swatch to the
            // right edge, each with its channel LED bar and its own reset inside.
            const GAP: f32 = 4.0;
            let h = side;
            let top = rect.top();
            let fields_left = rect.right() + GAP;
            let fields_right = ui.max_rect().right();
            let cell_w = ((fields_right - fields_left - GAP * 2.0) / 3.0).max(28.0);
            let body = TextStyle::Body.resolve(ui.style());
            let mut show = if mixed {
                [mixed_hint.into(), mixed_hint.into(), mixed_hint.into()]
            } else {
                rgb.map(|channel| channel.to_string())
            };
            for (index, value) in show.iter_mut().enumerate() {
                let x = fields_left + index as f32 * (cell_w + GAP);
                let cell =
                    egui::Rect::from_min_max(egui::pos2(x, top), egui::pos2(x + cell_w, top + h));
                let show_reset = resets.is_some_and(|(show, _)| show[index]);
                let reset_w = if show_reset { crate::crisp::RESET_PX + 4.0 } else { 0.0 };
                let slot = ui.painter().add(egui::Shape::Noop);
                let field_rect = egui::Rect::from_min_max(
                    egui::pos2(cell.left() + 1.0 + crate::crisp::LED_BAR_PX + 6.0, cell.top()),
                    egui::pos2(
                        (cell.right() - 4.0 - reset_w).max(cell.left() + 24.0),
                        cell.bottom(),
                    ),
                );
                let edit_id = ui.make_persistent_id((label, index));
                let draft_id = edit_id.with("draft");
                if ui.memory(|mem| mem.has_focus(edit_id)) {
                    if let Some(draft) = ui.data(|data| data.get_temp::<String>(draft_id)) {
                        *value = draft;
                    }
                }
                let field = ui
                    .put(
                        field_rect,
                        egui::TextEdit::singleline(value)
                            .id(edit_id)
                            .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(0, 2)))
                            .vertical_align(egui::Align::Center)
                            .desired_width(f32::INFINITY),
                    )
                    .labelled_by(name)
                    .on_hover_text(label);
                let focused = field.has_focus();
                if focused {
                    let typed = value.clone();
                    ui.data_mut(|data| data.insert_temp(draft_id, typed));
                } else {
                    ui.data_mut(|data| data.remove::<String>(draft_id));
                }
                {
                    let visuals = ui.visuals().clone();
                    let mut shapes: Vec<egui::Shape> = Vec::new();
                    crate::crisp::sunken(&mut shapes, cell, &visuals, focused, field.hovered());
                    crate::crisp::led_bar(
                        &mut shapes,
                        cell,
                        [crate::AXIS_X, crate::AXIS_Y, crate::AXIS_Z][index],
                        0.18,
                    );
                    if focused {
                        let tw = ui
                            .painter()
                            .layout_no_wrap(value.clone(), body.clone(), Color32::WHITE)
                            .size()
                            .x;
                        let back = egui::Rect::from_min_max(
                            egui::pos2(field_rect.left() - 3.0, cell.top() + 4.0),
                            egui::pos2(
                                (field_rect.left() + tw + 7.0).min(field_rect.right()),
                                cell.bottom() - 4.0,
                            ),
                        );
                        crate::crisp::backing(&mut shapes, back, &visuals);
                    }
                    crate::crisp::place_behind(ui.painter(), slot, shapes);
                }
                if let Some((show, tips)) = resets {
                    if show[index] {
                        let r = egui::Rect::from_center_size(
                            egui::pos2(
                                cell.right() - 3.0 - crate::crisp::RESET_PX / 2.0,
                                cell.center().y,
                            ),
                            egui::vec2(crate::crisp::RESET_PX, crate::crisp::RESET_PX),
                        );
                        if crate::crisp::reset_button(ui, r, field.id.with("reset"), tips[index]) {
                            reset = Some(index);
                        }
                    }
                }
                if field.changed() && !mixed {
                    if let Ok(parsed) = value.parse::<f32>() {
                        rgb[index] = parsed.clamp(0.0, 1.0);
                        changed = Some(index);
                    }
                }
                // Drag sideways on a field that is not being typed in to scrub the channel.
                if !mixed && !focused {
                    let scrub = ui
                        .interact(field.rect, field.id.with("scrub"), Sense::click_and_drag())
                        .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
                    if scrub.dragged() {
                        let dx = scrub.drag_delta().x;
                        if dx != 0.0 {
                            let speed = crate::number_field::drag_speed(0.005, ui);
                            rgb[index] = (rgb[index] + dx * speed).clamp(0.0, 1.0);
                            rgb[index] = (rgb[index] * 1000.0).round() / 1000.0;
                            changed = Some(index);
                        }
                    } else if scrub.clicked() {
                        crate::number_field::focus_select_all(ui, field.id);
                    }
                }
            }
        });
        (changed, reset)
    }

    /// Name on the left, a value that cannot be edited on the right.
    pub fn readout(&mut self, label: &str, value: &str) {
        self.ui.horizontal(|ui| {
            let _name = property_name(ui, label, false, None);
            ui.label(value);
        });
    }

    /// Name on the left, a small color chip on the right. Returns whether the chip was clicked.
    pub fn swatch(&mut self, label: &str, rgb: [u8; 3]) -> bool {
        let mut clicked = false;
        self.ui.horizontal(|ui| {
            let name = property_name(ui, label, false, None);
            let side = ui.spacing().interact_size.y;
            let (rect, response) = ui.allocate_exact_size(egui::vec2(side, side), Sense::click());
            let response = response.labelled_by(name).on_hover_text(label);
            response
                .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
            paint_focus_ring(ui, &response);
            let chip = rect.shrink(3.0);
            let fill = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
            let frame = ui.visuals().widgets.noninteractive.bg_stroke;
            if response.hovered() {
                crate::crisp::raised(
                    &mut ui.painter(),
                    rect,
                    ui.visuals(),
                    crate::crisp::Raised::Hover,
                );
            }
            ui.painter().rect_filled(chip, 0.0, fill);
            ui.painter().rect_stroke(chip, 0.0, frame, egui::StrokeKind::Inside);
            clicked = response.clicked();
        });
        clicked
    }

    /// Square icon control. `tooltip` is the accessible name.
    pub fn icon_button(&mut self, icon: crate::icons::Icon, tooltip: &str) -> bool {
        self.icon_toggle(icon, false, tooltip)
    }

    /// 12 px reset arrow at the row end.
    pub fn row_reset_icon(&mut self, tooltip: &str) -> bool {
        paint_row_reset_icon(self.ui, tooltip)
    }

    /// Square icon control that stays filled while `on` is true.
    pub fn icon_toggle(&mut self, icon: crate::icons::Icon, on: bool, tooltip: &str) -> bool {
        let (rect, response) = self.ui.allocate_exact_size(egui::vec2(22.0, 22.0), Sense::click());
        paint_icon_plate(self.ui, rect, &response, on);
        let response =
            crate::a11y::named_control(self.ui, response, egui::WidgetType::Button, tooltip);
        let ink = self.ui.visuals().text_color();
        crate::icons::paint_icon(self.ui.painter(), rect.center(), icon, crate::ICON_PX, ink);
        response.clicked()
    }

    /// Square icon button with an explicit ink colour (for example the green add control).
    pub fn icon_button_colored(
        &mut self,
        icon: crate::icons::Icon,
        tooltip: &str,
        color: Color32,
    ) -> bool {
        let (rect, response) = self.ui.allocate_exact_size(egui::vec2(22.0, 22.0), Sense::click());
        paint_icon_plate(self.ui, rect, &response, false);
        let response =
            crate::a11y::named_control(self.ui, response, egui::WidgetType::Button, tooltip);
        crate::icons::paint_icon(self.ui.painter(), rect.center(), icon, crate::ICON_PX, color);
        response.clicked()
    }

    /// Property value shown when a multi-selection disagrees.
    pub fn mixed_value(&mut self, text: &str) {
        let weak = egui::RichText::new(text).italics().color(self.ui.visuals().weak_text_color());
        self.ui.label(weak);
    }

    /// Icon menu with a custom icon colour (for example the green add control).
    pub fn icon_menu_colored(
        &mut self,
        icon: crate::icons::Icon,
        tooltip: &str,
        color: Color32,
        body: impl FnOnce(&mut egui::Ui),
    ) -> bool {
        let (rect, response) = self.ui.allocate_exact_size(egui::vec2(32.0, 22.0), Sense::click());
        paint_menu_plate(self.ui, rect, &response);
        let response =
            crate::a11y::named_control(self.ui, response, egui::WidgetType::Button, tooltip);
        crate::icons::paint_icon(
            self.ui.painter(),
            egui::pos2(rect.left() + 11.0, rect.center().y),
            icon,
            crate::ICON_PX,
            color,
        );
        crate::icons::paint_icon(
            self.ui.painter(),
            egui::pos2(rect.right() - 7.0, rect.center().y),
            crate::icons::Icon::ViewChevronDown,
            10.0,
            self.ui.visuals().text_color(),
        );
        egui::Popup::menu(&response).show(body);
        response.clicked()
    }

    /// Icon control with a small arrow. `body` is the menu shown on click.
    pub fn icon_menu(
        &mut self,
        icon: crate::icons::Icon,
        tooltip: &str,
        body: impl FnOnce(&mut egui::Ui),
    ) -> bool {
        let (rect, response) = self.ui.allocate_exact_size(egui::vec2(32.0, 22.0), Sense::click());
        paint_menu_plate(self.ui, rect, &response);
        let response =
            crate::a11y::named_control(self.ui, response, egui::WidgetType::Button, tooltip);
        let ink = self.ui.visuals().text_color();
        crate::icons::paint_icon(
            self.ui.painter(),
            egui::pos2(rect.left() + 11.0, rect.center().y),
            icon,
            crate::ICON_PX,
            ink,
        );
        crate::icons::paint_icon(
            self.ui.painter(),
            egui::pos2(rect.right() - 7.0, rect.center().y),
            crate::icons::Icon::ViewChevronDown,
            10.0,
            ink,
        );
        egui::Popup::menu(&response).show(body);
        response.clicked()
    }

    /// Icon plus a label, for the main toolbar. `label` is the accessible name.
    pub fn tool_button(&mut self, icon: crate::icons::Icon, label: &str) -> bool {
        let font = TextStyle::Body.resolve(self.ui.style());
        let galley = self.ui.painter().layout_no_wrap(
            label.to_owned(),
            font,
            self.ui.visuals().text_color(),
        );
        let width = 8.0 + crate::ICON_PX + 6.0 + galley.size().x + 8.0;
        let (rect, response) =
            self.ui.allocate_exact_size(egui::vec2(width, crate::TOOL_BUTTON_PX), Sense::click());
        paint_menu_plate(self.ui, rect, &response);
        let response =
            crate::a11y::named_control(self.ui, response, egui::WidgetType::Button, label);
        let ink = self.ui.visuals().text_color();
        crate::icons::paint_icon(
            self.ui.painter(),
            egui::pos2(rect.left() + 8.0 + crate::ICON_PX * 0.5, rect.center().y),
            icon,
            crate::ICON_PX,
            ink,
        );
        let text_y = rect.center().y - galley.size().y * 0.5;
        self.ui.painter().galley(
            egui::pos2(rect.left() + 8.0 + crate::ICON_PX + 6.0, text_y),
            galley,
            ink,
        );
        response.clicked()
    }

    /// Every icon at 16 px and 24 px, with its name. Asset icons carry their type colour.
    pub fn icon_gallery(&mut self) {
        let icons = crate::icons::Icon::ALL;
        let cell_w = 220.0;
        let cell_h = 28.0;
        let width = self.ui.available_width().max(cell_w);
        let columns = ((width / cell_w).floor() as usize).max(1);
        let rows = icons.len().div_ceil(columns);
        let (_, rect) = self.ui.allocate_space(egui::vec2(width, rows as f32 * cell_h));
        let painter = self.ui.painter().with_clip_rect(rect);
        let ink = self.ui.visuals().text_color();
        let dim = self.ui.visuals().weak_text_color();
        let font = TextStyle::Small.resolve(self.ui.style());
        for (index, icon) in icons.iter().copied().enumerate() {
            let col = index % columns;
            let row = index / columns;
            let origin = rect.left_top() + egui::vec2(col as f32 * cell_w, row as f32 * cell_h);
            let cell = egui::Rect::from_min_size(origin, egui::vec2(cell_w, cell_h));
            let cell_painter = painter.with_clip_rect(cell);
            let mid_y = cell.center().y;
            if let Some(kind) = icon.asset_kind() {
                let rgb = crate::icons::asset_tint(kind);
                let chip = egui::Rect::from_center_size(
                    egui::pos2(cell.left() + 4.0, mid_y),
                    egui::vec2(3.0, 16.0),
                );
                cell_painter.rect_filled(chip, 0.0, Color32::from_rgb(rgb[0], rgb[1], rgb[2]));
            }
            crate::icons::paint_icon(
                &cell_painter,
                egui::pos2(cell.left() + 18.0, mid_y),
                icon,
                16.0,
                ink,
            );
            crate::icons::paint_icon(
                &cell_painter,
                egui::pos2(cell.left() + 42.0, mid_y),
                icon,
                24.0,
                ink,
            );
            cell_painter.text(
                egui::pos2(cell.left() + 58.0, mid_y),
                Align2::LEFT_CENTER,
                icon.name(),
                font.clone(),
                dim,
            );
        }
    }

    /// What dropping a content file in the viewport creates (see [`content_drag_kind`]).
    pub fn place_kind_from_label(name: &str) -> Option<String> {
        content_drag_kind(name)
    }

    fn paint_place_ghost(
        painter: &egui::Painter,
        place: &dyn Fn(&GridMark<'_>) -> egui::Rect,
        sx: f32,
        sy: f32,
        kind: &str,
    ) {
        let ghost_rect = place(&GridMark {
            x: sx,
            y: sy,
            half: if kind.contains(':') { [0.5, 0.5] } else { [0.85, 0.85] },
            round: false,
            billboard: None,
            selected: false,
            faint: false,
            label: "",
            tint: [96, 160, 208],
            angle: 0.0,
            depth: 0.0,
            size: [1.0, 1.0, 1.0],
            mesh: (!kind.contains(':')).then(|| crate::viewport_mesh::mesh_for_place_kind(kind)),
            pickable: true,
            sprite: None,
            layer: 0,
            uv: [0.0, 0.0, 1.0, 1.0],
            order: 0,
            sprite_tint: [255; 4],
            slice: [0.0; 4],
            slice_mode: "stretch",
            slice_size: [0.0; 2],
            pivot: [0.5, 0.5],
            normal_map: None,
            shape2d: None,
            sprite_shape: None,
            material: None,
            mask_mode: 0,
            mask_kind: 0,
            surface: crate::gpu_textures::MeshSurface::EMPTY,
        });
        painter.rect_filled(ghost_rect, 0.0, Color32::from_rgba_unmultiplied(120, 185, 255, 165));
        painter.rect_stroke(
            ghost_rect,
            0.0,
            Stroke::new(2.0, Color32::from_rgba_unmultiplied(200, 230, 255, 220)),
            egui::StrokeKind::Outside,
        );
    }

    /// Full-width button with the place icon for `cube`, `sphere`, `plane`, or a dot.
    pub fn shape_button(&mut self, icon: &str, label: &str) -> bool {
        let height = self.ui.spacing().interact_size.y + 6.0;
        let (rect, response) = self.ui.allocate_exact_size(
            egui::vec2(self.ui.available_width(), height),
            Sense::click_and_drag(),
        );
        if response.dragged() {
            response.dnd_set_drag_payload(format!("place:{icon}"));
        }
        let response = help_focus_ring(self.ui, response, label);
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
        let visuals = self.ui.visuals().clone();
        row_fill(self.ui, rect, &response, false, true);
        let painter = self.ui.painter().with_clip_rect(rect);
        let center = egui::pos2(rect.left() + f32::from(PAD) + 9.0, rect.center().y);
        let ink = visuals.strong_text_color();
        crate::icons::paint_icon(
            &painter,
            center,
            crate::icons::place_icon(icon),
            crate::ICON_PX,
            ink,
        );
        painter.text(
            egui::pos2(center.x + 18.0, rect.center().y),
            Align2::LEFT_CENTER,
            label,
            TextStyle::Body.resolve(self.ui.style()),
            visuals.text_color(),
        );
        response.clicked()
    }

    /// Grid viewport. Marks are in scene units on the two axes the view shows.
    /// Drag a mark to move it, right-drag to pan, scroll to zoom, double-click to reset the view.
    #[allow(clippy::too_many_arguments)]
    pub fn grid_view(
        &mut self,
        marks: &[GridMark],
        grid: bool,
        axes: (&str, &str),
        tool: &str,
        menus: GridMenus<'_>,
        camera: &mut ViewportCamera,
        opts: GridViewOptions<'_>,
    ) -> GridHit {
        // Fill only the visible remainder. Asking for `available_size` makes the
        // dock add scroll bars, and the canvas then stays the old size when the split moves.
        let clip = self.ui.clip_rect();
        let max = self.ui.max_rect();
        let cursor = self.ui.cursor().min;
        let rect = egui::Rect::from_min_max(
            cursor,
            egui::pos2(clip.right().min(max.right()), clip.bottom().min(max.bottom())),
        );
        let rect = egui::Rect::from_min_size(
            rect.min,
            egui::vec2(rect.width().max(32.0), rect.height().max(32.0)),
        );
        let response = self.ui.allocate_rect(rect, Sense::click_and_drag());
        if response.clicked() {
            response.request_focus();
        }
        let id = response.id;
        let mut camera_dirty = false;
        let mut camera_speed_hit = None;
        let modifiers = self.ui.input(|input| input.modifiers);
        let alt_held = modifiers.alt;
        let ctrl_held = modifiers.ctrl;
        let pointer_state = self.ui.input(|input| input.pointer.clone());
        let rmb_down = pointer_state.button_down(egui::PointerButton::Secondary);
        let scroll = self.ui.input(|input| input.smooth_scroll_delta.y);

        let pivot_drag_id = id.with("pivot-drag");
        let mut pivot_scene_offset = self
            .ui
            .data(|data| data.get_temp::<PivotDrag>(pivot_drag_id))
            .map(|grab| grab.offset)
            .unwrap_or([0.0, 0.0, 0.0]);
        // Turning the view keeps a chosen world point where it is on screen: the
        // point in the middle of the viewport, or the selection when Alt-orbiting
        // with one. Without this the pan is a plain screen offset and every turn
        // swings the world around 0 0 0, away from where the user moved to.
        let before = *camera;
        let mut orbit_anchor: Option<[f32; 3]> = None;
        if opts.perspective {
            let pivot_grabbing =
                self.ui.data(|data| data.get_temp::<PivotDrag>(pivot_drag_id)).is_some();
            if response.dragged_by(egui::PointerButton::Middle) && !pivot_grabbing {
                camera.pan_x += response.drag_delta().x;
                camera.pan_y += response.drag_delta().y;
                camera_dirty = true;
            }
            if response.dragged_by(egui::PointerButton::Secondary) && !alt_held {
                camera.yaw_deg += response.drag_delta().x * 0.35;
                camera.pitch_deg =
                    (camera.pitch_deg + response.drag_delta().y * 0.35).clamp(-89.0, 89.0);
                camera_dirty = true;
            } else if response.dragged_by(egui::PointerButton::Secondary) && alt_held {
                let delta = response.drag_delta().y;
                camera.distance = (camera.distance * (1.0 - delta * 0.004)).clamp(2.0, 200.0);
                camera_dirty = true;
            }
            let orbit_drag = response.dragged_by(egui::PointerButton::Primary) && alt_held;
            if orbit_drag {
                if let Some((px, py)) = opts.orbit_pivot {
                    orbit_anchor = Some([px, py, 0.0]);
                }
                camera.yaw_deg += response.drag_delta().x * 0.35;
                camera.pitch_deg =
                    (camera.pitch_deg + response.drag_delta().y * 0.35).clamp(-89.0, 89.0);
                camera_dirty = true;
            }
            if rmb_down && response.hovered() {
                let step = opts.camera_speed * self.ui.input(|input| input.stable_dt) * 120.0;
                let input = self.ui.input(|input| input.clone());
                let lift = camera.pitch_deg.to_radians().cos().abs();
                // W/S fly forward and back (closer or farther), A/D strafe, E/Q rise
                // and sink. Moving the camera one way moves the picture the other way.
                for (key, forward, right, up) in [
                    (egui::Key::W, 1.0, 0.0, 0.0),
                    (egui::Key::S, -1.0, 0.0, 0.0),
                    (egui::Key::A, 0.0, -1.0, 0.0),
                    (egui::Key::D, 0.0, 1.0, 0.0),
                    (egui::Key::E, 0.0, 0.0, 1.0),
                    (egui::Key::Q, 0.0, 0.0, -1.0),
                ] {
                    if input.key_down(key) {
                        camera.distance =
                            (camera.distance - forward * step * 0.05).clamp(2.0, 200.0);
                        camera.pan_x -= right * step;
                        camera.pan_y += up * step * lift;
                        camera_dirty = true;
                    }
                }
            }
            if response.hovered() && scroll.abs() > f32::EPSILON {
                if rmb_down {
                    let next = (opts.camera_speed - scroll * 0.01).clamp(1.0, 8.0);
                    if (next - opts.camera_speed).abs() > f32::EPSILON {
                        camera_speed_hit = Some(next);
                    }
                } else {
                    camera.distance = (camera.distance * (1.0 - scroll * 0.002)).clamp(2.0, 200.0);
                    camera_dirty = true;
                }
            }
        } else {
            let pivot_grabbing =
                self.ui.data(|data| data.get_temp::<PivotDrag>(pivot_drag_id)).is_some();
            if (response.dragged_by(egui::PointerButton::Secondary)
                || response.dragged_by(egui::PointerButton::Middle))
                && !pivot_grabbing
            {
                camera.pan_x += response.drag_delta().x;
                camera.pan_y += response.drag_delta().y;
                camera_dirty = true;
            }
            let orbit_drag = response.dragged_by(egui::PointerButton::Primary) && alt_held;
            if orbit_drag {
                if let Some((px, py)) = opts.orbit_pivot {
                    orbit_anchor = Some([px, py, 0.0]);
                }
                camera.yaw_deg += response.drag_delta().x * 0.25;
                camera.pitch_deg =
                    (camera.pitch_deg + response.drag_delta().y * 0.25).clamp(-89.0, 89.0);
                camera_dirty = true;
            }
            if response.hovered() {
                camera.zoom = (camera.zoom * (1.0 + scroll * 0.002)).clamp(0.25, 6.0);
                if scroll.abs() > f32::EPSILON {
                    camera_dirty = true;
                }
            }
        }
        if (camera.yaw_deg - before.yaw_deg).abs() > f32::EPSILON
            || (camera.pitch_deg - before.pitch_deg).abs() > f32::EPSILON
        {
            keep_point_on_screen(camera, &before, rect, opts.perspective, orbit_anchor);
        }
        if response.double_clicked() {
            camera.pan_x = 0.0;
            camera.pan_y = rect.height() * 0.25;
            camera.zoom = 1.0;
            camera.yaw_deg = 0.0;
            camera.pitch_deg = 0.0;
            camera.distance = DEFAULT_ORBIT_DISTANCE;
            camera_dirty = true;
        }

        let visuals = self.ui.visuals().clone();
        let painter = self.ui.painter().with_clip_rect(rect);
        paint_viewport_sky(&painter, rect, &visuals);
        let ink = visuals.text_color();
        let cell = camera_cell(camera, opts.perspective);
        let origin = rect.center() + egui::vec2(camera.pan_x, camera.pan_y);
        let yaw = camera.yaw_deg.to_radians();
        let (sin_yaw, cos_yaw) = yaw.sin_cos();
        let scene_to_screen = |x: f32, y: f32| {
            let rx = x * cos_yaw - y * sin_yaw;
            let ry = x * sin_yaw + y * cos_yaw;
            egui::pos2(origin.x + rx * cell, origin.y - ry * cell)
        };
        let angled_view = opts.perspective;
        let (sin_pitch, cos_pitch) = camera.pitch_deg.to_radians().sin_cos();
        let screen_to_scene = |pos: egui::Pos2| {
            let dx = (pos.x - origin.x) / cell;
            let dy = -(pos.y - origin.y) / cell;
            if angled_view && cos_yaw.abs() > 0.05 && cos_pitch.abs() > 0.05 {
                let x = dx / cos_yaw;
                let y = (dy + x * sin_yaw * sin_pitch) / cos_pitch;
                return (x, y);
            }
            let sx = dx * cos_yaw + dy * sin_yaw;
            let sy = -dx * sin_yaw + dy * cos_yaw;
            (sx, sy)
        };
        let small = TextStyle::Small.resolve(self.ui.style());
        if opts.perspective && crate::gpu_viewport::enabled() && !opts.sky_cubemap.is_empty() {
            painter.add(crate::gpu_viewport::sky_callback(
                rect,
                crate::gpu_viewport::GpuFrame {
                    anisotropy: opts.anisotropy,
                    sky: opts.sky_cubemap.to_string(),
                    project: opts.texture_root.map(|path| path.to_path_buf()),
                    yaw_deg: camera.yaw_deg,
                    pitch_deg: camera.pitch_deg,
                    look_through: crate::viewport_mesh::look_through().is_some(),
                    render_scale: opts.render_scale,
                },
            ));
        }
        if opts.perspective && crate::viewport_mesh::look_through().is_none() {
            paint_angled_grid(
                &painter,
                rect,
                camera.yaw_deg,
                camera.pitch_deg,
                origin,
                cell,
                grid,
                ink,
            );
        } else {
            if grid {
                let first = ((rect.left() - origin.x) / cell).floor() as i32;
                let last = ((rect.right() - origin.x) / cell).ceil() as i32;
                for step in first..=last {
                    let major = step.rem_euclid(10) == 0;
                    let minor = step.rem_euclid(1) == 0;
                    if !major && (!minor || camera.zoom < 0.45) {
                        continue;
                    }
                    let fade = (1.0 - (step.unsigned_abs() as f32 / 40.0).min(1.0)) * 0.5 + 0.5;
                    let strength = if major { 0.18 * fade } else { 0.06 * fade };
                    painter.vline(
                        origin.x + step as f32 * cell,
                        rect.y_range(),
                        Stroke::new(1.0, ink.gamma_multiply(strength)),
                    );
                }
                let first = ((rect.top() - origin.y) / cell).floor() as i32;
                let last = ((rect.bottom() - origin.y) / cell).ceil() as i32;
                for step in first..=last {
                    let major = step.rem_euclid(10) == 0;
                    let minor = step.rem_euclid(1) == 0;
                    if !major && (!minor || camera.zoom < 0.45) {
                        continue;
                    }
                    let fade = (1.0 - (step.unsigned_abs() as f32 / 40.0).min(1.0)) * 0.5 + 0.5;
                    let strength = if major { 0.18 * fade } else { 0.06 * fade };
                    painter.hline(
                        rect.x_range(),
                        origin.y + step as f32 * cell,
                        Stroke::new(1.0, ink.gamma_multiply(strength)),
                    );
                }
            }
            let across = crate::AXIS_X;
            let up = crate::AXIS_Y;
            if !opts.game_view {
                painter.hline(rect.x_range(), origin.y, Stroke::new(1.5, across));
                painter.vline(origin.x, rect.y_range(), Stroke::new(1.5, up));
                painter.text(
                    egui::pos2(rect.right() - 8.0, origin.y - 4.0),
                    Align2::RIGHT_BOTTOM,
                    axes.0,
                    small.clone(),
                    across,
                );
                painter.text(
                    egui::pos2(origin.x + 6.0, rect.top() + 6.0),
                    Align2::LEFT_TOP,
                    axes.1,
                    small.clone(),
                    up,
                );
            }
        }

        // In the angled view every mark sits where its 3D position projects, and solid
        // shapes use the outline of their projected box; flat views keep the 2D layout.
        let angled = opts.perspective;
        let hull = |mark: &GridMark| -> Option<Vec<egui::Pos2>> {
            if !angled {
                return None;
            }
            crate::viewport_mesh::mark_hull(mark, camera.yaw_deg, camera.pitch_deg, origin, cell)
        };
        let pivot_shift = |mark: &GridMark| -> (f32, f32) {
            if mark.sprite.is_none() {
                return (0.0, 0.0);
            }
            let shift = seal_editor_core::sprite_pivot::card_shift(
                mark.pivot,
                [mark.half[0] * 2.0, mark.half[1] * 2.0],
            );
            (shift[0], shift[1])
        };
        let card_pos = |mark: &GridMark| -> (f32, f32) {
            let (sx, sy) = pivot_shift(mark);
            (mark.x + sx, mark.y + sy)
        };
        let place = |mark: &GridMark| {
            if let Some(points) = hull(mark) {
                return egui::Rect::from_points(&points);
            }
            let (cx, cy) = card_pos(mark);
            let point = if angled {
                crate::viewport_mesh::project_point(
                    [cx, cy, mark.depth],
                    camera.yaw_deg,
                    camera.pitch_deg,
                    origin,
                    cell,
                )
                .0
            } else {
                scene_to_screen(cx, cy)
            };
            // A thin slab still gets two pixels so the ground stays visible.
            let size = egui::vec2(
                (mark.half[0] * 2.0 * cell).max(2.0),
                (mark.half[1] * 2.0 * cell).max(2.0),
            );
            egui::Rect::from_center_size(point, size)
        };
        // A sprite is a flat card standing in the scene. In the angled view all four
        // corners are projected, so neighbouring tiles stay joined and tilt together
        // instead of each tile being moved but still drawn facing the camera.
        let sprite_quad = |mark: &GridMark| -> [egui::Pos2; 4] {
            let plain = || {
                if angled {
                    let (hx, hy) = (mark.half[0], mark.half[1]);
                    let (cx, cy) = card_pos(mark);
                    let corner = |dx: f32, dy: f32| {
                        crate::viewport_mesh::project_point(
                            [cx + dx, cy + dy, mark.depth],
                            camera.yaw_deg,
                            camera.pitch_deg,
                            origin,
                            cell,
                        )
                        .0
                    };
                    [corner(-hx, hy), corner(hx, hy), corner(hx, -hy), corner(-hx, -hy)]
                } else {
                    let r = place(mark);
                    [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()]
                }
            };
            let Some(material) = mark.material.as_ref() else {
                return plain();
            };
            if material.effect != "custom" || material.shader.is_empty() {
                return plain();
            }
            let Some(program) = seal_editor_core::material2d::cached_program(&material.shader)
            else {
                return plain();
            };
            if !program.has_vertex() {
                return plain();
            }
            let (hx, hy) = (mark.half[0], mark.half[1]);
            let (cx, cy) = card_pos(mark);
            let time = self.ui.input(|input| input.time) as f32;
            let corner = |dx: f32, dy: f32, u: f32, v: f32| {
                let [x, y] = program.vertex_at(
                    cx + dx,
                    cy + dy,
                    u,
                    v,
                    time,
                    material.amount,
                    material.speed,
                );
                if angled {
                    crate::viewport_mesh::project_point(
                        [x, y, mark.depth],
                        camera.yaw_deg,
                        camera.pitch_deg,
                        origin,
                        cell,
                    )
                    .0
                } else {
                    scene_to_screen(x, y)
                }
            };
            [
                corner(-hx, hy, 0.0, 0.0),
                corner(hx, hy, 1.0, 0.0),
                corner(hx, -hy, 1.0, 1.0),
                corner(-hx, -hy, 0.0, 1.0),
            ]
        };
        let hits = |mark: &GridMark, at: egui::Pos2| {
            if angled && mark.sprite.is_some() {
                return crate::viewport_mesh::hull_contains(&sprite_quad(mark), at, 2.0);
            }
            match hull(mark) {
                Some(points) => crate::viewport_mesh::hull_contains(&points, at, 4.0),
                None => place(mark).expand(4.0).contains(at),
            }
        };
        let near_depth = |mark: &GridMark| {
            if angled {
                crate::viewport_mesh::view_depth(
                    [mark.x, mark.y, mark.depth],
                    camera.yaw_deg,
                    camera.pitch_deg,
                )
            } else {
                mark.depth
            }
        };
        let outline = |painter: &egui::Painter, mark: &GridMark, grow: f32, stroke: Stroke| {
            match hull(mark) {
                Some(points) if points.len() >= 3 => {
                    let c = points.iter().fold(egui::Vec2::ZERO, |acc, p| acc + p.to_vec2())
                        / points.len() as f32;
                    let grown: Vec<egui::Pos2> = points
                        .iter()
                        .map(|p| {
                            let d = *p - c.to_pos2();
                            *p + d.normalized() * grow
                        })
                        .collect();
                    painter.add(egui::Shape::closed_line(grown, stroke));
                }
                _ => {
                    painter.rect_stroke(
                        place(mark).expand(grow),
                        0.0,
                        stroke,
                        egui::StrokeKind::Outside,
                    );
                }
            }
        };
        let pointer = response.interact_pointer_pos();
        let hover_index = pointer.and_then(|at| {
            marks
                .iter()
                .enumerate()
                .filter(|(_, mark)| hits(mark, at))
                .min_by(|left, right| {
                    place(left.1).area().partial_cmp(&place(right.1).area()).unwrap()
                })
                .map(|(index, _)| index)
        });
        let selected_outline = Color32::from_rgb(0xff, 0x8c, 0x2a);
        let draw_wire = opts.wireframe || matches!(opts.shade, "wireframe" | "collision");
        let overdraw_counts = if opts.shade == "overdraw" && !angled {
            let rects: Vec<seal_editor_core::viewport_selection::MarkRect> = marks
                .iter()
                .map(|m| seal_editor_core::viewport_selection::MarkRect {
                    x: m.x,
                    y: m.y,
                    half_w: m.half[0],
                    half_h: m.half[1],
                    depth: m.depth,
                    pickable: m.pickable,
                })
                .collect();
            Some(seal_editor_core::viewport_selection::overdraw_counts(&rects))
        } else {
            None
        };
        let buffer_shade = seal_editor_core::viewport_buffer::is_buffer_mode(opts.shade);
        let use_lit_mesh = opts.perspective && (opts.shade == "lit" || buffer_shade) && !draw_wire;
        if use_lit_mesh {
            let gpu = crate::gpu_viewport::enabled();
            let mut scene = crate::viewport_mesh::collect_lit_scene_gi(
                marks,
                camera.yaw_deg,
                camera.pitch_deg,
                origin,
                cell,
                opts.light_dir,
                opts.shadows,
                opts.shadow_map_size,
                opts.fill_lights,
                gpu,
                opts.gi,
                opts.light_gain,
            );
            if buffer_shade {
                crate::viewport_mesh::apply_buffer_view(&mut scene.tris, opts.shade);
            }
            if gpu {
                let _channel = seal_editor_core::viewport_buffer::gpu_debug_channel(opts.shade);
                painter.add(crate::gpu_viewport::paint_callback(
                    rect,
                    &scene.tris,
                    scene.shadow.as_ref(),
                    crate::gpu_viewport::GpuFrame {
                        anisotropy: opts.anisotropy,
                        sky: String::new(),
                        project: opts.texture_root.map(|path| path.to_path_buf()),
                        yaw_deg: camera.yaw_deg,
                        pitch_deg: camera.pitch_deg,
                        look_through: crate::viewport_mesh::look_through().is_some(),
                        render_scale: opts.render_scale,
                    },
                ));
            } else {
                // One plain mesh for all faces: per-triangle anti-aliased polygons leave
                // seams between pieces and long spikes on thin sliver faces.
                painter.add(egui::Shape::mesh(crate::viewport_mesh::lit_mesh(&scene.tris)));
            }
        }
        let collision_ink = visuals.warn_fg_color;
        let plane_at = |sx: f32, sy: f32| {
            let rx = (sx - origin.x) / cell.max(1e-4);
            let ry = (origin.y - sy) / cell.max(1e-4);
            [rx * cos_yaw + ry * sin_yaw, -rx * sin_yaw + ry * cos_yaw]
        };
        // Sprites draw back to front, after the lit shapes.
        let sprite_order: Vec<usize> = if let Some(custom) = opts.sprite_order {
            custom.to_vec()
        } else {
            let mut sprite_order: Vec<usize> = (0..marks.len())
                .filter(|&i| {
                    marks[i].sprite.is_some()
                        || marks[i].shape2d.is_some()
                        || marks[i].sprite_shape.is_some()
                        || marks[i].material.as_ref().is_some_and(|m| {
                            crate::sprite_blend::is_special_blend(m.blend.as_str())
                        })
                })
                .collect();
            sprite_order.sort_by(|&a, &b| {
                marks[a].layer.cmp(&marks[b].layer).then(marks[a].order.cmp(&marks[b].order)).then(
                    marks[b]
                        .depth
                        .partial_cmp(&marks[a].depth)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
            });
            sprite_order
        };
        let mut blend_pass =
            crate::sprite_blend::SpriteBlendPass::new(rect, crate::gpu_viewport::enabled());
        for &index in &sprite_order {
            let mark = &marks[index];
            let shape_blend = mark.material.as_ref().map(|m| m.blend.as_str()).unwrap_or("alpha");
            if crate::sprite_blend::is_alpha_blend(shape_blend) {
                blend_pass.flush_alpha_barrier(&painter);
            }
            if let Some(shape) = &mark.shape2d {
                let to_screen = |p: [f32; 2]| {
                    if angled {
                        crate::viewport_mesh::project_point(
                            [p[0], p[1], mark.depth],
                            camera.yaw_deg,
                            camera.pitch_deg,
                            origin,
                            cell,
                        )
                        .0
                    } else {
                        scene_to_screen(p[0], p[1])
                    }
                };
                let pts: Vec<egui::Pos2> = shape.points.iter().map(|p| to_screen(*p)).collect();
                let [r, g, b, a] = shape.color;
                let ink = Color32::from_rgba_unmultiplied(r, g, b, a);
                if shape.filled {
                    let mut mesh = egui::Mesh::default();
                    let placed = if !shape.texture.is_empty() && shape.uvs.len() == pts.len() {
                        crate::sprites::sprite_texture(painter.ctx(), &shape.texture)
                    } else {
                        None
                    };
                    if let Some((texture, place, _)) = placed {
                        mesh.texture_id = texture;
                        for (p, uv) in pts.iter().zip(&shape.uvs) {
                            let u = place[0] + uv[0] * (place[2] - place[0]);
                            let v = place[1] + uv[1] * (place[3] - place[1]);
                            mesh.vertices.push(egui::epaint::Vertex {
                                pos: *p,
                                uv: egui::pos2(u, v),
                                color: ink,
                            });
                        }
                    } else {
                        for p in &pts {
                            mesh.colored_vertex(*p, ink);
                        }
                    }
                    for t in seal_editor_core::shape2d::triangulate(&shape.points) {
                        mesh.add_triangle(t[0] as u32, t[1] as u32, t[2] as u32);
                    }
                    crate::sprite_blend::paint_blended_mesh(
                        &mut blend_pass,
                        &painter,
                        shape_blend,
                        &mesh,
                        angled,
                    );
                } else if !shape.ribbon.is_empty() {
                    let mut mesh = egui::Mesh::default();
                    for (p, color) in &shape.ribbon {
                        let [r, g, b, a] = *color;
                        mesh.colored_vertex(
                            to_screen(*p),
                            Color32::from_rgba_unmultiplied(r, g, b, a),
                        );
                    }
                    for t in &shape.ribbon_tris {
                        if t.iter().all(|i| *i < mesh.vertices.len()) {
                            mesh.add_triangle(t[0] as u32, t[1] as u32, t[2] as u32);
                        }
                    }
                    crate::sprite_blend::paint_blended_mesh(
                        &mut blend_pass,
                        &painter,
                        shape_blend,
                        &mesh,
                        angled,
                    );
                } else if pts.len() >= 2 {
                    let stroke = Stroke::new((shape.width * cell).max(1.0), ink);
                    if shape.closed && pts.len() > 2 {
                        painter.add(egui::Shape::closed_line(pts.clone(), stroke));
                    } else {
                        painter.add(egui::Shape::line(pts.clone(), stroke));
                    }
                }
                if shape.edit_points && !angled {
                    crate::poly_edit::paint(&painter, &shape.points, &|x, y| scene_to_screen(x, y));
                }
            }
            if let Some(shape) = &mark.sprite_shape {
                crate::sprite_shape::paint_body(&painter, painter.ctx(), shape, &scene_to_screen);
            }
            let loaded = mark
                .sprite
                .as_deref()
                .and_then(|path| crate::sprites::sprite_texture(painter.ctx(), path));
            let Some((mut texture, mut place, mut picture)) = loaded else {
                if mark
                    .material
                    .as_ref()
                    .is_some_and(|m| crate::sprite_blend::is_special_blend(m.blend.as_str()))
                {
                    let blend = mark.material.as_ref().map(|m| m.blend.as_str()).unwrap_or("alpha");
                    let corners = sprite_quad(mark);
                    let color = Color32::from_rgba_unmultiplied(
                        mark.sprite_tint[0],
                        mark.sprite_tint[1],
                        mark.sprite_tint[2],
                        mark.sprite_tint[3],
                    );
                    let uv = crate::sprite_blend::solid_uv();
                    crate::sprite_blend::paint_blended_quad_uv(
                        &mut blend_pass,
                        &painter,
                        blend,
                        crate::sprite_blend::solid_texture(),
                        corners,
                        [uv[0], uv[1], uv[0], uv[1]],
                        color,
                        angled,
                    );
                }
                continue;
            };
            let mut shaded = false;
            if let (Some(material), Some(path)) = (&mark.material, mark.sprite.as_deref()) {
                if let Some(texture_id) =
                    crate::sprites::material_texture(painter.ctx(), path, material)
                {
                    texture = texture_id;
                    place = [0.0, 0.0, 1.0, 1.0];
                    shaded = true;
                }
            }
            let mut uv = crate::sprites::uv_within(place, mark.uv);
            // Sprites sit exactly where picking, outlines and the gizmo place the object.
            let mut corners = sprite_quad(mark);
            let screen_area = {
                let w = (corners[1].x - corners[0].x).abs();
                let h = (corners[3].y - corners[0].y).abs();
                w * h
            };
            if let Some(path) = mark.sprite.as_deref() {
                crate::sprites::note_sprite_screen_coverage(
                    painter.ctx(),
                    path,
                    screen_area,
                    picture[0].round().max(1.0) as u32,
                    picture[1].round().max(1.0) as u32,
                );
            }
            let mut corner_uvs = None;
            if let Some(path) = mark.sprite.as_deref().filter(|_| !shaded) {
                if let Some(fit) = crate::sprites::fit_sprite(path, corners, mark.uv) {
                    corners = fit.corners;
                    uv = fit.uv;
                    picture = fit.content_px;
                    place = fit.uv;
                    if fit.turned {
                        corner_uvs = Some(fit.uvs);
                    }
                }
            }
            let [r, g, b, a] = mark.sprite_tint;
            let material = mark.material.as_ref();
            let unlit = material
                .is_some_and(|item| matches!(item.light_mode.as_str(), "unlit" | "unshaded"));
            let light_only = material.is_some_and(|item| {
                matches!(item.light_mode.as_str(), "light" | "light-only" | "light_only")
            });
            let blend = material.map(|item| item.blend.as_str()).unwrap_or("alpha");
            if crate::sprite_blend::is_alpha_blend(blend) {
                blend_pass.flush_alpha_barrier(&painter);
            }
            let program = material
                .filter(|item| item.effect == "custom")
                .and_then(|item| seal_editor_core::material2d::cached_program(&item.shader));
            let custom_light = program.as_ref().is_some_and(|item| item.has_light());
            if let Some(glow) = opts.glow {
                if !angled {
                    let luma = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
                    if luma > glow[0] * 255.0 {
                        let (cx, cy) = card_pos(mark);
                        let at = scene_to_screen(cx, cy);
                        let radius = (glow[2] * cell).max(4.0);
                        let alpha = ((luma / 255.0 - glow[0]) * glow[1]).clamp(0.0, 1.0);
                        painter.circle_filled(
                            at,
                            radius,
                            Color32::from_rgba_unmultiplied(r, g, b, (alpha * 90.0) as u8),
                        );
                    }
                }
            }
            if mark.mask_mode != 0 && !angled && !opts.masks2d.is_empty() {
                let (s, c) = mark.angle.sin_cos();
                let mid = scene_to_screen(mark.x, mark.y);
                let corner = |dx: f32, dy: f32| {
                    let rx = dx * c - dy * s;
                    let ry = dx * s + dy * c;
                    egui::pos2(mid.x + rx * cell, mid.y - ry * cell)
                };
                let (hx, hy) = (mark.half[0], mark.half[1]);
                let masked = if mark.angle.abs() > 0.01 {
                    [corner(-hx, hy), corner(hx, hy), corner(hx, -hy), corner(-hx, -hy)]
                } else {
                    corners
                };
                let mask_key = format!(
                    "{}:{:.3}:{:.3}:{}:{}:{}",
                    mark.layer,
                    mark.x,
                    mark.y,
                    mark.order,
                    mark.mask_mode,
                    mark.sprite.as_deref().unwrap_or("")
                );
                if crate::sprite_blend::is_special_blend(blend) {
                    if let Some((mask_tex, mask_corners)) = crate::sprite_mask::clipped_quad(
                        painter.ctx(),
                        rect,
                        masked,
                        uv,
                        [r, g, b, a],
                        mark.sprite.as_deref(),
                        plane_at,
                        opts.masks2d,
                        mark.layer,
                        mark.order,
                        mark.mask_mode,
                        &mask_key,
                    ) {
                        crate::sprite_blend::paint_blended_quad_uv(
                            &mut blend_pass,
                            &painter,
                            blend,
                            mask_tex,
                            mask_corners,
                            [0.0, 0.0, 1.0, 1.0],
                            Color32::WHITE,
                            angled,
                        );
                        continue;
                    }
                } else if crate::sprite_mask::paint_clipped(
                    &painter,
                    rect,
                    masked,
                    uv,
                    [r, g, b, a],
                    mark.sprite.as_deref(),
                    plane_at,
                    opts.masks2d,
                    mark.layer,
                    mark.order,
                    mark.mask_mode,
                    &mask_key,
                ) {
                    continue;
                }
            }
            let mut color = if opts.shade == "lighting_only" {
                Color32::from_rgba_unmultiplied(255, 255, 255, a)
            } else {
                Color32::from_rgba_unmultiplied(r, g, b, a)
            };
            if mark.slice != [0.0; 4] && corner_uvs.is_none() {
                if !opts.lights2d.is_empty() && !angled && !unlit {
                    // Nine-slice pieces take the light at the sprite's centre.
                    let l = seal_editor_core::light2d::light_at(
                        [card_pos(mark).0, card_pos(mark).1],
                        None,
                        opts.lights2d,
                        opts.blockers2d,
                        opts.ambient2d,
                    );
                    let scale = |c: u8, k: f32| (c as f32 * k.min(1.0)).round() as u8;
                    color = Color32::from_rgba_unmultiplied(
                        scale(r, l[0]),
                        scale(g, l[1]),
                        scale(b, l[2]),
                        a,
                    );
                }
                // Pixel size of the whole texture the rectangle is measured on.
                let texture_px = [
                    picture[0] / (place[2] - place[0]).max(1e-6),
                    picture[1] / (place[3] - place[1]).max(1e-6),
                ];
                let nine = crate::sprites::NineSlice {
                    border: mark.slice,
                    picture: texture_px,
                    scale: [2.0 * mark.half[0].abs(), 2.0 * mark.half[1].abs()],
                };
                let mesh = if mark.slice_mode == "tile" {
                    crate::sprites::build_nine_tile_mesh(texture, corners, uv, color, nine)
                } else {
                    crate::sprites::build_nine_mesh(texture, corners, uv, color, nine)
                };
                crate::sprite_blend::paint_blended_mesh(
                    &mut blend_pass,
                    &painter,
                    blend,
                    &mesh,
                    angled,
                );
            } else if !opts.lights2d.is_empty() && !angled && !unlit {
                let normals = mark.normal_map.as_deref().and_then(crate::sprites::normal_pixels);
                let (cx, cy) = card_pos(mark);
                let (x0, y0) = (cx - mark.half[0], cy + mark.half[1]);
                let (w, h) = (2.0 * mark.half[0], 2.0 * mark.half[1]);
                let tex_uv = mark.uv;
                if light_only && !custom_light {
                    let l = seal_editor_core::light2d::light_at(
                        [cx, cy],
                        None,
                        opts.lights2d,
                        opts.blockers2d,
                        opts.ambient2d,
                    );
                    let byte = |k: f32| (k.clamp(0.0, 1.0) * 255.0).round() as u8;
                    let color =
                        Color32::from_rgba_unmultiplied(byte(l[0]), byte(l[1]), byte(l[2]), a);
                    if let Some(uvs) = corner_uvs {
                        let mesh =
                            crate::sprites::build_quad_uvs_mesh(texture, corners, uvs, color);
                        crate::sprite_blend::paint_blended_mesh(
                            &mut blend_pass,
                            &painter,
                            blend,
                            &mesh,
                            angled,
                        );
                    } else if let Some(path) = mark.sprite.as_deref() {
                        if let Some(mesh) =
                            crate::sprites::build_tight_mesh(path, texture, corners, uv, color)
                        {
                            crate::sprite_blend::paint_blended_mesh(
                                &mut blend_pass,
                                &painter,
                                blend,
                                &mesh,
                                angled,
                            );
                        } else {
                            crate::sprite_blend::paint_blended_quad_uv(
                                &mut blend_pass,
                                &painter,
                                blend,
                                texture,
                                corners,
                                uv,
                                color,
                                angled,
                            );
                        }
                    } else {
                        crate::sprite_blend::paint_blended_quad_uv(
                            &mut blend_pass,
                            &painter,
                            blend,
                            texture,
                            corners,
                            uv,
                            color,
                            angled,
                        );
                    }
                } else {
                    let paint_color = if custom_light {
                        Color32::from_rgba_unmultiplied(255, 255, 255, a)
                    } else {
                        color
                    };
                    let light_at = |s: f32, t: f32| {
                        let normal = normals.as_ref().map(|px| {
                            let n = crate::sprites::sample_normal(
                                px,
                                [
                                    tex_uv[0] + (tex_uv[2] - tex_uv[0]) * s,
                                    tex_uv[1] + (tex_uv[3] - tex_uv[1]) * t,
                                ],
                            );
                            [
                                if tex_uv[2] < tex_uv[0] { -n[0] } else { n[0] },
                                if tex_uv[3] < tex_uv[1] { -n[1] } else { n[1] },
                                n[2],
                            ]
                        });
                        let at = [x0 + w * s, y0 - h * t];
                        if custom_light {
                            if let Some(program) = program.as_ref() {
                                let albedo = [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0];
                                return seal_editor_core::light2d::light_respond(
                                    at,
                                    normal,
                                    opts.lights2d,
                                    opts.blockers2d,
                                    opts.ambient2d,
                                    albedo,
                                    &|surface, color, amount, n, dot| {
                                        program.light_at_surface(surface, color, amount, n, dot)
                                    },
                                );
                            }
                        }
                        seal_editor_core::light2d::light_at(
                            at,
                            normal,
                            opts.lights2d,
                            opts.blockers2d,
                            opts.ambient2d,
                        )
                    };
                    if let Some(uvs) = corner_uvs {
                        let mesh = crate::sprites::build_lit_quad_mesh(
                            texture,
                            corners,
                            uvs,
                            paint_color,
                            light_at,
                        );
                        crate::sprite_blend::paint_blended_mesh(
                            &mut blend_pass,
                            &painter,
                            blend,
                            &mesh,
                            angled,
                        );
                    } else if let Some(path) = mark.sprite.as_deref() {
                        if let Some(mesh) = crate::sprites::build_tight_mesh(
                            path,
                            texture,
                            corners,
                            uv,
                            paint_color,
                        ) {
                            crate::sprite_blend::paint_blended_mesh(
                                &mut blend_pass,
                                &painter,
                                blend,
                                &mesh,
                                angled,
                            );
                        } else {
                            let mesh = crate::sprites::build_lit_quad_mesh(
                                texture,
                                corners,
                                [
                                    egui::pos2(uv[0], uv[1]),
                                    egui::pos2(uv[2], uv[1]),
                                    egui::pos2(uv[2], uv[3]),
                                    egui::pos2(uv[0], uv[3]),
                                ],
                                paint_color,
                                light_at,
                            );
                            crate::sprite_blend::paint_blended_mesh(
                                &mut blend_pass,
                                &painter,
                                blend,
                                &mesh,
                                angled,
                            );
                        }
                    }
                }
            } else if let Some(uvs) = corner_uvs {
                let mesh = crate::sprites::build_quad_uvs_mesh(texture, corners, uvs, color);
                crate::sprite_blend::paint_blended_mesh(
                    &mut blend_pass,
                    &painter,
                    blend,
                    &mesh,
                    angled,
                );
            } else if let Some(path) = mark.sprite.as_deref() {
                if let Some(mesh) =
                    crate::sprites::build_tight_mesh(path, texture, corners, uv, color)
                {
                    crate::sprite_blend::paint_blended_mesh(
                        &mut blend_pass,
                        &painter,
                        blend,
                        &mesh,
                        angled,
                    );
                } else {
                    crate::sprite_blend::paint_blended_quad_uv(
                        &mut blend_pass,
                        &painter,
                        blend,
                        texture,
                        corners,
                        uv,
                        color,
                        angled,
                    );
                }
            } else {
                crate::sprite_blend::paint_blended_quad_uv(
                    &mut blend_pass,
                    &painter,
                    blend,
                    texture,
                    corners,
                    uv,
                    color,
                    angled,
                );
            }
        }
        blend_pass.finish(&painter);
        let looking_through = crate::viewport_mesh::look_through().is_some();
        for (index, mark) in marks.iter().enumerate() {
            // Through the play camera only the lit shapes and sprites show, as in the game:
            // editor cards, icons and handles belong to the angled editor view.
            if looking_through && (mark.mesh.is_none() || mark.faint) && mark.sprite.is_none() {
                continue;
            }
            if let Some(shape) = &mark.sprite_shape {
                let selected = self
                    .ui
                    .data(|data| data.get_temp::<(usize, usize)>(id.with("spline-select")))
                    .filter(|(mark_i, _)| *mark_i == index)
                    .map(|(_, point)| point);
                crate::sprite_shape::paint_handles(&painter, shape, selected, &scene_to_screen);
                continue;
            }
            let shape = place(mark);
            if mark.mask_kind != 0 && !angled {
                let selected_now = mark.selected || Some(index) == hover_index;
                let ink = if mark.selected {
                    selected_outline
                } else if selected_now {
                    Color32::WHITE.gamma_multiply(0.85)
                } else {
                    Color32::from_rgb(mark.tint[0], mark.tint[1], mark.tint[2]).gamma_multiply(0.55)
                };
                let fill = (mark.sprite.is_none()).then(|| {
                    Color32::from_rgba_unmultiplied(mark.tint[0], mark.tint[1], mark.tint[2], 40)
                });
                crate::sprite_mask::paint_outline(
                    &painter,
                    [mark.x, mark.y],
                    mark.angle,
                    mark.half,
                    mark.mask_kind,
                    fill,
                    Stroke::new(if mark.selected { 2.0 } else { 1.0 }, ink),
                    scene_to_screen,
                );
                if mark.sprite.is_none() {
                    continue;
                }
            }
            let tint = if let Some(counts) = &overdraw_counts {
                let rgb = seal_editor_core::viewport_selection::overdraw_tint(
                    counts.get(index).copied().unwrap_or(1),
                );
                Color32::from_rgb(rgb[0], rgb[1], rgb[2])
            } else {
                Color32::from_rgb(mark.tint[0], mark.tint[1], mark.tint[2])
            };
            if mark.sprite.is_some() {
                if mark.selected || Some(index) == hover_index {
                    let (width, ink) = if mark.selected {
                        (2.0, selected_outline)
                    } else {
                        (1.0, Color32::WHITE.gamma_multiply(0.85))
                    };
                    // Drawn inside the sprite's own square: tiles touch their
                    // neighbours, so a padded outline would spill onto the next tile
                    // and look shifted and oversized.
                    if angled {
                        let quad = sprite_quad(mark).to_vec();
                        painter.add(egui::Shape::closed_line(quad, Stroke::new(width, ink)));
                    } else {
                        painter.rect_stroke(
                            shape,
                            0.0,
                            Stroke::new(width, ink),
                            egui::StrokeKind::Inside,
                        );
                    }
                }
                continue;
            }
            if use_lit_mesh && mark.mesh.is_some() && !mark.faint {
                if mark.selected {
                    outline(&painter, mark, 3.0, Stroke::new(2.0, selected_outline));
                } else if Some(index) == hover_index {
                    outline(
                        &painter,
                        mark,
                        2.0,
                        Stroke::new(1.0, Color32::WHITE.gamma_multiply(0.85)),
                    );
                }
                if opts.show_names {
                    let shape = place(mark);
                    painter.text(
                        egui::pos2(shape.center().x, shape.top() - 4.0),
                        Align2::CENTER_BOTTOM,
                        mark.label,
                        small.clone(),
                        if mark.selected {
                            visuals.strong_text_color()
                        } else {
                            visuals.weak_text_color()
                        },
                    );
                }
                continue;
            }
            if let Some(icon) = mark.billboard {
                let ink =
                    if mark.selected { visuals.strong_text_color() } else { visuals.text_color() };
                crate::icons::paint_icon(&painter, shape.center(), icon, 24.0, ink);
                if mark.selected {
                    painter.circle_stroke(shape.center(), 14.0, Stroke::new(2.0, selected_outline));
                } else if Some(index) == hover_index {
                    painter.circle_stroke(
                        shape.center(),
                        13.0,
                        Stroke::new(1.0, Color32::WHITE.gamma_multiply(0.85)),
                    );
                }
                if mark.selected {
                    if icon == crate::icons::Icon::EntityCamera {
                        let c = shape.center();
                        painter.line_segment(
                            [c, c + egui::vec2(0.0, -36.0)],
                            Stroke::new(1.0, visuals.weak_text_color()),
                        );
                        painter.line_segment(
                            [c + egui::vec2(-14.0, -36.0), c + egui::vec2(14.0, -36.0)],
                            Stroke::new(1.0, visuals.weak_text_color()),
                        );
                    }
                    if icon == crate::icons::Icon::EntityLightDirectional {
                        let c = shape.center();
                        painter.line_segment(
                            [c, c + egui::vec2(0.0, 30.0)],
                            Stroke::new(1.0, crate::AXIS_Y),
                        );
                    }
                }
                continue;
            }
            let stroke_ink = if opts.shade == "collision" {
                collision_ink
            } else if mark.selected {
                tint
            } else {
                tint.gamma_multiply(0.9)
            };
            if draw_wire {
                if mark.round {
                    painter.circle_stroke(
                        shape.center(),
                        shape.width().min(shape.height()) / 2.0,
                        Stroke::new(1.5, stroke_ink),
                    );
                } else if mark.angle.abs() > 0.01 {
                    let (sin, cos) = mark.angle.sin_cos();
                    let center = shape.center();
                    let hx = shape.width() / 2.0;
                    let hy = shape.height() / 2.0;
                    let corners = [(-hx, -hy), (hx, -hy), (hx, hy), (-hx, hy)].map(|(x, y)| {
                        egui::pos2(center.x + x * cos - y * sin, center.y + x * sin + y * cos)
                    });
                    painter.add(egui::Shape::closed_line(
                        corners.to_vec(),
                        Stroke::new(1.5, stroke_ink),
                    ));
                } else {
                    painter.rect_stroke(
                        shape,
                        0.0,
                        Stroke::new(1.5, stroke_ink),
                        egui::StrokeKind::Outside,
                    );
                }
            } else {
                let fill = if mark.faint {
                    tint.gamma_multiply(0.18)
                } else if mark.selected || opts.shade == "unlit" || opts.shade == "lighting_only" {
                    tint
                } else {
                    tint.gamma_multiply(0.9)
                };
                if mark.round {
                    painter.circle_filled(
                        shape.center(),
                        shape.width().min(shape.height()) / 2.0,
                        fill,
                    );
                } else if mark.angle.abs() > 0.01 {
                    let (sin, cos) = mark.angle.sin_cos();
                    let center = shape.center();
                    let hx = shape.width() / 2.0;
                    let hy = shape.height() / 2.0;
                    let corners = [(-hx, -hy), (hx, -hy), (hx, hy), (-hx, hy)].map(|(x, y)| {
                        egui::pos2(center.x + x * cos - y * sin, center.y + x * sin + y * cos)
                    });
                    painter.add(egui::Shape::convex_polygon(corners.to_vec(), fill, Stroke::NONE));
                } else {
                    painter.rect_filled(shape, 0.0, fill);
                }
            }
            if mark.selected {
                painter.rect_stroke(
                    shape.expand(3.0),
                    0.0,
                    Stroke::new(2.0, selected_outline),
                    egui::StrokeKind::Outside,
                );
            } else if Some(index) == hover_index {
                painter.rect_stroke(
                    shape.expand(2.0),
                    0.0,
                    Stroke::new(1.0, Color32::WHITE.gamma_multiply(0.85)),
                    egui::StrokeKind::Outside,
                );
            }
            if opts.show_names {
                painter.text(
                    egui::pos2(shape.center().x, shape.top() - 4.0),
                    Align2::CENTER_BOTTOM,
                    mark.label,
                    small.clone(),
                    if mark.selected {
                        visuals.strong_text_color()
                    } else {
                        visuals.weak_text_color()
                    },
                );
            }
        }
        if response.drag_started_by(egui::PointerButton::Middle) && tool == "move" {
            if let (Some(at), Some(ix)) = (pointer, opts.gizmo_pivot) {
                if ix < marks.len() {
                    let base = place(&marks[ix]).center();
                    if at.distance(base) < 32.0 && opts.gizmo_world.is_some() {
                        self.ui.data_mut(|data| {
                            data.insert_temp(pivot_drag_id, PivotDrag { offset: [0.0, 0.0, 0.0] });
                        });
                    }
                }
            }
        }
        if let Some(mut grab) = self.ui.data(|data| data.get_temp::<PivotDrag>(pivot_drag_id)) {
            if response.dragged_by(egui::PointerButton::Middle) {
                let delta = response.drag_delta();
                let rdx = delta.x / cell;
                let rdy = -delta.y / cell;
                grab.offset[0] += rdx * cos_yaw + rdy * sin_yaw;
                grab.offset[1] += -rdx * sin_yaw + rdy * cos_yaw;
                pivot_scene_offset = grab.offset;
                self.ui.data_mut(|data| data.insert_temp(pivot_drag_id, grab));
            }
        }
        if response.drag_stopped_by(egui::PointerButton::Middle) {
            self.ui.data_mut(|data| data.remove::<PivotDrag>(pivot_drag_id));
        }
        let pivot_at = |ix: usize, scene_off: [f32; 3]| -> egui::Pos2 {
            let base = place(&marks[ix]).center();
            if scene_off[0].abs() < f32::EPSILON
                && scene_off[1].abs() < f32::EPSILON
                && scene_off[2].abs() < f32::EPSILON
            {
                return base;
            }
            let dx = scene_off[0] * cos_yaw - scene_off[1] * sin_yaw;
            let dy = scene_off[0] * sin_yaw + scene_off[1] * cos_yaw;
            base + egui::vec2(dx * cell, -dy * cell)
        };
        // Axis lines stay readable over sprites and solid shapes: a see-through copy
        // drawn on top of the scene, under the gizmos.
        if opts.axes_on_top && !opts.game_view {
            const ON_TOP: f32 = 0.45;
            if opts.perspective {
                let reach = ((rect.width().hypot(rect.height()) / cell.max(0.01))
                    / camera.pitch_deg.to_radians().sin().abs().max(0.12))
                .clamp(10.0, 2000.0);
                let r = (reach / 10.0).ceil() * 10.0;
                paint_angled_axes(
                    &painter,
                    camera.yaw_deg,
                    camera.pitch_deg,
                    origin,
                    cell,
                    r,
                    ON_TOP,
                );
            } else {
                painter.hline(
                    rect.x_range(),
                    origin.y,
                    Stroke::new(1.5, crate::AXIS_X.gamma_multiply(ON_TOP)),
                );
                painter.vline(
                    origin.x,
                    rect.y_range(),
                    Stroke::new(1.5, crate::AXIS_Y.gamma_multiply(ON_TOP)),
                );
            }
        }
        let gizmo_id = id.with("gizmo-grab");
        let rotate_id = id.with("rotate-grab");
        let scale_id = id.with("scale-grab");
        let local_rot = opts.gizmo_start_rot;
        let move_dirs = crate::viewport_gizmo::move_gizmo_dirs(
            camera.yaw_deg,
            camera.pitch_deg,
            origin,
            cell,
            opts.gizmo_space,
            if opts.gizmo_space == "local" { local_rot } else { None },
        );
        let mut gizmo_hover = None;
        let mut gizmo_delta_label = String::new();
        let mut gizmo_move = None;
        let mut gizmo_rotate = None;
        let mut gizmo_scale = None;
        let mut rotate_hover = None;
        let mut rotate_angle_label = String::new();
        let mut rotate_arc: Option<crate::viewport_gizmo::RotateArcVisual> = None;
        if tool == "rotate" {
            if let Some(ix) = opts.gizmo_pivot {
                if ix < marks.len() {
                    let pivot = pivot_at(ix, pivot_scene_offset);
                    if rotate_hover.is_none() {
                        rotate_hover = pointer.and_then(|at| {
                            crate::viewport_gizmo::hit_rotate_gizmo(at, pivot, &move_dirs)
                        });
                    }
                    if let Some(grab) = self.ui.data(|data| data.get_temp::<RotateGrab>(rotate_id))
                    {
                        if let Some(at) = pointer {
                            if response.dragged_by(egui::PointerButton::Primary) {
                                let delta = crate::viewport_gizmo::rotate_drag_delta(
                                    grab.handle,
                                    pivot,
                                    grab.start_pointer,
                                    at,
                                );
                                let axis = crate::viewport_gizmo::rotate_axis_index(grab.handle);
                                let mut rot = grab.start_rot;
                                rot[axis] += delta;
                                gizmo_rotate = Some((grab.index, rot));
                                let degrees = (delta * 180.0 / std::f32::consts::PI).abs();
                                rotate_angle_label = format!("{degrees:.1}°");
                                let end = crate::viewport_gizmo::pointer_angle(pivot, at);
                                rotate_arc = Some(crate::viewport_gizmo::RotateArcVisual {
                                    handle: grab.handle,
                                    start: grab.start_angle,
                                    end,
                                    radius: crate::viewport_gizmo::ROTATE_SCREEN_R,
                                });
                            }
                        }
                    }
                    let label = rotate_angle_label.as_str();
                    crate::viewport_gizmo::paint_rotate_gizmo(
                        &painter,
                        pivot,
                        &move_dirs,
                        rotate_hover,
                        rotate_arc,
                        label,
                        self.ui.style(),
                    );
                }
            }
        }
        if tool == "scale" {
            if let Some(ix) = opts.gizmo_pivot {
                if ix < marks.len() {
                    let pivot = pivot_at(ix, pivot_scene_offset);
                    let mut scale_hover = None;
                    let mut scale_label = String::new();
                    if scale_hover.is_none() {
                        scale_hover = pointer.and_then(|at| {
                            crate::viewport_gizmo::hit_scale_gizmo(at, pivot, &move_dirs)
                        });
                    }
                    if let Some(grab) = self.ui.data(|data| data.get_temp::<ScaleGrab>(scale_id)) {
                        if let Some(at) = pointer {
                            if response.dragged_by(egui::PointerButton::Primary) {
                                let delta = at - grab.start_pointer;
                                let scene_d = crate::viewport_gizmo::scale_delta_from_screen(
                                    delta,
                                    grab.handle,
                                    camera.yaw_deg,
                                    cell,
                                );
                                let scale = crate::viewport_gizmo::apply_scale_drag(
                                    grab.start_scale,
                                    scene_d,
                                    grab.handle,
                                );
                                gizmo_scale = Some((grab.index, scale));
                                scale_label =
                                    format!("{:.2}, {:.2}, {:.2}", scale[0], scale[1], scale[2]);
                            }
                        }
                    }
                    let label = scale_label.as_str();
                    crate::viewport_gizmo::paint_scale_gizmo(
                        &painter,
                        pivot,
                        &move_dirs,
                        scale_hover,
                        label,
                        self.ui.style(),
                    );
                }
            }
        }
        if tool == "move" {
            if let Some(ix) = opts.gizmo_pivot {
                if ix < marks.len() {
                    let pivot = pivot_at(ix, pivot_scene_offset);
                    if gizmo_hover.is_none() {
                        gizmo_hover = pointer.and_then(|at| {
                            crate::viewport_gizmo::hit_move_gizmo(at, pivot, &move_dirs)
                        });
                    }
                    if let Some(grab) = self.ui.data(|data| data.get_temp::<GizmoGrab>(gizmo_id)) {
                        if let Some(at) = pointer {
                            if response.dragged_by(egui::PointerButton::Primary) {
                                let delta = at - grab.start_pointer;
                                let scene_d = crate::viewport_gizmo::scene_delta_from_screen(
                                    delta,
                                    grab.handle,
                                    camera.yaw_deg,
                                    cell,
                                );
                                let t = [
                                    grab.start_trans[0] + scene_d[0],
                                    grab.start_trans[1] + scene_d[1],
                                    grab.start_trans[2] + scene_d[2],
                                ];
                                gizmo_move = Some((grab.index, t));
                                gizmo_delta_label = format!(
                                    "{:.2}, {:.2}, {:.2}",
                                    scene_d[0], scene_d[1], scene_d[2]
                                );
                            }
                        }
                    }
                    let label = gizmo_delta_label.as_str();
                    crate::viewport_gizmo::paint_move_gizmo(
                        &painter,
                        pivot,
                        &move_dirs,
                        gizmo_hover,
                        label,
                        self.ui.style(),
                    );
                }
            }
        }
        let mut warn_y = rect.top() + 52.0;
        for (index, warning) in opts.warnings.iter().enumerate() {
            let ink = if warning.error {
                Color32::from_rgb(0xff, 0x55, 0x55)
            } else {
                Color32::from_rgb(0xff, 0xcc, 0x22)
            };
            painter.text(
                egui::pos2(rect.left() + 12.0, warn_y),
                Align2::LEFT_TOP,
                warning.title,
                small.clone(),
                ink,
            );
            warn_y += 18.0;
            painter.text(
                egui::pos2(rect.left() + 12.0, warn_y),
                Align2::LEFT_TOP,
                warning.hint,
                small.clone(),
                visuals.weak_text_color(),
            );
            warn_y += if index + 1 < opts.warnings.len() { 22.0 } else { 16.0 };
        }
        if let Some(overlay) = opts.stats_overlay {
            let (ax, ay) =
                opts.stats_overlay_edge.overlay_text_position(rect.width(), rect.height(), 10.0);
            let align = match opts.stats_overlay_edge {
                seal_editor_core::viewport_overlays::Edge::TopLeft => Align2::LEFT_TOP,
                seal_editor_core::viewport_overlays::Edge::TopRight => Align2::RIGHT_TOP,
                seal_editor_core::viewport_overlays::Edge::BottomLeft => Align2::LEFT_BOTTOM,
                seal_editor_core::viewport_overlays::Edge::BottomRight => Align2::RIGHT_BOTTOM,
            };
            painter.text(
                rect.left_top() + egui::vec2(ax, ay),
                align,
                overlay,
                small.clone(),
                Color32::from_rgb(0x5c, 0xc4, 0x6e),
            );
        }
        if let Some(caption) = opts.scene_caption {
            painter.text(
                rect.right_bottom() + egui::vec2(-10.0, -8.0),
                Align2::RIGHT_BOTTOM,
                caption,
                small.clone(),
                visuals.weak_text_color(),
            );
        }
        if let Some((label, running)) = opts.play_badge {
            paint_play_badge(&painter, rect, label, running, &small, !opts.game_view);
        }
        let banner = opts.paint_brush.map(|kind| {
            // Make the brush impossible to miss: while it is on, clicks paint instead of select.
            if opts.paint_erase {
                return "Erasing tiles: click or drag to erase, Esc to stop".to_string();
            }
            let file = kind.rsplit(['/', '\\']).next().unwrap_or(kind);
            format!("Painting {file}: click or drag to paint, Esc to stop")
        });
        if let Some(text) = banner.or_else(|| opts.prefab_banner.map(str::to_string)) {
            let galley = painter.layout_no_wrap(text, small.clone(), Color32::WHITE);
            let at = rect.center_top() + egui::vec2(-galley.size().x / 2.0, 34.0);
            let back = egui::Rect::from_min_size(at, galley.size()).expand2(egui::vec2(8.0, 4.0));
            painter.rect_filled(back, 0.0, Color32::from_rgba_unmultiplied(20, 60, 110, 230));
            painter.rect_stroke(
                back,
                0.0,
                Stroke::new(1.0, visuals.selection.bg_fill),
                egui::StrokeKind::Inside,
            );
            painter.galley(at, galley, Color32::WHITE);
        }
        paint_game_ui(&painter, rect, opts.game_ui);
        // The play view is where game screens live; remember it so pointer input can reach them.
        if !opts.game_screens.is_empty() {
            painter.ctx().data_mut(|d| d.insert_temp(egui::Id::new(GAME_SCREEN_RECT), rect));
        }
        paint_game_screens(painter.ctx(), &painter, rect, opts.game_screens);
        if !opts.game_view {
            paint_view_axis_gizmo(&painter, rect, camera.yaw_deg, camera.pitch_deg, &small);
        }
        if response.has_focus() && !opts.game_view {
            painter.rect_stroke(
                rect.shrink(1.0),
                0.0,
                Stroke::new(1.0, visuals.selection.bg_fill),
                egui::StrokeKind::Inside,
            );
        }

        // The smallest mark under the pointer wins, so a prop on the ground can be picked.
        let under = |pointer: egui::Pos2| {
            marks
                .iter()
                .enumerate()
                .filter(|(_, mark)| mark.pickable && hits(mark, pointer))
                .min_by(|left, right| {
                    let by_depth = near_depth(left.1)
                        .partial_cmp(&near_depth(right.1))
                        .unwrap_or(std::cmp::Ordering::Equal);
                    let by_area = place(left.1)
                        .area()
                        .partial_cmp(&place(right.1).area())
                        .unwrap_or(std::cmp::Ordering::Equal);
                    // In the angled view the smaller shape wins, so a prop on the ground is picked.
                    if angled {
                        by_area.then(by_depth)
                    } else {
                        by_depth.then(by_area)
                    }
                })
                .map(|(index, _)| index)
        };
        let grab = id.with("grab");
        let marquee_id = id.with("marquee");
        let lasso_id = id.with("lasso");
        let pick_spot_id = id.with("pick_spot");
        let pick_cycle_id = id.with("pick_cycle");
        let mark_world = |ix: usize| -> [f32; 3] {
            if opts.gizmo_pivot == Some(ix) {
                if let Some(world) = opts.gizmo_world {
                    return world;
                }
            }
            if angled {
                [marks[ix].x, marks[ix].y, marks[ix].depth]
            } else {
                [marks[ix].x, marks[ix].y, 0.0]
            }
        };
        let ctrl_drag_axis = |pointer: &egui::PointerState| -> Option<u8> {
            let primary = pointer.button_down(egui::PointerButton::Primary);
            let secondary = pointer.button_down(egui::PointerButton::Secondary);
            match (primary, secondary) {
                (true, false) => Some(0),
                (false, true) => Some(1),
                (true, true) => Some(2),
                _ => None,
            }
        };
        let mut picked = None;
        let mut moved = None;
        let mut move_axis = None;
        let mut alt_drag = false;
        let mut floor_key = false;
        let mut began = false;
        let mut marquee_pick = None;
        let mut lasso_pick = None;
        let allow_alt_entity = alt_held && tool == "move";
        if opts.paint_brush.is_none()
            && ((response.drag_started_by(egui::PointerButton::Primary)
                && (!alt_held || allow_alt_entity))
                || (response.drag_started_by(egui::PointerButton::Secondary)
                    && ctrl_held
                    && tool == "move"))
        {
            if let Some(at) = pointer {
                let gizmo_start =
                    (tool == "move").then_some(opts.gizmo_pivot).flatten().and_then(|ix| {
                        if ix >= marks.len() {
                            return None;
                        }
                        let pivot = pivot_at(ix, pivot_scene_offset);
                        crate::viewport_gizmo::hit_move_gizmo(at, pivot, &move_dirs)
                            .map(|handle| (ix, handle))
                    });
                if !angled {
                    if let Some((mark_i, index)) =
                        marks.iter().enumerate().rev().find_map(|(mark_i, mark)| {
                            let shape = mark.shape2d.as_ref()?;
                            if !shape.edit_points {
                                return None;
                            }
                            crate::poly_edit::hit(&shape.points, at, &|x, y| scene_to_screen(x, y))
                                .map(|index| (mark_i, index))
                        })
                    {
                        self.ui.data_mut(|data| {
                            data.insert_temp(
                                id.with("poly"),
                                crate::PolyGrab { mark: mark_i, index, fresh: true },
                            );
                        });
                        began = true;
                    }
                }
                if self.ui.data(|data| data.get_temp::<crate::PolyGrab>(id.with("poly"))).is_none()
                {
                    if let Some(hit) = crate::sprite_shape::hit_control(marks, at, &scene_to_screen)
                    {
                        self.ui.data_mut(|data| {
                            data.insert_temp(
                                id.with("spline"),
                                crate::SplineGrab {
                                    mark: hit.mark,
                                    handle: hit.handle,
                                    index: hit.index,
                                    fresh: true,
                                },
                            );
                            data.insert_temp(id.with("spline-select"), (hit.mark, hit.index));
                        });
                        began = true;
                    } else if let Some((ix, handle)) = gizmo_start {
                        if let Some(start_trans) = opts.gizmo_world {
                            self.ui.data_mut(|data| {
                                data.insert_temp(
                                    gizmo_id,
                                    GizmoGrab { index: ix, handle, start_trans, start_pointer: at },
                                );
                            });
                            began = true;
                        }
                    } else if tool == "rotate" {
                        if let Some(ix) = opts.gizmo_pivot {
                            if ix < marks.len() {
                                let pivot = pivot_at(ix, pivot_scene_offset);
                                if let Some(handle) =
                                    crate::viewport_gizmo::hit_rotate_gizmo(at, pivot, &move_dirs)
                                {
                                    if let Some(start_rot) = opts.gizmo_start_rot {
                                        let start_angle =
                                            crate::viewport_gizmo::pointer_angle(pivot, at);
                                        self.ui.data_mut(|data| {
                                            data.insert_temp(
                                                rotate_id,
                                                RotateGrab {
                                                    index: ix,
                                                    handle,
                                                    start_rot,
                                                    start_pointer: at,
                                                    start_angle,
                                                },
                                            );
                                        });
                                        began = true;
                                    }
                                }
                            }
                        }
                    } else if tool == "scale" {
                        if let Some(ix) = opts.gizmo_pivot {
                            if ix < marks.len() {
                                let pivot = pivot_at(ix, pivot_scene_offset);
                                if let Some(handle) =
                                    crate::viewport_gizmo::hit_scale_gizmo(at, pivot, &move_dirs)
                                {
                                    if let Some(start_scale) = opts.gizmo_start_scale {
                                        self.ui.data_mut(|data| {
                                            data.insert_temp(
                                                scale_id,
                                                ScaleGrab {
                                                    index: ix,
                                                    handle,
                                                    start_scale,
                                                    start_pointer: at,
                                                },
                                            );
                                        });
                                        began = true;
                                    }
                                }
                            }
                        }
                    } else if under(at).is_none() && !allow_alt_entity && !opts.measure_mode {
                        // Alt+drag on empty space draws a lasso; a plain drag draws a box.
                        if alt_held && tool == "select" {
                            self.ui.data_mut(|data| data.insert_temp(lasso_id, vec![[at.x, at.y]]));
                        } else if !alt_held {
                            self.ui.data_mut(|data| data.insert_temp(marquee_id, at));
                        }
                    } else if tool != "select"
                        && !opts.measure_mode
                        && !self.ui.data(|data| data.get_temp::<GizmoGrab>(gizmo_id)).is_some()
                        && !self.ui.data(|data| data.get_temp::<RotateGrab>(rotate_id)).is_some()
                        && !self.ui.data(|data| data.get_temp::<ScaleGrab>(scale_id)).is_some()
                    {
                        if let Some(index) = under(at) {
                            let offset = at - place(&marks[index]).center();
                            self.ui.data_mut(|data| {
                                data.insert_temp(
                                    grab,
                                    EntityGrab { index, offset, start_trans: mark_world(index) },
                                );
                            });
                            picked = Some(index);
                            began = true;
                            alt_drag = allow_alt_entity;
                        }
                    }
                }
            }
        }
        if let (Some(start), Some(end)) =
            (self.ui.data(|data| data.get_temp::<egui::Pos2>(marquee_id)), pointer)
        {
            let area = egui::Rect::from_two_pos(start, end);
            painter.rect_filled(area, 0.0, visuals.selection.bg_fill.gamma_multiply(0.12));
            painter.rect_stroke(
                area,
                0.0,
                Stroke::new(1.0, visuals.selection.stroke.color),
                egui::StrokeKind::Outside,
            );
        }
        if let Some(mut poly) = self.ui.data(|data| data.get_temp::<Vec<[f32; 2]>>(lasso_id)) {
            if let Some(at) = pointer {
                if response.dragged_by(egui::PointerButton::Primary)
                    && poly.last().is_none_or(|last| {
                        let dx = last[0] - at.x;
                        let dy = last[1] - at.y;
                        (dx * dx + dy * dy) > 16.0
                    })
                {
                    poly.push([at.x, at.y]);
                    self.ui.data_mut(|data| data.insert_temp(lasso_id, poly.clone()));
                }
                if poly.len() >= 2 {
                    let pts: Vec<egui::Pos2> =
                        poly.iter().map(|p| egui::pos2(p[0], p[1])).collect();
                    painter.add(egui::Shape::line(
                        pts,
                        Stroke::new(1.5, visuals.selection.stroke.color),
                    ));
                }
            }
        }
        let gizmo_grabbing = self.ui.data(|data| data.get_temp::<GizmoGrab>(gizmo_id)).is_some()
            || self.ui.data(|data| data.get_temp::<RotateGrab>(rotate_id)).is_some()
            || self.ui.data(|data| data.get_temp::<ScaleGrab>(scale_id)).is_some();
        let entity_dragging = response.dragged_by(egui::PointerButton::Primary)
            || (ctrl_held && response.dragged_by(egui::PointerButton::Secondary));
        if entity_dragging && !gizmo_grabbing {
            let held: Option<EntityGrab> = self.ui.data(|data| data.get_temp(grab));
            if let (Some(held), Some(at)) = (held, pointer) {
                alt_drag = alt_held && tool == "move";
                let center = at - held.offset;
                if angled && !matches!(tool, "rotate" | "scale") && !(ctrl_held && tool == "move") {
                    // Slide along the floor under the pointer: undo the view's yaw and pitch.
                    let (sx, sy) = pivot_shift(&marks[held.index]);
                    let base = crate::viewport_mesh::project_point(
                        [held.start_trans[0] + sx, held.start_trans[1] + sy, held.start_trans[2]],
                        camera.yaw_deg,
                        camera.pitch_deg,
                        origin,
                        cell,
                    )
                    .0;
                    let d = center - base;
                    let (sy, cy) = camera.yaw_deg.to_radians().sin_cos();
                    let sp = camera.pitch_deg.to_radians().sin();
                    let a = d.x / cell;
                    let mut t = held.start_trans;
                    if sp.abs() > 0.15 {
                        let b = d.y / (cell * sp);
                        t[0] += a * cy + b * sy;
                        t[2] += -a * sy + b * cy;
                    } else {
                        // Looking almost level: move across and up the screen instead.
                        t[0] += a * cy;
                        t[2] += -a * sy;
                        t[1] -= d.y / cell;
                    }
                    gizmo_move = Some((held.index, t));
                    alt_drag = alt_held;
                } else {
                    let rx = (center.x - origin.x) / cell;
                    let ry = (origin.y - center.y) / cell;
                    let x = rx * cos_yaw + ry * sin_yaw;
                    let y = -rx * sin_yaw + ry * cos_yaw;
                    let mut trans = held.start_trans;
                    trans[0] = x;
                    trans[1] = y;
                    if ctrl_held && tool == "move" {
                        if let Some(axis) = ctrl_drag_axis(&pointer_state) {
                            let mut locked = held.start_trans;
                            locked[axis as usize] = trans[axis as usize];
                            move_axis = Some((held.index, axis, locked));
                        }
                    } else {
                        let (sx, sy) = pivot_shift(&marks[held.index]);
                        moved = Some(match tool {
                            "rotate" => (held.index, response.drag_delta().x * 0.4, 0.0),
                            "scale" => (
                                held.index,
                                (1.0 - response.drag_delta().y * 0.01).clamp(0.5, 1.5),
                                0.0,
                            ),
                            _ => (held.index, x - sx, y - sy),
                        });
                    }
                }
            }
        } else if !response.drag_started_by(egui::PointerButton::Primary)
            && !response.drag_started_by(egui::PointerButton::Secondary)
        {
            self.ui.data_mut(|data| data.remove::<EntityGrab>(grab));
        }
        if response.drag_stopped_by(egui::PointerButton::Primary)
            || response.drag_stopped_by(egui::PointerButton::Secondary)
        {
            self.ui.data_mut(|data| data.remove::<EntityGrab>(grab));
            self.ui.data_mut(|data| data.remove::<GizmoGrab>(gizmo_id));
            self.ui.data_mut(|data| data.remove::<RotateGrab>(rotate_id));
            self.ui.data_mut(|data| data.remove::<ScaleGrab>(scale_id));
            if let (Some(start), Some(end)) =
                (self.ui.data(|data| data.get_temp::<egui::Pos2>(marquee_id)), pointer)
            {
                let area = egui::Rect::from_two_pos(start, end);
                if area.width() > 4.0 && area.height() > 4.0 {
                    marquee_pick = Some(
                        marks
                            .iter()
                            .enumerate()
                            .filter(|(_, mark)| mark.pickable && area.intersects(place(mark)))
                            .map(|(index, _)| index)
                            .collect(),
                    );
                }
                self.ui.data_mut(|data| data.remove::<egui::Pos2>(marquee_id));
            }
            if let Some(poly) = self.ui.data(|data| data.get_temp::<Vec<[f32; 2]>>(lasso_id)) {
                if poly.len() >= 3 {
                    let scene_poly: Vec<[f32; 2]> = poly
                        .iter()
                        .map(|p| {
                            let (x, y) = screen_to_scene(egui::pos2(p[0], p[1]));
                            [x, y]
                        })
                        .collect();
                    let rects: Vec<seal_editor_core::viewport_selection::MarkRect> = marks
                        .iter()
                        .map(|m| seal_editor_core::viewport_selection::MarkRect {
                            x: m.x,
                            y: m.y,
                            half_w: m.half[0],
                            half_h: m.half[1],
                            depth: m.depth,
                            pickable: m.pickable,
                        })
                        .collect();
                    lasso_pick = Some(seal_editor_core::viewport_selection::lasso_indices(
                        &rects,
                        &scene_poly,
                    ));
                }
                self.ui.data_mut(|data| data.remove::<Vec<[f32; 2]>>(lasso_id));
            }
        }
        if response.clicked() && !alt_held && opts.paint_brush.is_none() && !opts.measure_mode {
            if let Some(at) = pointer {
                let rects: Vec<seal_editor_core::viewport_selection::MarkRect> = marks
                    .iter()
                    .map(|m| seal_editor_core::viewport_selection::MarkRect {
                        x: m.x,
                        y: m.y,
                        half_w: m.half[0],
                        half_h: m.half[1],
                        depth: m.depth,
                        pickable: m.pickable,
                    })
                    .collect();
                let (sx, sy) = screen_to_scene(at);
                let stack = seal_editor_core::viewport_selection::pick_stack_at(&rects, sx, sy);
                let last_spot: Option<[f32; 2]> = self.ui.data(|d| d.get_temp(pick_spot_id));
                let last_pick: Option<usize> = self.ui.data(|d| d.get_temp(pick_cycle_id));
                let same_spot = last_spot.is_some_and(|spot| {
                    let dx = spot[0] - at.x;
                    let dy = spot[1] - at.y;
                    dx * dx + dy * dy < 36.0
                });
                picked = seal_editor_core::viewport_selection::pick_through_next(
                    &stack,
                    if same_spot { last_pick } else { None },
                );
                self.ui.data_mut(|d| {
                    d.insert_temp(pick_spot_id, [at.x, at.y]);
                    if let Some(ix) = picked {
                        d.insert_temp(pick_cycle_id, ix);
                    }
                });
            }
        }
        if let Some(pos) = response.hover_pos() {
            let (x, y) = screen_to_scene(pos);
            self.ui.data_mut(|data| {
                data.insert_temp(egui::Id::new("seal-pointer-world"), [x, y]);
            });
        }
        // Tile brush: build a stroke while the button is down, commit on release (one undo step).
        let mut paint_cells: Vec<(f32, f32)> = Vec::new();
        let mut paint_tiles: Vec<(i32, i32)> = Vec::new();
        let mut paint_stroke_commit = None;
        let mut paint_preview: Vec<(i32, i32)> = Vec::new();
        if let Some(kind) = opts.paint_brush {
            let grid = seal_editor_core::tiles::Grid::parse(opts.paint_grid);
            let tool = opts.paint_tool;
            let erase = opts.paint_erase || tool == "eraser";
            let cell_of = |pos: egui::Pos2| {
                let (sx, sy) = screen_to_scene(pos);
                grid.cell_at(sx, sy)
            };
            let anchor_id = id.with("paint-anchor");
            let stroke_id = id.with("paint-stroke");
            let last_id = id.with("paint-last");
            let was_down_id = id.with("paint-was-down");
            let (presses, down, primary_up, hover) = self.ui.input(|input| {
                let presses: Vec<egui::Pos2> = input
                    .events
                    .iter()
                    .filter_map(|event| match event {
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            ..
                        } => Some(*pos),
                        _ => None,
                    })
                    .collect();
                let primary_up = input.events.iter().any(|event| {
                    matches!(
                        event,
                        egui::Event::PointerButton {
                            button: egui::PointerButton::Primary,
                            pressed: false,
                            ..
                        }
                    )
                });
                (presses, input.pointer.primary_down(), primary_up, input.pointer.hover_pos())
            });
            let was_down: bool = self.ui.data(|data| data.get_temp(was_down_id).unwrap_or(false));
            let mut stroke: Vec<(i32, i32)> =
                self.ui.data(|data| data.get_temp(stroke_id)).unwrap_or_default();
            let mut anchor: Option<(i32, i32)> = self.ui.data(|data| data.get_temp(anchor_id));
            let mut last: Option<(i32, i32)> = self.ui.data(|data| data.get_temp(last_id));
            if let Some(pos) = hover.filter(|pos| rect.contains(*pos)) {
                let hovered = cell_of(pos);
                let (cx, cy) = grid.center(hovered);
                if !erase {
                    Self::paint_place_ghost(&painter, &place, cx, cy, kind);
                }
                if !angled_view {
                    let corners: Vec<egui::Pos2> = grid
                        .outline(hovered)
                        .into_iter()
                        .map(|(x, y)| scene_to_screen(x, y))
                        .collect();
                    let color = if erase {
                        Color32::from_rgb(235, 90, 80)
                    } else {
                        Color32::from_rgb(140, 200, 255)
                    };
                    painter.add(egui::Shape::closed_line(corners, Stroke::new(1.5, color)));
                }
            }
            for pos in presses.into_iter().filter(|pos| rect.contains(*pos)) {
                let cell = cell_of(pos);
                anchor = Some(cell);
                last = Some(cell);
                stroke = match tool {
                    "rectangle" | "pattern_save" => vec![cell],
                    "pattern" => vec![cell],
                    _ => vec![cell],
                };
            }
            if down {
                if let Some(pos) = hover.filter(|pos| rect.contains(*pos)) {
                    let cell = cell_of(pos);
                    stroke = match tool {
                        "rectangle" | "pattern_save" => anchor
                            .map(|a| seal_editor_core::tile_tools::rectangle_cells(a, cell))
                            .unwrap_or(vec![cell]),
                        "line" | "brush" | "random" | "eraser" => {
                            let from = anchor.unwrap_or(cell);
                            let mut cells = grid.line(from, cell);
                            if cells.is_empty() {
                                cells.push(cell);
                            }
                            cells
                        }
                        _ => stroke,
                    };
                    last = Some(cell);
                }
            }
            paint_preview = stroke.clone();
            for cell in &paint_preview {
                if !angled_view {
                    let fill: Vec<egui::Pos2> = grid
                        .outline(*cell)
                        .into_iter()
                        .map(|(x, y)| scene_to_screen(x, y))
                        .collect();
                    let tint = if erase {
                        Color32::from_rgba_unmultiplied(235, 90, 80, 50)
                    } else {
                        Color32::from_rgba_unmultiplied(140, 200, 255, 55)
                    };
                    painter.add(egui::Shape::convex_polygon(fill, tint, Stroke::NONE));
                }
            }
            if was_down && !down && primary_up {
                if !stroke.is_empty() {
                    paint_stroke_commit = Some(stroke.clone());
                } else if let Some(start) = anchor.or(last) {
                    paint_stroke_commit = Some(vec![start]);
                }
                stroke.clear();
                anchor = None;
                last = None;
            }
            paint_tiles = paint_preview.clone();
            for cell in &paint_tiles {
                paint_cells.push(grid.center(*cell));
            }
            if !paint_tiles.is_empty() {
                response.request_focus();
            }
            self.ui.data_mut(|data| {
                data.insert_temp(was_down_id, down);
                if let Some(c) = anchor {
                    data.insert_temp(anchor_id, c);
                } else {
                    data.remove::<(i32, i32)>(anchor_id);
                }
                if let Some(c) = last {
                    data.insert_temp(last_id, c);
                } else {
                    data.remove::<(i32, i32)>(last_id);
                }
                if stroke.is_empty() {
                    data.remove::<Vec<(i32, i32)>>(stroke_id);
                } else {
                    data.insert_temp(stroke_id, stroke);
                }
            });
        }
        let mut place_drop = None;
        if let Some(payload) = response.dnd_hover_payload::<String>() {
            if !payload.starts_with("place:") {
                if let Some(pos) = self.ui.input(|input| input.pointer.hover_pos()) {
                    if rect.contains(pos) {
                        response.clone().on_hover_cursor(egui::CursorIcon::NotAllowed);
                    }
                }
            }
        }
        if let Some(kind) = response
            .dnd_hover_payload::<String>()
            .as_deref()
            .and_then(|payload| seal_editor_core::place_body_from_drag(payload))
        {
            if let Some(pos) = self.ui.input(|input| input.pointer.hover_pos()) {
                if rect.contains(pos) {
                    let (sx, sy) = screen_to_scene(pos);
                    Self::paint_place_ghost(&painter, &place, sx, sy, kind);
                }
            }
        }
        if let Some(payload) = response.dnd_release_payload::<String>() {
            if let Some(kind) = seal_editor_core::place_body_from_drag(&payload) {
                if let Some(pos) = self.ui.input(|input| input.pointer.interact_pos()) {
                    if rect.contains(pos) {
                        let (sx, sy) = screen_to_scene(pos);
                        place_drop = Some((kind.to_string(), sx, sy));
                    }
                }
            }
        }
        let mut menu_pick = None;
        response.context_menu(|ui| {
            let at = ui.input(|input| input.pointer.interact_pos());
            if let Some(pos) = at {
                if let Some(hit) = crate::sprite_shape::hit_control(marks, pos, &scene_to_screen) {
                    if !hit.handle {
                        if let Some(shape) =
                            marks.get(hit.mark).and_then(|m| m.sprite_shape.as_ref())
                        {
                            if let Some(pick) = crate::paint_context_menu(
                                ui,
                                None,
                                &crate::sprite_shape::point_menu(shape),
                            ) {
                                ui.ctx().data_mut(|data| {
                                    data.insert_temp(
                                        id.with("spline-menu"),
                                        (hit.mark, hit.index, pick.command),
                                    );
                                });
                            }
                            return;
                        }
                    }
                }
            }
            let target = at.and_then(under);
            let (caption, entries) = if let Some(ix) = target {
                (Some(marks[ix].label), menus.entity.entries)
            } else {
                (menus.empty.caption, menus.empty.entries)
            };
            if let Some(mut pick) = crate::paint_context_menu(ui, caption, entries) {
                if pick.command == "viewport.go-here" {
                    if let Some(pos) = at {
                        let (sx, sy) = screen_to_scene(pos);
                        pick.args.push(("x".into(), sx.to_string()));
                        pick.args.push(("y".into(), sy.to_string()));
                    }
                }
                menu_pick = Some((target, pick));
            }
        });
        let mut key = None;
        if response.has_focus() {
            self.ui.input(|input| {
                for event in &input.events {
                    if let egui::Event::Key { key: pressed, pressed: true, .. } = event {
                        if *pressed == egui::Key::End {
                            floor_key = true;
                        }
                        key = Some(format!("{pressed:?}"));
                    }
                }
            });
        }
        let (pick_shift, pick_ctrl) =
            self.ui.input(|input| (input.modifiers.shift, input.modifiers.ctrl));
        if let (Some(ix), true) = (opts.gizmo_pivot, tool == "move") {
            if ix < marks.len() {
                let pivot = pivot_at(ix, pivot_scene_offset);
                if let Some((_, axis, _)) = move_axis {
                    let name = match axis {
                        0 => "x",
                        1 => "y",
                        _ => "z",
                    };
                    crate::viewport_gizmo::paint_ctrl_axis_guide(&painter, pivot, &move_dirs, name);
                }
                if opts.vertex_snap {
                    crate::viewport_gizmo::paint_vertex_snap_guides(&painter, pivot);
                }
                if pivot_scene_offset.iter().any(|v| v.abs() > f32::EPSILON) {
                    crate::viewport_gizmo::paint_temp_pivot(&painter, pivot);
                }
            }
        }
        let measure = if opts.measure_mode {
            let (shift, _) = self.ui.input(|input| (input.modifiers.shift, input.modifiers.ctrl));
            let primary_pressed =
                self.ui.input(|input| input.pointer.button_pressed(egui::PointerButton::Primary));
            let primary_released =
                self.ui.input(|input| input.pointer.button_released(egui::PointerButton::Primary));
            let primary_down = self.ui.input(|input| input.pointer.primary_down());
            Some(MeasurePointer {
                pos: pointer,
                primary_pressed,
                primary_released,
                primary_down,
                double_clicked: response.double_clicked_by(egui::PointerButton::Primary),
                shift,
                rect,
                origin,
                cell,
            })
        } else {
            None
        };
        let mut spline = None;
        let spline_id = id.with("spline");
        if let Some(mut grab) = self.ui.data(|data| data.get_temp::<crate::SplineGrab>(spline_id)) {
            if let Some(at) = pointer {
                if marks.get(grab.mark).and_then(|m| m.sprite_shape.as_ref()).is_some() {
                    let (x, y) = screen_to_scene(at);
                    spline = Some(crate::SplineEdit {
                        mark: grab.mark,
                        op: if grab.handle { "handle" } else { "point" },
                        index: grab.index,
                        x,
                        y,
                        began: grab.fresh,
                        smooth: false,
                    });
                    grab.fresh = false;
                    self.ui.data_mut(|data| data.insert_temp(spline_id, grab));
                    moved = None;
                    picked = None;
                }
            }
        }
        if response.drag_stopped_by(egui::PointerButton::Primary) {
            self.ui.data_mut(|data| data.remove::<crate::SplineGrab>(spline_id));
        }
        let mut poly = None;
        let poly_id = id.with("poly");
        if let Some(mut grab) = self.ui.data(|data| data.get_temp::<crate::PolyGrab>(poly_id)) {
            if let Some(at) = pointer {
                if marks
                    .get(grab.mark)
                    .and_then(|m| m.shape2d.as_ref())
                    .is_some_and(|s| s.edit_points)
                {
                    let (x, y) = screen_to_scene(at);
                    poly = Some(crate::PolyEdit {
                        mark: grab.mark,
                        index: grab.index,
                        x,
                        y,
                        began: grab.fresh,
                    });
                    grab.fresh = false;
                    self.ui.data_mut(|data| data.insert_temp(poly_id, grab));
                    moved = None;
                    picked = None;
                }
            }
        }
        if response.drag_stopped_by(egui::PointerButton::Primary) {
            self.ui.data_mut(|data| data.remove::<crate::PolyGrab>(poly_id));
        }
        if spline.is_none() && response.double_clicked_by(egui::PointerButton::Primary) {
            if let Some(at) = pointer {
                let (x, y) = screen_to_scene(at);
                let max_dist = 8.0 / cell.max(1.0);
                for (mark_i, mark) in marks.iter().enumerate() {
                    let Some(shape) = &mark.sprite_shape else { continue };
                    if !shape.editing
                        || crate::sprite_shape::hit_control(marks, at, &scene_to_screen).is_some()
                    {
                        continue;
                    }
                    if let Some(seg) = crate::sprite_shape::hit_outline(shape, [x, y], max_dist) {
                        spline = Some(crate::SplineEdit {
                            mark: mark_i,
                            op: "insert",
                            index: seg,
                            x,
                            y,
                            began: true,
                            smooth: true,
                        });
                        break;
                    }
                }
            }
        }
        if spline.is_none() && matches!(key.as_deref(), Some("Delete") | Some("Backspace")) {
            if let Some((mark_i, index)) =
                self.ui.data(|data| data.get_temp::<(usize, usize)>(id.with("spline-select")))
            {
                if marks
                    .get(mark_i)
                    .and_then(|m| m.sprite_shape.as_ref())
                    .is_some_and(|s| s.editing)
                {
                    spline = Some(crate::SplineEdit {
                        mark: mark_i,
                        op: "delete",
                        index,
                        x: 0.0,
                        y: 0.0,
                        began: true,
                        smooth: false,
                    });
                    key = None;
                    self.ui
                        .data_mut(|data| data.remove::<(usize, usize)>(id.with("spline-select")));
                }
            }
        }
        let menu_edit =
            self.ui.data(|data| data.get_temp::<(usize, usize, String)>(id.with("spline-menu")));
        if let Some((mark_i, index, command)) = menu_edit {
            self.ui.data_mut(|data| data.remove::<(usize, usize, String)>(id.with("spline-menu")));
            spline = Some(crate::SplineEdit {
                mark: mark_i,
                op: "corner",
                index,
                x: 0.0,
                y: 0.0,
                began: true,
                smooth: command == "ss-smooth",
            });
            menu_pick = None;
        }
        GridHit {
            clicked: response.clicked(),
            key,
            pick_shift,
            pick_ctrl,
            picked,
            moved,
            began,
            menu: menu_pick,
            marquee: marquee_pick,
            lasso: lasso_pick,
            camera_dirty,
            camera_speed: camera_speed_hit,
            gizmo_move,
            gizmo_rotate,
            gizmo_scale,
            alt_drag,
            move_axis,
            floor_key,
            place_drop,
            paint_cells,
            paint_tiles,
            paint_stroke_commit,
            paint_preview,
            measure,
            spline,
            poly,
        }
    }

    fn viewport_tool_icon(
        ui: &mut egui::Ui,
        icon: crate::icons::Icon,
        on: bool,
        tip: &str,
    ) -> bool {
        let (rect, response) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), Sense::click());
        paint_icon_plate(ui, rect, &response, on);
        let response = crate::a11y::named_control(ui, response, egui::WidgetType::Button, tip);
        let ink = ui.visuals().text_color();
        crate::icons::paint_icon(ui.painter(), rect.center(), icon, crate::ICON_PX, ink);
        response.clicked()
    }

    fn viewport_snap_value(
        ui: &mut egui::Ui,
        id: &str,
        current: f32,
        steps: &[f32],
        suffix: &str,
    ) -> Option<f32> {
        let label = if suffix.is_empty() {
            trim_step(current)
        } else {
            format!("{}{suffix}", trim_step(current))
        };
        let mut chosen = None;
        egui::ComboBox::from_id_salt(id).selected_text(label).width(52.0).show_crisp(ui, |ui| {
            for step in steps {
                let text = if suffix.is_empty() {
                    trim_step(*step).to_string()
                } else {
                    format!("{}{suffix}", trim_step(*step))
                };
                if ui.selectable_label((*step - current).abs() < f32::EPSILON, text).clicked() {
                    chosen = Some(*step);
                }
            }
        });
        chosen
    }

    /// Translucent toolbar along the top inside the viewport (view pill and tool icons).
    #[allow(clippy::too_many_arguments)]
    pub fn viewport_toolbar(
        &mut self,
        view: &str,
        view_entries: &[(&str, &str)],
        mode: &str,
        mode_entries: &[(&str, &str, bool)],
        tool: &str,
        tools: &[(crate::icons::Icon, &str, &str)],
        hamburger: &ViewportHamburgerMenu<'_>,
        show_menu: ViewportShowMenu<'_>,
        bar: ViewportBarState<'_>,
        tooltips: &ViewportBarTooltips<'_>,
    ) -> ViewportToolbarHit {
        let mut hit = ViewportToolbarHit {
            view: None,
            mode: None,
            tool: None,
            maximize: false,
            space_toggle: false,
            surface_toggle: false,
            grid_toggle: false,
            grid_step: None,
            angle_toggle: false,
            angle_step: None,
            scale_toggle: false,
            scale_step: None,
            speed_toggle: false,
            speed: None,
            show_defaults: false,
            show_layer: None,
            show_menu_toggle: false,
            hamburger_toggle: false,
            hamburger_pick: None,
            hamburger_fov: None,
            hamburger_far: None,
            hamburger_screen_pct: None,
            toolbar_expand: false,
        };
        let view_id = view;
        let mode_id = mode;
        let tool_id = tool;
        let plate = self.ui.visuals().panel_fill.gamma_multiply(if self.ui.visuals().dark_mode {
            0.72
        } else {
            0.82
        });
        egui::Frame::new()
            .fill(plate)
            .corner_radius(egui::CornerRadius::same(crate::look::CONTROL_RADIUS))
            .inner_margin(egui::Margin::symmetric(6, 2))
            .show(self.ui, |ui| {
                ui.set_min_height(crate::TOOL_ROW_PX - 4.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let (ham_rect, ham_resp) =
                        ui.allocate_exact_size(egui::vec2(22.0, 22.0), Sense::click());
                    let ham_popup_id = egui::Popup::default_response_id(&ham_resp);
                    if hamburger.menu_open {
                        // A scripted open: open once, then hand the state back to the popup.
                        egui::Popup::open_id(ui.ctx(), ham_popup_id);
                        hit.hamburger_toggle = true;
                    }
                    let ham_is_open = egui::Popup::is_id_open(ui.ctx(), ham_popup_id);
                    paint_icon_plate(ui, ham_rect, &ham_resp, ham_is_open);
                    let ham_resp = crate::a11y::named_control(
                        ui,
                        ham_resp,
                        egui::WidgetType::Button,
                        hamburger.tooltip,
                    );
                    crate::icons::paint_icon(
                        ui.painter(),
                        ham_rect.center(),
                        crate::icons::Icon::EditMore,
                        crate::ICON_PX,
                        ui.visuals().text_color(),
                    );
                    let ham_popup = egui::Popup::menu(&ham_resp)
                        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
                    ham_popup.show(|ui| {
                        ui.set_min_width(280.0);
                        if crate::crisp::menu_row(
                            ui,
                            Some(hamburger.realtime),
                            hamburger.lbl_realtime,
                        )
                        .clicked()
                        {
                            hit.hamburger_pick = Some("realtime".into());
                            ui.close();
                        }
                        if crate::crisp::menu_row(
                            ui,
                            Some(hamburger.show_stats),
                            hamburger.lbl_show_stats,
                        )
                        .clicked()
                        {
                            hit.hamburger_pick = Some("show_stats".into());
                            ui.close();
                        }
                        if crate::crisp::menu_row(
                            ui,
                            Some(hamburger.show_fps),
                            hamburger.lbl_show_fps,
                        )
                        .clicked()
                        {
                            hit.hamburger_pick = Some("show_fps".into());
                            ui.close();
                        }
                        if crate::crisp::menu_row(
                            ui,
                            Some(hamburger.show_toolbar),
                            hamburger.lbl_show_toolbar,
                        )
                        .clicked()
                        {
                            hit.hamburger_pick = Some("show_toolbar".into());
                            ui.close();
                        }
                        if crate::crisp::menu_row(ui, None, hamburger.lbl_isolate).clicked() {
                            hit.hamburger_pick = Some("isolate".into());
                            ui.close();
                        }
                        ui.menu_button(hamburger.lbl_overlays, |ui| {
                            if crate::crisp::menu_row(
                                ui,
                                Some(hamburger.toolbar_collapsed),
                                hamburger.lbl_overlay_collapse,
                            )
                            .clicked()
                            {
                                hit.hamburger_pick = Some("overlay_collapse".into());
                                ui.close();
                            }
                            ui.label(hamburger.lbl_stats_edge);
                            for (edge, label) in hamburger.overlay_edges {
                                if ui.button(*label).clicked() {
                                    hit.hamburger_pick = Some(format!("overlay_stats_{edge}"));
                                    ui.close();
                                }
                            }
                        });
                        ui.separator();
                        ui.label(hamburger.lbl_fov);
                        let mut fov = hamburger.fov;
                        if ui.add(egui::Slider::new(&mut fov, 40.0..=120.0).step_by(1.0)).changed()
                        {
                            hit.hamburger_fov = Some(fov);
                        }
                        ui.horizontal(|ui| {
                            ui.label(hamburger.lbl_far_plane);
                            let mut far = hamburger.far_plane;
                            if ui
                                .add(
                                    egui::DragValue::new(&mut far)
                                        .speed(100.0)
                                        .range(100.0..=1_000_000.0),
                                )
                                .changed()
                            {
                                hit.hamburger_far = Some(far);
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label(hamburger.lbl_screen_pct);
                            let mut pct = hamburger.screen_pct;
                            if ui
                                .add(
                                    egui::DragValue::new(&mut pct)
                                        .speed(1.0)
                                        .range(25.0..=200.0)
                                        .suffix("%"),
                                )
                                .changed()
                            {
                                hit.hamburger_screen_pct = Some(pct);
                            }
                        });
                        if crate::crisp::menu_row(
                            ui,
                            Some(hamburger.cinematic),
                            hamburger.lbl_cinematic,
                        )
                        .clicked()
                        {
                            hit.hamburger_pick = Some("cinematic".into());
                            ui.close();
                        }
                        ui.add_enabled_ui(false, |ui| ui.label(hamburger.lbl_game_view));
                        if crate::crisp::menu_row(ui, None, hamburger.lbl_immersive).clicked() {
                            hit.hamburger_pick = Some("immersive".into());
                            ui.close();
                        }
                        ui.menu_button(hamburger.lbl_bookmarks, |ui| {
                            ui.label(hamburger.lbl_bookmark_store_heading);
                            for (slot, label) in hamburger.bookmark_store {
                                if ui.button(*label).clicked() {
                                    hit.hamburger_pick = Some(format!("bookmark_store:{slot}"));
                                    ui.close();
                                }
                            }
                            ui.separator();
                            ui.label(hamburger.lbl_bookmark_jump_heading);
                            for (slot, label) in hamburger.bookmark_jump {
                                if ui.button(*label).clicked() {
                                    hit.hamburger_pick = Some(format!("bookmark_jump:{slot}"));
                                    ui.close();
                                }
                            }
                        });
                        if crate::crisp::menu_row(ui, None, hamburger.lbl_create_camera).clicked() {
                            hit.hamburger_pick = Some("create_camera".into());
                            ui.close();
                        }
                        if crate::crisp::menu_row(ui, None, hamburger.lbl_screenshot).clicked() {
                            hit.hamburger_pick = Some("screenshot".into());
                            ui.close();
                        }
                        ui.menu_button(hamburger.lbl_layouts, |ui| {
                            for (id, label, enabled) in hamburger.layout_entries {
                                if *enabled {
                                    if ui.button(*label).clicked() {
                                        hit.hamburger_pick = Some((*id).to_string());
                                        ui.close();
                                    }
                                } else {
                                    ui.add_enabled_ui(false, |ui| ui.label(*label));
                                }
                            }
                        });
                        ui.add_enabled_ui(false, |ui| ui.label(hamburger.lbl_advanced));
                    });
                    let current = view_entries
                        .iter()
                        .find(|(id, _)| *id == view_id)
                        .map(|(_, label)| *label)
                        .unwrap_or(view_id);
                    egui::ComboBox::from_id_salt("viewport-view-pill")
                        .selected_text(current)
                        .width(120.0)
                        .show_crisp(ui, |ui| {
                            for (id, label) in view_entries {
                                let response = ui.selectable_label(*id == view_id, *label);
                                if response.clicked() {
                                    hit.view = Some((*id).to_string());
                                }
                            }
                        });
                    let mode_current = mode_entries
                        .iter()
                        .find(|(id, _, _)| *id == mode_id)
                        .map(|(_, label, _)| *label)
                        .unwrap_or(mode_id);
                    egui::ComboBox::from_id_salt("viewport-shade-pill")
                        .selected_text(mode_current)
                        .width(108.0)
                        .show_crisp(ui, |ui| {
                            for (id, label, enabled) in mode_entries {
                                if *enabled {
                                    let response = ui.selectable_label(*id == mode_id, *label);
                                    if response.clicked() {
                                        hit.mode = Some((*id).to_string());
                                    }
                                } else {
                                    ui.add_enabled_ui(false, |ui| ui.label(*label));
                                }
                            }
                        });
                    let (show_rect, show_response) =
                        ui.allocate_exact_size(egui::vec2(72.0, 22.0), Sense::click());
                    let show_popup_id = egui::Popup::default_response_id(&show_response);
                    if show_menu.menu_open {
                        egui::Popup::open_id(ui.ctx(), show_popup_id);
                        hit.show_menu_toggle = true;
                    }
                    let show_is_open = egui::Popup::is_id_open(ui.ctx(), show_popup_id);
                    paint_icon_plate(ui, show_rect, &show_response, show_is_open);
                    show_response.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, show_menu.title)
                    });
                    ui.painter().text(
                        show_rect.center(),
                        Align2::CENTER_CENTER,
                        show_menu.title,
                        TextStyle::Body.resolve(ui.style()),
                        ui.visuals().text_color(),
                    );
                    let popup = egui::Popup::menu(&show_response)
                        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
                    popup.show(|ui| {
                        ui.set_min_width(220.0);
                        if ui.button(show_menu.defaults_label).clicked() {
                            hit.show_defaults = true;
                            ui.close();
                        }
                        ui.separator();
                        for (id, label) in show_menu.entries {
                            let on = show_menu.layers.get(*id).copied().unwrap_or(true);
                            if crate::crisp::menu_row(ui, Some(on), label).clicked() {
                                hit.show_layer = Some(((*id).to_string(), !on));
                                ui.close();
                            }
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        if Self::viewport_tool_icon(
                            ui,
                            crate::icons::Icon::ViewMaximize,
                            false,
                            tooltips.maximize,
                        ) {
                            hit.maximize = true;
                        }
                        for (icon, id, tip) in tools {
                            if Self::viewport_tool_icon(ui, *icon, tool_id == *id, tip) {
                                hit.tool = Some((*id).to_string());
                            }
                        }
                        const GRID_STEPS: [f32; 7] = [0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0];
                        const ANGLE_STEPS: [f32; 8] =
                            [5.0, 10.0, 15.0, 30.0, 45.0, 60.0, 90.0, 120.0];
                        const SCALE_STEPS: [f32; 6] = [10.0, 1.0, 0.5, 0.25, 0.125, 0.0625];
                        let speed_id = ui.id().with("viewport-speed");
                        let speed_popup = ui
                            .data(|data| data.get_temp::<bool>(speed_id))
                            .unwrap_or(bar.speed_popup);
                        if bar.speed_popup {
                            ui.data_mut(|data| data.insert_temp(speed_id, true));
                        }
                        let speed_label = format!("{}", bar.camera_speed.round() as i32);
                        let (speed_rect, speed_resp) =
                            ui.allocate_exact_size(egui::vec2(26.0, 22.0), Sense::click());
                        paint_icon_plate(ui, speed_rect, &speed_resp, speed_popup);
                        ui.painter().text(
                            speed_rect.center(),
                            Align2::CENTER_CENTER,
                            speed_label,
                            TextStyle::Small.resolve(ui.style()),
                            ui.visuals().text_color(),
                        );
                        if speed_resp.clicked() {
                            hit.speed_toggle = true;
                            ui.data_mut(|data| data.insert_temp(speed_id, !speed_popup));
                        }
                        if ui.data(|data| data.get_temp::<bool>(speed_id).unwrap_or(false)) {
                            egui::Popup::menu(&speed_resp)
                                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                                .show(|ui| {
                                    ui.set_min_width(140.0);
                                    let mut speed = bar.camera_speed;
                                    if ui
                                        .add(egui::Slider::new(&mut speed, 1.0..=8.0).step_by(0.5))
                                        .changed()
                                    {
                                        hit.speed = Some(speed);
                                    }
                                });
                        }
                        if let Some(step) = Self::viewport_snap_value(
                            ui,
                            "viewport-scale-step",
                            bar.scale_step,
                            &SCALE_STEPS,
                            "",
                        ) {
                            hit.scale_step = Some(step);
                        }
                        if Self::viewport_tool_icon(
                            ui,
                            crate::icons::Icon::ToolScaleSnap,
                            bar.scale_snap,
                            tooltips.scale_snap,
                        ) {
                            hit.scale_toggle = true;
                        }
                        if let Some(step) = Self::viewport_snap_value(
                            ui,
                            "viewport-angle-step",
                            bar.angle_deg,
                            &ANGLE_STEPS,
                            "°",
                        ) {
                            hit.angle_step = Some(step);
                        }
                        if Self::viewport_tool_icon(
                            ui,
                            crate::icons::Icon::ToolAngleSnap,
                            bar.angle_snap,
                            tooltips.angle_snap,
                        ) {
                            hit.angle_toggle = true;
                        }
                        if let Some(step) = Self::viewport_snap_value(
                            ui,
                            "viewport-grid-step",
                            bar.grid_step,
                            &GRID_STEPS,
                            "",
                        ) {
                            hit.grid_step = Some(step);
                        }
                        if Self::viewport_tool_icon(
                            ui,
                            crate::icons::Icon::ToolGridSnap,
                            bar.grid_snap,
                            tooltips.grid_snap,
                        ) {
                            hit.grid_toggle = true;
                        }
                        if Self::viewport_tool_icon(
                            ui,
                            crate::icons::Icon::ToolSnapSurface,
                            bar.surface_snap,
                            tooltips.surface_snap,
                        ) {
                            hit.surface_toggle = true;
                        }
                        let space_icon = if bar.space == "local" {
                            crate::icons::Icon::ToolLocal
                        } else {
                            crate::icons::Icon::ToolWorld
                        };
                        let space_on = bar.space == "local";
                        if Self::viewport_tool_icon(ui, space_icon, space_on, tooltips.space) {
                            hit.space_toggle = true;
                        }
                    });
                });
            });
        hit
    }

    /// One control that expands the full viewport toolbar when it is collapsed.
    pub fn viewport_collapsed_toolbar(
        &mut self,
        edge: seal_editor_core::viewport_overlays::Edge,
        tooltip: &str,
    ) -> ViewportToolbarHit {
        let mut hit = ViewportToolbarHit::default();
        let plate = self.ui.visuals().panel_fill.gamma_multiply(if self.ui.visuals().dark_mode {
            0.72
        } else {
            0.82
        });
        egui::Frame::new()
            .fill(plate)
            .corner_radius(egui::CornerRadius::same(crate::look::CONTROL_RADIUS))
            .inner_margin(egui::Margin::symmetric(4, 2))
            .show(self.ui, |ui| {
                ui.horizontal(|ui| {
                    let (rect, resp) =
                        ui.allocate_exact_size(egui::vec2(22.0, 22.0), Sense::click());
                    paint_icon_plate(ui, rect, &resp, false);
                    let resp =
                        crate::a11y::named_control(ui, resp, egui::WidgetType::Button, tooltip);
                    let icon = match edge {
                        seal_editor_core::viewport_overlays::Edge::BottomLeft
                        | seal_editor_core::viewport_overlays::Edge::BottomRight => {
                            crate::icons::Icon::ViewChevronDown
                        }
                        _ => crate::icons::Icon::ViewChevronUp,
                    };
                    crate::icons::paint_icon(
                        ui.painter(),
                        rect.center(),
                        icon,
                        crate::ICON_PX,
                        ui.visuals().text_color(),
                    );
                    if resp.clicked() {
                        hit.toolbar_expand = true;
                    }
                });
            });
        hit
    }
}

/// Tooltip ids for the viewport overlay bar.
pub struct ViewportShowMenu<'a> {
    pub title: &'a str,
    pub defaults_label: &'a str,
    pub layers: &'a std::collections::BTreeMap<String, bool>,
    pub entries: &'a [(&'a str, &'a str)],
    pub menu_open: bool,
}

pub struct ViewportBarTooltips<'a> {
    pub space: &'a str,
    pub surface_snap: &'a str,
    pub grid_snap: &'a str,
    pub angle_snap: &'a str,
    pub scale_snap: &'a str,
    pub camera_speed: &'a str,
    pub maximize: &'a str,
}

fn trim_step(value: f32) -> String {
    if (value - value.round()).abs() < f32::EPSILON {
        format!("{}", value.round() as i32)
    } else {
        format!("{value:.3}").trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

pub(crate) fn paint_row_rule(ui: &egui::Ui) {
    let rect = ui.min_rect();
    if rect.height() <= 0.0 {
        return;
    }
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        Stroke::new(1.0, ui.visuals().extreme_bg_color),
    );
}

pub(crate) fn attach_property_row_context(
    ui: &mut egui::Ui,
    label: &str,
    menu: Option<&[crate::ContextMenuEntry]>,
    menu_pick: &mut Option<crate::ContextMenuPick>,
    field_key: Option<&str>,
) {
    let Some(entries) = menu else {
        return;
    };
    // This runs after the row's controls, so a click-sensing rect here would sit on top of
    // them and swallow their left clicks (fields would not focus, dropdowns would not
    // open). Sense hover only and watch for the right click ourselves.
    let rect = ui.min_rect();
    let response = ui.interact(rect, ui.id().with(("property_row_menu", label)), Sense::hover());
    let right_click =
        response.contains_pointer() && ui.input(|input| input.pointer.secondary_clicked());
    egui::Popup::menu(&response)
        .open_memory(right_click.then_some(egui::SetOpenCommand::Bool(true)))
        .at_pointer_fixed()
        .close_behavior(egui::PopupCloseBehavior::CloseOnClick)
        .show(|ui| {
            if let Some(mut pick) = crate::paint_context_menu(ui, Some(label), entries) {
                if pick.command == "inspector.property_reset" {
                    if let Some(key) = field_key {
                        pick.args.push(("field".into(), key.into()));
                    }
                }
                *menu_pick = Some(pick);
            }
        });
}

pub(crate) fn paint_row_reset_icon(ui: &mut egui::Ui, tooltip: &str) -> bool {
    // Same amber reset as inside the fields, for rows whose control has no room for it.
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(18.0, ui.spacing().interact_size.y), Sense::hover());
    let r = egui::Rect::from_center_size(
        rect.center(),
        egui::vec2(crate::crisp::RESET_PX, crate::crisp::RESET_PX),
    );
    // Rows with equal defaults share a tooltip: the id must follow the row, not the text.
    crate::crisp::reset_button(ui, r, ui.auto_id_with("row-reset"), tooltip)
}

pub fn property_name(ui: &mut egui::Ui, label: &str, bright: bool, help: Option<&str>) -> egui::Id {
    let ratio_id = ui.make_persistent_id("property-ratio");
    let drag_id = ui.make_persistent_id("property-drag");
    let width = ui.available_width().max(1.0);
    // Narrow panels give labels less room so values stay readable; dragging
    // the split still overrides this.
    let mut ratio = ui
        .data(|data| data.get_temp::<f32>(ratio_id))
        .unwrap_or_else(|| (130.0 / width).clamp(0.3, 0.4));
    let split_x = ui.cursor().left() + width * ratio;
    let pointer = ui.input(|input| input.pointer.clone());
    let mut dragging = ui.data(|data| data.get_temp::<bool>(drag_id)).unwrap_or(false);
    if let Some(pos) = pointer.latest_pos() {
        if (pos.x - split_x).abs() <= 4.0 {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            if pointer.primary_pressed() {
                dragging = true;
            }
        }
    }
    if dragging && pointer.primary_down() {
        if let Some(pos) = pointer.latest_pos() {
            ratio = ((pos.x - ui.cursor().left()) / width).clamp(0.25, 0.6);
            ui.data_mut(|data| data.insert_temp(ratio_id, ratio));
        }
    } else if !pointer.primary_down() {
        dragging = false;
    }
    ui.data_mut(|data| {
        data.insert_temp(drag_id, dragging);
        data.insert_temp(ratio_id, ratio);
    });
    let label_width = (width * ratio).max(48.0);
    let (rect, response) = ui
        .allocate_exact_size(egui::vec2(label_width, ui.spacing().interact_size.y), Sense::hover());
    ui.painter().vline(
        rect.right(),
        rect.y_range(),
        Stroke::new(1.0, ui.visuals().extreme_bg_color),
    );
    let response = if let Some(help) = help {
        // Name the full label too: it may be shortened with an ellipsis.
        response.on_hover_ui(|ui| {
            ui.strong(label);
            ui.label(help);
        })
    } else {
        response.on_hover_text(label)
    };
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, label));
    let highlight_id = ui.id().with("seal_property_search_highlight");
    let highlight = ui.data(|data| data.get_temp::<String>(highlight_id));
    let mut ink = if bright { ui.visuals().text_color() } else { ui.visuals().weak_text_color() };
    if let Some(q) = highlight.as_ref() {
        if label.to_lowercase().contains(q) {
            let pad = egui::vec2(2.0, 1.0);
            let hi = rect.expand2(pad);
            ui.painter().rect_filled(hi, 0.0, ui.visuals().selection.bg_fill);
            ink = ui.visuals().text_color();
        }
    }
    // Long labels end in an ellipsis instead of being cut mid-letter; the tooltip
    // carries the full text.
    let mut job = egui::text::LayoutJob::simple_singleline(
        label.to_owned(),
        TextStyle::Body.resolve(ui.style()),
        ink,
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width((rect.width() - 6.0).max(8.0));
    let galley = ui.painter().layout_job(job);
    let pos = egui::pos2(rect.left(), rect.center().y - galley.size().y * 0.5);
    ui.painter().with_clip_rect(rect).galley(pos, galley, ink);
    response.id
}

fn paint_scale_uniform_lock(ui: &mut egui::Ui, label: &str, locked: bool, tip: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(VECTOR_PREFIX_PX, ui.spacing().interact_size.y),
        Sense::click(),
    );
    let response =
        crate::a11y::named_control(ui, response, egui::WidgetType::Button, tip).on_hover_text(tip);
    let ink =
        if locked { ui.visuals().selection.stroke.color } else { ui.visuals().weak_text_color() };
    let icon = if locked { crate::icons::Icon::EditLock } else { crate::icons::Icon::EditUnlock };
    crate::icons::paint_icon(ui.painter(), rect.center(), icon, 12.0, ink);
    let _ = label;
    response.clicked()
}

fn paint_vector_space_combo(
    ui: &mut egui::Ui,
    label: &str,
    mode: VectorRowMode<'_>,
) -> Option<String> {
    let current_text = mode
        .options
        .iter()
        .find(|(space, _)| *space == mode.current)
        .map(|(_, text)| *text)
        .unwrap_or(mode.current);
    let short: String = current_text.chars().take(1).collect();
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(VECTOR_PREFIX_PX, ui.spacing().interact_size.y),
        Sense::click(),
    );
    let tip = format!("{label}: {current_text}");
    let response = crate::a11y::named_control(ui, response, egui::WidgetType::ComboBox, &tip)
        .on_hover_text(&tip);
    let visuals = ui.visuals().clone();
    crate::crisp::raised(
        &mut ui.painter(),
        rect,
        &visuals,
        crate::crisp::state_of(&response, false),
    );
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        short,
        TextStyle::Small.resolve(ui.style()),
        visuals.text_color(),
    );
    paint_focus_ring(ui, &response);
    let mut picked = None;
    egui::Popup::menu(&response).show(|ui| {
        ui.set_min_width(120.0);
        for (space, text) in mode.options {
            if ui.selectable_label(*space == mode.current, *text).clicked()
                && *space != mode.current
            {
                picked = Some((*space).to_string());
            }
        }
    });
    picked
}

pub(crate) fn help_focus_ring(
    ui: &mut egui::Ui,
    response: egui::Response,
    help: &str,
) -> egui::Response {
    crate::a11y::hover_focus_help(ui, response, help)
}

pub(crate) fn paint_focus_ring(ui: &egui::Ui, response: &egui::Response) {
    crate::a11y::paint_focus_ring(ui, response);
}

/// Paint the background of a full-width row. Returns the text color when the fill changes it.
pub(crate) fn row_fill(
    ui: &egui::Ui,
    rect: egui::Rect,
    response: &egui::Response,
    selected: bool,
    panel_focused: bool,
) -> Option<Color32> {
    let visuals = ui.visuals();
    let mut ink = None;
    if selected {
        let fill = if panel_focused {
            visuals.selection.bg_fill
        } else {
            visuals.widgets.inactive.bg_fill
        };
        ui.painter().rect_filled(rect, 0.0, fill);
        if panel_focused {
            ink = Some(Color32::WHITE);
        }
    } else if response.hovered() {
        ui.painter().rect_filled(rect, 0.0, visuals.widgets.hovered.bg_fill);
    }
    paint_focus_ring(ui, response);
    ink
}

fn file_icon(
    painter: &egui::Painter,
    rect: egui::Rect,
    folder: bool,
    folder_ink: Color32,
    file_ink: Color32,
) {
    let icon = if folder { crate::icons::Icon::Folder } else { crate::icons::Icon::File };
    let color = if folder { folder_ink } else { file_ink };
    let size = rect.height().clamp(12.0, 28.0);
    crate::icons::paint_icon(painter, rect.center(), icon, size, color);
}

fn paint_viewport_sky(painter: &egui::Painter, rect: egui::Rect, visuals: &egui::Visuals) {
    let zenith = Color32::from_rgb(0x1a, 0x20, 0x2c);
    let horizon = Color32::from_rgb(0x4a, 0x54, 0x66);
    let bands = 48usize;
    let h = rect.height();
    for band in 0..bands {
        let t0 = band as f32 / bands as f32;
        let t1 = (band + 1) as f32 / bands as f32;
        let mid = (t0 + t1) * 0.5;
        let r = zenith.r() as f32 + (horizon.r() as f32 - zenith.r() as f32) * mid;
        let g = zenith.g() as f32 + (horizon.g() as f32 - zenith.g() as f32) * mid;
        let b = zenith.b() as f32 + (horizon.b() as f32 - zenith.b() as f32) * mid;
        let y0 = rect.top() + h * t0;
        let y1 = rect.top() + h * t1;
        painter.rect_filled(
            egui::Rect::from_min_max(egui::pos2(rect.left(), y0), egui::pos2(rect.right(), y1)),
            0.0,
            Color32::from_rgb(r as u8, g as u8, b as u8),
        );
    }
    let _ = visuals.extreme_bg_color;
}

fn paint_view_axis_gizmo(
    painter: &egui::Painter,
    rect: egui::Rect,
    yaw_deg: f32,
    pitch_deg: f32,
    font: &egui::FontId,
) {
    let center = rect.left_bottom() + egui::vec2(46.0, -46.0);
    let axis_len = 22.0;
    // The same projection the scene uses, so the corner axes always match the view.
    let dir = |p: [f32; 3]| {
        crate::viewport_mesh::project_point_angled(
            p,
            yaw_deg,
            pitch_deg,
            egui::Pos2::ZERO,
            axis_len,
        )
        .0
        .to_vec2()
    };
    let axes = [
        (crate::AXIS_X, dir([1.0, 0.0, 0.0]), "X"),
        (crate::AXIS_Y, dir([0.0, 1.0, 0.0]), "Y"),
        (crate::AXIS_Z, dir([0.0, 0.0, 1.0]), "Z"),
    ];
    for (color, delta, label) in axes {
        if delta.length() < 2.0 {
            // Pointing straight at the viewer: a dot with the label.
            painter.circle_filled(center, 2.5, color);
            painter.text(
                center + egui::vec2(7.0, -7.0),
                Align2::CENTER_CENTER,
                label,
                font.clone(),
                color,
            );
            continue;
        }
        let end = center + delta;
        painter.line_segment([center, end], Stroke::new(2.0, color));
        painter.text(
            end + delta.normalized() * 4.0,
            Align2::CENTER_CENTER,
            label,
            font.clone(),
            color,
        );
    }
}

fn paint_icon_plate(ui: &egui::Ui, rect: egui::Rect, response: &egui::Response, on: bool) {
    crate::crisp::plate(ui, rect, response, on, false);
}

/// Plate for controls that open a menu: always raised so they read as buttons.
fn paint_menu_plate(ui: &egui::Ui, rect: egui::Rect, response: &egui::Response) {
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(response));
    let state =
        if open { crate::crisp::Raised::Pressed } else { crate::crisp::state_of(response, false) };
    crate::crisp::raised(&mut ui.painter(), rect, ui.visuals(), state);
}

/// Play state on the viewport: a thin LED-coloured frame and a raised badge at the top
/// centre, so it is obvious the world is simulating and edits are temporary.
/// The game's on-screen text, top left under the play badge, with a dark
/// edge so it reads on any scene.
/// Where the play view's game screens were drawn last frame (an `egui::Rect` in temp data).
pub const GAME_SCREEN_RECT: &str = "seal-game-screen-rect";

fn game_color(c: [f32; 4]) -> egui::Color32 {
    let b = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
    egui::Color32::from_rgba_unmultiplied(b(c[0]), b(c[1]), b(c[2]), b(c[3]))
}

/// Paint a game screen's draw list into the play view.
pub fn paint_game_screens(
    ctx: &egui::Context,
    painter: &egui::Painter,
    view: egui::Rect,
    draws: &[seal_editor_core::gameui::Draw],
) {
    use seal_editor_core::gameui::Draw;
    let at = |r: &seal_editor_core::gameui::Rect| {
        egui::Rect::from_min_size(
            view.min + egui::vec2(r.x, r.y),
            egui::vec2(r.w.max(0.0), r.h.max(0.0)),
        )
    };
    for d in draws {
        let clip = match d {
            Draw::Fill { clip, .. }
            | Draw::Frame { clip, .. }
            | Draw::Text { clip, .. }
            | Draw::Picture { clip, .. } => at(clip).intersect(view),
        };
        if clip.width() <= 0.0 || clip.height() <= 0.0 {
            continue;
        }
        let p = painter.with_clip_rect(clip);
        match d {
            Draw::Fill { rect, color, corner, .. } => {
                p.rect_filled(at(rect), *corner, game_color(*color));
            }
            Draw::Frame { rect, color, width, corner, .. } => {
                p.rect_stroke(
                    at(rect),
                    *corner,
                    Stroke::new(*width, game_color(*color)),
                    egui::StrokeKind::Inside,
                );
            }
            Draw::Text { rect, text, size, font, color, align, .. } => {
                let r = at(rect);
                let family = crate::game_fonts::game_font(ctx, font)
                    .unwrap_or(egui::FontFamily::Proportional);
                let (anchor, x) = match align.as_str() {
                    "center" => (Align2::CENTER_CENTER, r.center().x),
                    "right" => (Align2::RIGHT_CENTER, r.right()),
                    _ => (Align2::LEFT_CENTER, r.left()),
                };
                p.text(
                    egui::pos2(x, r.center().y),
                    anchor,
                    text,
                    egui::FontId::new(*size, family),
                    game_color(*color),
                );
            }
            Draw::Picture { rect, path, tint, crop, slice, slice_mode, .. } => {
                let r = at(rect);
                match crate::sprites::sprite_texture(ctx, path) {
                    Some((tex, mut place, mut picture)) => {
                        // `crop` picks part of the picture (an inline sprite frame); 9-slice works within it.
                        let mut uv = crate::sprites::uv_within(place, *crop);
                        let color = game_color(*tint);
                        let mut quad =
                            [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()];
                        let mut turned_uvs = None;
                        if let Some(fit) = crate::sprites::fit_sprite(path, quad, *crop) {
                            quad = fit.corners;
                            uv = fit.uv;
                            picture = fit.content_px;
                            place = fit.uv;
                            if fit.turned {
                                turned_uvs = Some(fit.uvs);
                            }
                        }
                        if let Some(uvs) = turned_uvs {
                            crate::sprites::paint_quad_uvs(&p, tex, quad, uvs, color);
                        } else if *slice != [0.0; 4] {
                            let texture_px = [
                                picture[0] / (place[2] - place[0]).max(1e-6),
                                picture[1] / (place[3] - place[1]).max(1e-6),
                            ];
                            let drawn_w = (quad[1].x - quad[0].x).abs();
                            let drawn_h = (quad[3].y - quad[0].y).abs();
                            let nine = crate::sprites::NineSlice {
                                border: *slice,
                                picture: texture_px,
                                scale: [drawn_w, drawn_h],
                            };
                            if slice_mode == "tile" {
                                crate::sprites::paint_nine_tile(&p, tex, quad, uv, color, nine);
                            } else {
                                crate::sprites::paint_nine(&p, tex, quad, uv, color, nine);
                            }
                        } else if quad[0] != r.left_top() || quad[2] != r.right_bottom() {
                            crate::sprites::paint_quad_uv(&p, tex, quad, uv, color);
                        } else {
                            let uv_rect = egui::Rect::from_min_max(
                                egui::pos2(uv[0], uv[1]),
                                egui::pos2(uv[2], uv[3]),
                            );
                            p.image(tex, r, uv_rect, color);
                        }
                    }
                    None => {
                        p.rect_stroke(
                            r,
                            0.0,
                            Stroke::new(1.0, egui::Color32::from_gray(140)),
                            egui::StrokeKind::Inside,
                        );
                    }
                }
            }
        }
    }
}

fn paint_game_ui(painter: &egui::Painter, rect: egui::Rect, lines: &[String]) {
    let font = egui::FontId::proportional(16.0);
    let mut y = rect.top() + 44.0;
    for line in lines {
        let at = egui::pos2(rect.left() + 16.0, y);
        let r = painter.text(
            at + egui::vec2(1.0, 1.0),
            Align2::LEFT_TOP,
            line,
            font.clone(),
            egui::Color32::from_black_alpha(200),
        );
        painter.text(
            at,
            Align2::LEFT_TOP,
            line,
            font.clone(),
            egui::Color32::from_rgb(0xf2, 0xf2, 0xf2),
        );
        y += r.height() + 4.0;
        if y > rect.bottom() - 20.0 {
            break;
        }
    }
}

fn paint_play_badge(
    painter: &egui::Painter,
    rect: egui::Rect,
    label: &str,
    running: bool,
    font: &egui::FontId,
    frame: bool,
) {
    let led = if running {
        egui::Color32::from_rgb(0x4c, 0xd0, 0x64)
    } else {
        egui::Color32::from_rgb(0xe8, 0xa8, 0x3a)
    };
    if frame {
        painter.rect_stroke(
            rect.shrink(1.0),
            0.0,
            egui::Stroke::new(2.0, led.gamma_multiply(0.75)),
            egui::StrokeKind::Inside,
        );
    }
    let text = painter.layout_no_wrap(label.to_uppercase(), font.clone(), egui::Color32::WHITE);
    let size = egui::vec2(text.size().x + 30.0, 20.0);
    let badge = egui::Rect::from_center_size(
        egui::pos2(rect.center().x, rect.top() + 14.0 + size.y * 0.5),
        size,
    );
    painter.rect_filled(badge, 1.0, egui::Color32::from_rgb(0x2a, 0x2a, 0x2a));
    painter.rect_stroke(
        badge,
        1.0,
        egui::Stroke::new(1.0, egui::Color32::from_rgb(0x46, 0x46, 0x46)),
        egui::StrokeKind::Inside,
    );
    let dot = egui::pos2(badge.left() + 11.0, badge.center().y);
    painter.circle_filled(dot, 5.0, led.gamma_multiply(0.18));
    painter.circle_filled(dot, 3.0, led);
    painter.galley(
        egui::pos2(badge.left() + 20.0, badge.center().y - text.size().y * 0.5),
        text,
        egui::Color32::from_rgb(0xe6, 0xe6, 0xe6),
    );
}

/// A rename field starts with the whole old name selected, so typing replaces it.
/// Call before showing a rename edit with id `id`. On the first frame of a rename (the edit
/// was not shown last frame) it focuses the edit and selects all of its text, so the first
/// typed letter replaces the old name instead of being added to it.
/// Starts a rename with the whole name selected. This egui build collapses a stored selection
/// before the edit runs, so the "selected" state is kept here: the first typed text or
/// Backspace replaces the whole name, and a cursor key or click keeps it and edits in place.
pub(crate) fn begin_rename_edit(ctx: &egui::Context, id: egui::Id, text: &mut String) -> bool {
    let pass = ctx.cumulative_pass_nr();
    let seen = id.with("rename-pass");
    let fresh = id.with("rename-fresh");
    let last = ctx.data(|d| d.get_temp::<u64>(seen));
    ctx.data_mut(|d| d.insert_temp(seen, pass));
    if !last.is_some_and(|last| last + 1 >= pass) {
        let mut state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
        state.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(
            text.chars().count(),
        ))));
        state.store(ctx, id);
        ctx.memory_mut(|m| m.request_focus(id));
        ctx.data_mut(|d| d.insert_temp(fresh, true));
        return true;
    }
    if !ctx.data(|d| d.get_temp::<bool>(fresh)).unwrap_or(false) {
        return false;
    }
    let (typed, erase, moved) = ctx.input(|i| {
        let typed =
            i.events.iter().any(|e| matches!(e, egui::Event::Text(_) | egui::Event::Paste(_)));
        let erase = i.key_pressed(egui::Key::Backspace) || i.key_pressed(egui::Key::Delete);
        let moved = [egui::Key::ArrowLeft, egui::Key::ArrowRight, egui::Key::Home, egui::Key::End]
            .iter()
            .any(|k| i.key_pressed(*k))
            || i.pointer.any_pressed();
        (typed, erase, moved)
    });
    if typed || erase {
        text.clear();
        let mut state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(0))));
        state.store(ctx, id);
        if erase {
            ctx.input_mut(|i| {
                i.consume_key(egui::Modifiers::NONE, egui::Key::Backspace);
                i.consume_key(egui::Modifiers::NONE, egui::Key::Delete);
            });
        }
    }
    if typed || erase || moved {
        ctx.data_mut(|d| d.insert_temp(fresh, false));
        return false;
    }
    true
}

/// Paints the select-all highlight over a rename edit while its name is still fully selected.
pub(crate) fn paint_rename_selection(
    ui: &egui::Ui,
    rect: egui::Rect,
    text: &str,
    font: TextStyle,
    centered: bool,
) {
    if text.is_empty() {
        return;
    }
    let font_id = font.resolve(ui.style());
    let width = ui
        .painter()
        .layout_no_wrap(text.to_string(), font_id.clone(), egui::Color32::WHITE)
        .size()
        .x;
    let height = font_id.size + 2.0;
    let left = if centered { rect.center().x - width * 0.5 } else { rect.left() + 4.0 };
    let band = egui::Rect::from_min_size(
        egui::pos2(left - 1.0, rect.center().y - height * 0.5),
        egui::vec2(width + 2.0, height),
    );
    let fill = ui.visuals().selection.bg_fill.gamma_multiply(0.55);
    ui.painter().with_clip_rect(rect).rect_filled(band, 0.0, fill);
}

/// The angled view's ground grid and axes, turned and tilted exactly like the
/// shapes in the scene so the floor, the grid and the objects always agree.
/// Lines lie on the floor (height 0); when the view is nearly level the floor
/// is edge-on, so the grid stands up as the back wall (depth 0) instead.
/// Pixels per scene unit for a camera, the same formula the viewport draws with.
fn camera_cell(camera: &ViewportCamera, perspective: bool) -> f32 {
    let dolly = if perspective { DEFAULT_ORBIT_DISTANCE / camera.distance.max(1.0) } else { 1.0 };
    let pitch_scale =
        if perspective { camera.pitch_deg.to_radians().cos().abs().max(0.35) } else { 1.0 };
    20.0 * camera.zoom * dolly * pitch_scale
}

/// Screen offset (right, up) of a scene point from the origin, in scene units.
fn camera_plane(p: [f32; 3], camera: &ViewportCamera, perspective: bool) -> (f32, f32) {
    if perspective {
        let q = crate::viewport_mesh::project_point_angled(
            p,
            camera.yaw_deg,
            camera.pitch_deg,
            egui::Pos2::ZERO,
            1.0,
        )
        .0;
        (q.x, -q.y)
    } else {
        let (s, c) = camera.yaw_deg.to_radians().sin_cos();
        (p[0] * c - p[1] * s, p[0] * s + p[1] * c)
    }
}

/// After the camera turned, move its pan so `anchor` (or, without one, the scene
/// point that was in the middle of the viewport) sits where it was on screen.
pub(crate) fn keep_point_on_screen(
    camera: &mut ViewportCamera,
    before: &ViewportCamera,
    rect: egui::Rect,
    perspective: bool,
    anchor: Option<[f32; 3]>,
) {
    let cell0 = camera_cell(before, perspective);
    let origin0 = rect.center() + egui::vec2(before.pan_x, before.pan_y);
    let (point, screen) = match anchor {
        Some(p) => {
            let (r, u) = camera_plane(p, before, perspective);
            (p, origin0 + egui::vec2(r * cell0, -u * cell0))
        }
        None => {
            let r = (rect.center().x - origin0.x) / cell0;
            let u = (origin0.y - rect.center().y) / cell0;
            let p = if perspective {
                // The centre ray at the origin's depth: rotate (r, u, 0) back out of view space.
                let (sy, cy) = before.yaw_deg.to_radians().sin_cos();
                let (sp, cp) = before.pitch_deg.to_radians().sin_cos();
                let right = [cy, 0.0, -sy];
                let upv = [-sy * sp, cp, -cy * sp];
                [r * right[0] + u * upv[0], r * right[1] + u * upv[1], r * right[2] + u * upv[2]]
            } else {
                let (s, c) = before.yaw_deg.to_radians().sin_cos();
                [r * c + u * s, -r * s + u * c, 0.0]
            };
            (p, rect.center())
        }
    };
    let cell1 = camera_cell(camera, perspective);
    let (r, u) = camera_plane(point, camera, perspective);
    let origin1 = screen - egui::vec2(r * cell1, -u * cell1);
    camera.pan_x = origin1.x - rect.center().x;
    camera.pan_y = origin1.y - rect.center().y;
}

fn paint_angled_grid(
    painter: &egui::Painter,
    rect: egui::Rect,
    yaw_deg: f32,
    pitch_deg: f32,
    origin: egui::Pos2,
    cell: f32,
    grid: bool,
    ink: Color32,
) {
    let floor = pitch_deg.to_radians().sin().abs() >= 0.12;
    let at = |a: f32, b: f32| -> egui::Pos2 {
        let p = if floor { [a, 0.0, b] } else { [a, b, 0.0] };
        crate::viewport_mesh::project_point(p, yaw_deg, pitch_deg, origin, cell).0
    };
    let reach = ((rect.width().hypot(rect.height()) / cell.max(0.01))
        / pitch_deg.to_radians().sin().abs().max(0.12))
    .clamp(10.0, 2000.0);
    let stride = if reach > 400.0 { 10 } else { 1 };
    let n = (reach / 10.0).ceil() as i32 * 10;
    if grid {
        let mut step = -n;
        while step <= n {
            let major = step.rem_euclid(10) == 0;
            let fade = (1.0 - (step.unsigned_abs() as f32 / 40.0).min(1.0)) * 0.5 + 0.5;
            let strength = if major { 0.18 * fade } else { 0.06 * fade };
            let stroke = Stroke::new(1.0, ink.gamma_multiply(strength));
            let v = step as f32;
            let r = n as f32;
            painter.line_segment([at(v, -r), at(v, r)], stroke);
            painter.line_segment([at(-r, v), at(r, v)], stroke);
            step += stride;
        }
    }
    paint_angled_axes(painter, yaw_deg, pitch_deg, origin, cell, n as f32, 1.0);
}

/// The world X, Y and Z lines of the angled view; `alpha` below 1 for the
/// semi-transparent copy drawn over the scene.
fn paint_angled_axes(
    painter: &egui::Painter,
    yaw_deg: f32,
    pitch_deg: f32,
    origin: egui::Pos2,
    cell: f32,
    r: f32,
    alpha: f32,
) {
    let floor = pitch_deg.to_radians().sin().abs() >= 0.12;
    let at = |a: f32, b: f32| -> egui::Pos2 {
        let p = if floor { [a, 0.0, b] } else { [a, b, 0.0] };
        crate::viewport_mesh::project_point(p, yaw_deg, pitch_deg, origin, cell).0
    };
    painter.line_segment(
        [at(-r, 0.0), at(r, 0.0)],
        Stroke::new(1.5, crate::AXIS_X.gamma_multiply(alpha)),
    );
    let second = if floor { crate::AXIS_Z } else { crate::AXIS_Y };
    painter.line_segment([at(0.0, -r), at(0.0, r)], Stroke::new(1.5, second.gamma_multiply(alpha)));
    if floor {
        let up = |h: f32| {
            crate::viewport_mesh::project_point([0.0, h, 0.0], yaw_deg, pitch_deg, origin, cell).0
        };
        painter.line_segment(
            [up(0.0), up(r)],
            Stroke::new(1.5, crate::AXIS_Y.gamma_multiply(0.6 * alpha)),
        );
    }
}

/// What dragging a Asset Browser file places: pictures become sprite objects,
/// prefabs place a copy, effects make a particle object and logic graphs become
/// the script of the object they are dropped on. Other files are not dragged.
pub fn content_drag_kind(name: &str) -> Option<String> {
    let lower = name.to_ascii_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or("");
    let kind = match ext {
        ext if lower.contains('.') && seal_editor_core::picture::is_extension(ext) => "sprite",
        "prefab" => "prefab",
        "fx" => "fx",
        "graph" => "graph",
        _ => return None,
    };
    Some(format!("{kind}:{name}"))
}

#[cfg(test)]
mod tests {
    use super::PanelFrame;

    fn screen_of(
        p: [f32; 3],
        cam: &super::ViewportCamera,
        rect: egui::Rect,
        persp: bool,
    ) -> egui::Pos2 {
        let cell = super::camera_cell(cam, persp);
        let (r, u) = super::camera_plane(p, cam, persp);
        rect.center() + egui::vec2(cam.pan_x, cam.pan_y) + egui::vec2(r * cell, -u * cell)
    }

    #[test]
    fn turning_after_a_pan_orbits_the_view_centre_not_the_world_origin() {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        for persp in [true, false] {
            let before = super::ViewportCamera {
                pan_x: -260.0,
                pan_y: 90.0,
                zoom: 1.0,
                yaw_deg: 20.0,
                pitch_deg: -15.0,
                distance: super::DEFAULT_ORBIT_DISTANCE,
            };
            // The scene point under the middle of the screen before the turn.
            let mut cam = before;
            cam.yaw_deg += 50.0;
            cam.pitch_deg -= 25.0;
            super::keep_point_on_screen(&mut cam, &before, rect, persp, None);
            // A point on the centre ray must come back to the centre.
            let cell = super::camera_cell(&before, persp);
            let origin = rect.center() + egui::vec2(before.pan_x, before.pan_y);
            let (r, u) = ((rect.center().x - origin.x) / cell, (origin.y - rect.center().y) / cell);
            let p = if persp {
                let (sy, cy) = before.yaw_deg.to_radians().sin_cos();
                let (sp, cp) = before.pitch_deg.to_radians().sin_cos();
                [r * cy - u * sy * sp, u * cp, -r * sy - u * cy * sp]
            } else {
                let (s, c) = before.yaw_deg.to_radians().sin_cos();
                [r * c + u * s, -r * s + u * c, 0.0]
            };
            assert!(screen_of(p, &before, rect, persp).distance(rect.center()) < 0.01);
            assert!(
                screen_of(p, &cam, rect, persp).distance(rect.center()) < 0.05,
                "persp={persp}"
            );
            // The world origin, by contrast, moved: the turn is not around 0 0 0.
            assert!(
                screen_of([0.0; 3], &cam, rect, persp)
                    .distance(screen_of([0.0; 3], &before, rect, persp))
                    > 5.0
            );
        }
    }

    #[test]
    fn alt_orbit_keeps_the_selection_in_place() {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let before = super::ViewportCamera {
            pan_x: 120.0,
            pan_y: -40.0,
            zoom: 1.3,
            yaw_deg: 0.0,
            pitch_deg: -20.0,
            distance: super::DEFAULT_ORBIT_DISTANCE,
        };
        let pivot = [6.0, 2.0, 0.0];
        let mut cam = before;
        cam.yaw_deg = 70.0;
        super::keep_point_on_screen(&mut cam, &before, rect, true, Some(pivot));
        let a = screen_of(pivot, &before, rect, true);
        let b = screen_of(pivot, &cam, rect, true);
        assert!(a.distance(b) < 0.05);
    }

    #[test]
    fn a_color_swatch_paints_without_a_click() {
        let mut clicked = true;
        crate::run_frame(crate::default_input(Vec::new()), |ui| {
            clicked = PanelFrame::new(ui).swatch("Accent", [12, 34, 56]);
        });
        assert!(!clicked);
    }
}
