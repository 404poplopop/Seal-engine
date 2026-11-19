//! Headless editor model. This crate has no user-interface dependency.
#![forbid(unsafe_code)]

pub mod agent_link;
pub mod aseprite;
mod asset_browser;
pub mod asset_database_ops;
pub mod asset_editor;
pub mod asset_index;
pub mod asset_preview;
pub mod atlas;
mod build_settings;
mod catalog;
mod catalog_ids;
mod cli;
mod command;
mod content;
pub mod data_editor;
mod diagnostics;
mod dirty;
mod document;
pub mod editor_remote;
mod editor_tools;
mod error;
pub mod fault_io;
pub mod import_activity;
pub mod naming_rules;
pub mod preview_providers;
pub mod script_inspector;
pub mod viewport_buffer;
pub use seal_graph::inspector_layout::{
    plan_rows, InspectorFieldRule, InspectorLayout, InspectorRowPlan,
};
pub use seal_world::data_asset::{
    is_data_path, DataAsset, DataFieldDef, DataTypeDef, FieldKind, DATATYPE_EXT, DATA_EXT,
};
mod composite_texture_cmds;
mod fx_curve;
pub mod layered_picture;
mod particle_fx_fields;
pub mod particle_lit;
pub mod play_textures;
mod prefab_data_cmds;
mod prefab_variant;
pub mod project_tutorials;
mod recipes;
mod template_first_person;
mod template_third_person;
pub mod template_view3d;
pub mod texture_asset_cmd;
mod texture_ktx_export;
pub mod texture_memory;
mod tutorial_hooks;
mod user_templates;
mod watcher_reimport;
pub use fx_curve::FX_FIELD_EDIT;
pub mod asset_thumbnails;
mod collider_edit;
mod editor_language;
pub mod export_preset;
mod foundation;
pub mod functional_tests;
pub mod game_pack;
pub mod graph_doc;
pub mod hdr_window;
pub mod import_pipeline;
pub mod import_platform;
pub mod import_presets;
pub mod input_editor;
mod input_live;
pub mod inspector_fields;
pub mod journal;
pub mod journal_crc;
mod keymap;
mod keymap_profiles;
mod kinds;
mod launch;
mod layout;
pub mod light2d;
mod light2d_cmd;
mod list_field;
pub mod measure_math;
mod menu_mnemonic;
mod messages;
mod models;
mod module;
pub mod msix_pack;
mod outliner_query;
pub mod package;
mod panel;
mod persist;
mod physics_cmd;
pub mod pickers;
pub mod picture_import;
mod picture_pack;
mod platform_overrides;
mod play;
mod play_tint;
mod project;
mod project_audio;
mod project_dimension;
mod render_quality;
pub mod save_editor;
mod save_fault;
mod scene_pack;
mod scene_thumbnail;
pub mod sequences;
mod session;
mod settings;
pub mod shape_ops;
mod sound_export;
pub mod soundset_editor;
mod sprite_shape_cmd;
pub mod strings_editor;
mod template_registry;
mod theme;
pub mod tile_set_editor;
pub mod tile_tools;
pub mod tiles;
pub mod timeline_doc;
pub mod ui_designer;
mod version;
mod viewport;
pub mod viewport_isolate;
mod viewport_layer_cull;
pub mod viewport_overlays;
pub mod viewport_selection;
pub mod viewport_split;
mod when;
pub mod window_game;
mod wizard;
mod workspace;
mod writer;

pub use asset_browser::{
    browser_items, current_rel, folder_rows, kind_of, BrowserItem, BrowserState, SortBy,
    FILTER_KINDS,
};
pub use build_settings::{
    discover_scenes, read_build_scenes, write_build_scenes, BUILD_SCENES_KEY,
};
pub use catalog::SPEC_COMMANDS;
pub use catalog_ids::{
    english_catalog_source_roots, referenced_dynamic_ids, verify_english_catalog,
};
pub use cli::{execute_cli, parse_cli, CliOutput, CliRequest};
pub use command::{
    palette_hits, ArgSpec, CommandArgs, CommandRegistry, CommandSpec, PaletteHit, PaletteMode,
    MENU_CATEGORIES,
};
pub use content::{content_tree_rows, list_current_folder, ContentEntry, ContentInlineError};
pub use diagnostics::{install_panic_hook, mark_clean_exit, take_unclean_start, write_diagnostics};
pub use dirty::{dirty_marks, panel_is_dirty, tab_label, DirtyMarks, DirtyStatus};
pub use document::{JournalStep, TextDocument, TextOp};
pub use editor_tools::{
    active_workers, diff_tool_argv, external_editor_argv, map_in_order, note_worker_budget,
    sanitize_worker_threads, worker_thread_count, DEFAULT_DIFF_ARGS, DEFAULT_EXTERNAL_ARGS,
    DIFF_TOOL_ARGS, DIFF_TOOL_PATH, EXTERNAL_EDITOR_ARGS, EXTERNAL_EDITOR_PATH, WORKER_THREADS,
};
pub use error::CoreError;
pub use foundation::{
    BackgroundTask, DockSide, EditorEvent, Foundation, Modal, ModalChoice, ModalKind, Notice,
    StatusSnapshot,
};
pub use import_presets::sound_load_type;
pub use inspector_fields::{
    changed_from_default, clamp_number_field, collision_layer_labels, field_spec, join_layer_names,
    load_collision_layer_names, metadata_row, project_collision_layer_names,
    project_sorting_layer_names, sanitize_enum_field, sorting_layer_labels, split_layer_names,
    store_collision_layer_names, store_sorting_layer_names, unit_message_key, InspectorFieldKind,
    InspectorFieldSpec, InspectorUnit, MetadataRow, NumberWidget, COLLISION_LAYER_SETTING,
    INSPECTOR_FIELD_SPECS, SORTING_LAYER_SETTING,
};
pub use journal_crc::{append_record, read_records, JournalRecord};
pub use keymap::{Chord, KeyResolution, Keymap};
pub use keymap_profiles::{
    binding_conflicts, compare_saved_enabled, export_profile, fix_conflict_drop_first,
    import_profile, open_external_enabled, profile_file_path, profile_index_path, profiles_dir,
    save_profile, switch_profile, KeymapConflict, ProfileIndex,
};
pub use kinds::{DocumentKind, KindRegistry};
pub use launch::{startup_choice, LaunchOptions, StartupChoice, USAGE};
pub use layout::{
    monitor_info_from_physical, resolve_window, Axis, DockNode, FloatingWindow, Layout,
    MonitorInfo, WindowPlacement, NARROW_LAYOUT_MAX_WIDTH, NARROW_RIGHT_COLUMN_PX,
};
pub use list_field::{
    apply_list_edit, default_element, field_check as list_field_check, list_spec_for_field,
    parse_list_edit_op, parse_list_field, serialize_list_field, set_list_element, ListEditOp,
    ListElemKind, ListFieldSpec, ListRow,
};
pub use menu_mnemonic::{menu_category_mnemonic, menu_mnemonic_parts};
pub use messages::Messages;
pub use models::{
    collect_log_lines, console_level_counts, default_viewport_show_layers, delete_content,
    ensure_viewport_tab, format_log_line, inspector_component_entries, inspector_field_default,
    inspector_open_sections_for_query, inspector_section_match_keys, list_folder, log_records,
    outliner_component_keys, outliner_drop_verdict, seed_outliner_stress_rows, visible_range,
    EditorModels, InspectorViewOptions, LogLine, MockNode, OutlinerActiveFilters, OutlinerDropMode,
    OutlinerTable, OutlinerViewOptions, OutlinerVisibleRow, CONSOLE_ROW_DRAFT,
    OUTLINER_STRESS_ROW_COUNT, VIEWPORT_SHOW_LAYER_IDS,
};
pub use module::{CorePanels, EditorModule, ModuleHost};
pub use outliner_query::{
    outliner_asset_edit, outliner_filter_type_key, outliner_matches, outliner_row_marks,
    outliner_type_display, outliner_type_text, OutlinerAssetEdit, OutlinerRowMarks,
};
pub use panel::{
    allows_multiple_instances, next_instance_id, panel_base, panel_tab_title, viewport_tab_seed,
    DockPreference, PanelDesc, PanelRegistry, PANEL_INSTANCE_SEP,
};
pub use persist::{
    atomic_write, clear_focus_request, read_focus_request, write_focus_request, EditorLock,
};
pub use pickers::{
    apply_record_op, asset_drag_payload, asset_field, asset_path_from_drag, asset_reference,
    collect_assets, colour_field, default_for_kind, entity_ref_field, eyedropper_armed,
    filter_assets, graph_string_records, hex_to_rgba, hsv_to_rgb, load_record, looks_like_record,
    object_reference, objects_from_nodes, palette_search_hits, place_body_from_drag,
    recent_colours, record_path, remember_colour_list, rgb_to_hsv, rgba_to_hex,
    save_recent_colours, saved_searches, serialize_record, split_search_activation, AssetCandidate,
    FieldValue, Hsv, RecordOp, Rgba,
};
pub use play::{NoPlay, PlaySession};
pub use play_tint::{
    blend_play_tint, effective_editor_theme, format_play_tint_color, parse_play_tint_color,
    parse_play_tint_strength, play_tint_active, play_tint_applies, play_tint_look_token,
    play_tint_settings, PlayTintSettings, DEFAULT_PLAY_TINT_COLOR, DEFAULT_PLAY_TINT_STRENGTH,
};
pub use project::{
    create_project, create_project_with_options, open_project, read_recent, repair_project,
    touch_recent, CreateProjectOptions, OpenProject, PROJECT_SCHEMA,
};
pub use project_audio::{
    apply_to_mixer, backend_choices, backend_label, bus_gains, effective_output, is_mix_setting,
    parsed_output_sample_rate, AudioOutputSettings, EFFECTS_BUS, EFFECTS_VOLUME, MASTER_VOLUME,
    MUSIC_BUS, MUSIC_VOLUME, OUTPUT_BACKEND, OUTPUT_BUFFER, OUTPUT_SAMPLE_RATE,
};
pub use save_fault::{classify_save, plan_save_failure, SaveFault, SavePlan};
pub use seal_render::clip2d;
pub use seal_render::gi;
pub use seal_render::gpu_block_format::{decode_block_to_rgba8, encode_export_rgba};
/// Bounced light for the probes, gathered by ray casts.
pub use seal_render::picture;
/// Irradiance probes, used by the editor viewport's lit view for fill light.
pub use seal_render::probes;
/// CPU shadow maps, used by the editor viewport's lit view.
pub use seal_render::shadow;
pub use seal_render::texture_encode::encode_rgba as encode_block_rgba;
pub use seal_render::{read_ktx2, write_ktx2, EncodeQuality};
pub use seal_render::{ColourSpace, TextureWrap};
pub use seal_render::{GpuBlockFormat, TextureAsset, TextureAssetKind};
pub use seal_world::area_effect;
pub use seal_world::cam_drag;
pub use seal_world::camera2d;
pub use seal_world::character2d;
pub use seal_world::collider_edit as collider_points;
pub use seal_world::controls;
pub use seal_world::curve;
pub use seal_world::curve_text;
pub use seal_world::fonts;
pub use seal_world::gameui;
pub use seal_world::material2d;
pub use seal_world::nav2d;
pub use seal_world::object_files;
pub use seal_world::object_merge;
pub use seal_world::phys2d;
pub use seal_world::post_volume;
pub use seal_world::prefab_nest;
pub use seal_world::remote_transform;
pub use seal_world::script_groups;
pub use seal_world::seal_input;
pub use seal_world::shape2d;
pub use seal_world::shape_style;
pub use seal_world::sheet;
pub use seal_world::sound_container;
pub use seal_world::sprite_mask;
pub use seal_world::sprite_pivot;
pub use seal_world::sprite_shape;
pub use seal_world::sprite_slice;
pub use seal_world::sprite_sort;
pub use seal_world::volumes;
pub use user_templates::{
    apply_user_template, list_user_templates, save_project_as_template, user_template_by_id,
    UserTemplate,
};
pub use viewport_layer_cull::{filter_draw_list, layer_visible, parse_cull_mask};

/// Collision shapes the 2D Physics section offers.
pub fn physics2d_shapes() -> &'static [&'static str] {
    seal_world::phys2d::SHAPES
}

/// Surfaces the 2D Physics section offers; `custom` uses the object's own friction and bounce.
pub fn physics2d_surfaces() -> &'static [&'static str] {
    &["custom", "ice", "rubber", "metal", "wood", "stone", "bouncy"]
}

/// Joint kinds the 2D Physics section offers, with `none` first.
pub fn physics2d_joints() -> &'static [&'static str] {
    &[
        "none", "fixed", "hinge", "slider", "distance", "rope", "spring", "wheel", "friction",
        "relative", "target",
    ]
}
pub use editor_language::{
    catalog_label_key, AVAILABLE as EDITOR_LANGUAGES, SETTING as EDITOR_LANGUAGE_SETTING,
};
pub use input_live::{InputDevice, InputLive, INPUT_LIVE_KEY, MOUSE_LOOK_RAW, PAD_LIGHT};
pub use platform_overrides::{
    effective_from_session, effective_value, host_platform, load as load_platform_overrides,
    store as store_platform_overrides, PlatformOverrides, PLATFORMS as EXPORT_PLATFORMS,
    STORE_KEY as PLATFORM_OVERRIDE_STORE_KEY,
};
pub use project_dimension::{
    apply_template_defaults, bootstrap_new_scene, is_2d as project_is_2d,
    SETTING as PROJECT_DIMENSION_SETTING,
};
pub use render_quality::{
    apply_level_to_session, editor_shadow_map_size, from_session as render_quality_from_session,
    is_render_field, load_fields as load_render_fields, parse_runner as parse_render_runner,
    project_msaa, runner_text as render_runner_text, sync_world_glow, RenderQuality, QUALITY_LEVEL,
    RENDER_FILE,
};
pub use seal_world::frames as sprite_frames;
pub use seal_world::scripts::key_name as script_key_name;
pub use seal_world::scripts::MusicCue as ScriptMusic;
pub use seal_world::scripts::SoundCue as ScriptSound;
pub use seal_world::seal_scene::sequence::Sequence;
pub use seal_world::sprite_socket;
pub use seal_world::text_sprites::{self as text_sprites, StringTable};
pub use seal_world::{
    active_listener, ActiveListener, Entity, PlayInput, PlayMode, PlayStatus, Shape, World,
};
pub use seal_world::{
    parallax_text, parse_limits, parse_parallax, parse_slice, parse_tint, slice_text, tint_hex,
};
pub use session::{
    apply_default_workspace, apply_layout_reset, autosave_due, EditorSession, HostAction,
    RecoveredDocument,
};
pub use settings::{
    body_font_px, motion_seconds, resolved_theme, setting_catalog, setting_matches_query,
    SettingMeta, SettingsStore,
};
pub use theme::{
    contrast_ratio, load_theme, look_from_theme, parse_theme, rgb_from_theme, theme_look,
    theme_read_token, theme_rgb, theme_role_color, theme_text, LookColors, BUILTIN_THEME_IDS,
    THEME_ROLE_NAMES, THEME_SURFACE_ROLES, THEME_TEXT_ROLES,
};
pub use version::{ENGINE_HASH, ENGINE_VERSION, ENGINE_VERSION_FULL, PACKAGE_VERSION};
pub use viewport::{
    orbit, pan, zoom_distance, NativeSurfaceId, PhysicalSize, ReservedViewport, ViewportHost,
    ViewportInput,
};
pub use when::{when_allows, when_matches, WhenContext};
pub mod template_platformer;
pub mod template_puzzle;
pub mod template_topdown;
pub use project_tutorials::{load_done, load_guide, TutorialGuide, TutorialStep};
pub use recipes::{recipe_by_id, Recipe, RECIPES};
pub use template_registry::{apply_template, template_by_id, ProjectTemplate, PROJECT_TEMPLATES};
pub use wizard::{
    default_projects_dir, open_wizard, validate_wizard, wizard_name, wizard_template,
    WizardValidation, DRAFT_CONFIRM_NONEMPTY, DRAFT_FOLDER, DRAFT_GIT, DRAFT_LFS, DRAFT_NAME,
    DRAFT_TEMPLATE,
};
pub use workspace::{
    portable_root_from_exe, prepare_user_state_dir, user_state_dir, LoadOrigin, WorkspaceFile,
    SCHEMA_VERSION,
};

/// Run one registered command without opening a window.
///
/// # Examples
///
/// ```
/// use seal_editor_core::{headless_command, CommandArgs};
/// assert!(headless_command("edit.select_all", &CommandArgs::default()).is_ok());
/// ```
pub fn headless_command(id: &str, args: &CommandArgs) -> Result<(), CoreError> {
    let mut session = EditorSession::new(crate::layout::Layout::default());
    session.dispatch(id, args)
}
pub use writer::{BackgroundWriter, SaveKind};
