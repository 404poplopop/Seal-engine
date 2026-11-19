//! Read and write one animatable field on a live object.
//!
//! Paths are `component.field` or `component.field.channel`. A path this build
//! does not know is left untouched so the caller can warn once.

use seal_scene::sequence::PropValue;

use crate::Entity;

/// Paths the Timeline can offer for `entity`, in a stable order.
pub fn entity_field_paths(entity: &Entity) -> Vec<&'static str> {
    let mut out = Vec::new();
    out.extend(TRANSFORM);
    out.extend(LOOK);
    // Offer a component's fields only on objects that have it (no camera fields on a sprite).
    if entity.tag == "light" {
        out.extend(LIGHT);
    }
    if entity.tag == "camera" {
        out.extend(CAMERA);
    }
    if entity.shape.is_some() {
        out.extend(BODY);
    }
    out.extend(EDITOR);
    if !entity.sprite.is_empty() {
        out.extend(SPRITE);
    }
    if entity.camera2d.on || entity.tag == "camera" {
        out.extend(CAMERA2D);
    }
    if !entity.light2d.kind.is_empty() || entity.tag == "light" {
        out.extend(LIGHT2D);
    }
    if !entity.phys2d.body.is_empty() {
        out.extend(PHYSICS2D);
    }
    if entity.nav2d.on {
        out.extend(NAV);
    }
    if !entity.shape2d.kind.is_empty() {
        out.extend(SHAPE2D);
    }
    if !entity.follow.path.is_empty() {
        out.extend(FOLLOW);
    }
    if !entity.material2d.effect.is_empty() {
        out.extend(MATERIAL);
    }
    out
}

const TRANSFORM: &[&str] = &[
    "transform.translation",
    "transform.translation.x",
    "transform.translation.y",
    "transform.translation.z",
    "transform.rotation",
    "transform.rotation.x",
    "transform.rotation.y",
    "transform.rotation.z",
    "transform.scale",
    "transform.scale.x",
    "transform.scale.y",
    "transform.scale.z",
];
const LOOK: &[&str] = &[
    "look.shape",
    "look.color",
    "look.color.r",
    "look.color.g",
    "look.color.b",
    "look.roughness",
    "look.metal",
];
const LIGHT: &[&str] =
    &["light.intensity", "light.color", "light.color.r", "light.color.g", "light.color.b"];
const CAMERA: &[&str] = &["camera.fov", "camera.near", "camera.far"];
const BODY: &[&str] = &["body.dynamic", "body.friction", "body.restitution", "body.walk_speed"];
const EDITOR: &[&str] = &["editor.hidden", "editor.locked"];
const SPRITE: &[&str] = &[
    "sprite_image.image",
    "sprite_image.layer",
    "sprite_image.frames",
    "sprite_image.fps",
    "sprite_image.animation",
    "sprite_image.flip_x",
    "sprite_image.flip_y",
    "sprite_image.tint",
    "sprite_image.tint.r",
    "sprite_image.tint.g",
    "sprite_image.tint.b",
    "sprite_image.tint.a",
    "sprite_image.opacity",
    "sprite_image.order",
    "sprite_image.slice",
    "sprite_image.slice_mode",
    "sprite_image.slice_size",
    "sprite_image.pivot",
    "sprite_image.sort_mode",
    "sprite_image.normal_map",
    "sprite_image.parallax",
    "sprite_image.repeat_x",
];
const CAMERA2D: &[&str] = &[
    "camera2d.on",
    "camera2d.follow",
    "camera2d.smoothing",
    "camera2d.zoom",
    "camera2d.limits",
    "camera2d.pixel_snap",
];
const LIGHT2D: &[&str] = &[
    "light2d.kind",
    "light2d.range",
    "light2d.cone",
    "light2d.shadows",
    "light2d.blend",
    "light2d.falloff",
    "light2d.points",
];
const PHYSICS2D: &[&str] = &[
    "physics2d.body",
    "physics2d.shape",
    "physics2d.size",
    "physics2d.layer",
    "physics2d.mask",
    "physics2d.one_way",
    "physics2d.surface",
    "physics2d.ccd",
    "physics2d.joint",
    "physics2d.joint_to",
    "physics2d.anchor",
    "physics2d.other_anchor",
    "physics2d.axis",
    "physics2d.limits",
    "physics2d.length",
    "physics2d.stiffness",
    "physics2d.damping",
    "physics2d.motor",
];
const NAV: &[&str] = &[
    "nav_agent2d.on",
    "nav_agent2d.target",
    "nav_agent2d.speed",
    "nav_agent2d.radius",
    "nav_agent2d.avoid",
];
const SHAPE2D: &[&str] = &["shape2d.kind", "shape2d.width", "shape2d.color", "shape2d.closed"];
const FOLLOW: &[&str] = &[
    "path_follow.path",
    "path_follow.speed",
    "path_follow.mode",
    "path_follow.rotate",
    "path_follow.start",
];
const MATERIAL: &[&str] = &[
    "material2d.effect",
    "material2d.color",
    "material2d.amount",
    "material2d.speed",
    "material2d.shader",
    "material2d.blend",
    "material2d.light_mode",
];

pub fn read_entity_field(entity: &Entity, path: &str) -> Option<PropValue> {
    Some(match path {
        "transform.translation" => vec3(entity.translation),
        "transform.translation.x" => num(entity.translation[0]),
        "transform.translation.y" => num(entity.translation[1]),
        "transform.translation.z" => num(entity.translation[2]),
        "transform.rotation" => angles3(entity.rotation),
        "transform.rotation.x" => angle(entity.rotation[0]),
        "transform.rotation.y" => angle(entity.rotation[1]),
        "transform.rotation.z" => angle(entity.rotation[2]),
        "transform.scale" => vec3(entity.scale),
        "transform.scale.x" => num(entity.scale[0]),
        "transform.scale.y" => num(entity.scale[1]),
        "transform.scale.z" => num(entity.scale[2]),
        "look.shape" => text(&entity.shape.map(|s| s.label().to_string()).unwrap_or_default()),
        "look.color" => color3(entity.color),
        "look.color.r" | "light.color.r" => num(entity.color[0]),
        "look.color.g" | "light.color.g" => num(entity.color[1]),
        "look.color.b" | "light.color.b" => num(entity.color[2]),
        "look.roughness" => num(entity.roughness),
        "look.metal" => num(entity.metal),
        "light.intensity" => num(entity.intensity),
        "light.color" => color3(entity.color),
        "camera.fov" => num(entity.fov),
        "camera.near" => num(entity.near),
        "camera.far" => num(entity.far),
        "body.dynamic" => flag(entity.dynamic),
        "body.friction" => num(entity.friction),
        "body.restitution" => num(entity.restitution),
        "body.walk_speed" => num(entity.walk_speed),
        "editor.hidden" => flag(entity.hidden),
        "editor.locked" => flag(entity.locked),
        "sprite_image.image" => text(&entity.sprite),
        "sprite_image.layer" => num(entity.layer as f32),
        "sprite_image.frames" => num(entity.frames as f32),
        "sprite_image.fps" => num(entity.fps),
        "sprite_image.animation" => text(&entity.animation),
        "sprite_image.flip_x" => flag(entity.flip[0]),
        "sprite_image.flip_y" => flag(entity.flip[1]),
        "sprite_image.tint" => color4(entity.tint),
        "sprite_image.tint.r" => num(entity.tint[0]),
        "sprite_image.tint.g" => num(entity.tint[1]),
        "sprite_image.tint.b" => num(entity.tint[2]),
        "sprite_image.tint.a" | "sprite_image.opacity" => num(entity.tint[3]),
        "sprite_image.order" => num(entity.order as f32),
        "sprite_image.slice" => vec4(entity.slice),
        "sprite_image.slice_mode" => text(&entity.slice_mode),
        "sprite_image.slice_size" => vec2(entity.slice_size),
        "sprite_image.pivot" => vec2(entity.pivot),
        "sprite_image.sort_mode" => text(entity.sort_mode.name()),
        "sprite_image.normal_map" => text(&entity.normal_map),
        "sprite_image.parallax" => vec2(entity.parallax),
        "sprite_image.repeat_x" => flag(entity.repeat_x),
        "camera2d.on" => flag(entity.camera2d.on),
        "camera2d.follow" => text(&entity.camera2d.follow),
        "camera2d.smoothing" => num(entity.camera2d.smoothing),
        "camera2d.zoom" => num(entity.camera2d.zoom),
        "camera2d.limits" => vec4(entity.camera2d.limits),
        "camera2d.pixel_snap" => num(entity.camera2d.pixel_snap),
        "light2d.kind" => text(&entity.light2d.kind),
        "light2d.range" => num(entity.light2d.range),
        "light2d.cone" => num(entity.light2d.cone),
        "light2d.shadows" => flag(entity.light2d.shadows),
        "light2d.blend" => text(&crate::light_fields::blend_name(&entity.light2d.blend)),
        "light2d.falloff" => num(entity.light2d.falloff),
        "light2d.points" => text(&crate::phys2d::points_text(&entity.light2d.points)),
        "physics2d.body" => text(&entity.phys2d.body),
        "physics2d.shape" => text(&entity.phys2d.shape),
        "physics2d.size" => vec2(entity.phys2d.size),
        "physics2d.layer" => num(entity.phys2d.layer as f32),
        "physics2d.mask" => num(entity.phys2d.mask as f32),
        "physics2d.one_way" => flag(entity.phys2d.one_way),
        "physics2d.surface" => text(&entity.phys2d.surface),
        "physics2d.ccd" => flag(entity.phys2d.ccd),
        "physics2d.joint" => text(&entity.phys2d.joint),
        "physics2d.joint_to" => text(&entity.phys2d.joint_to),
        "physics2d.anchor" => vec2(entity.phys2d.anchor),
        "physics2d.other_anchor" => vec2(entity.phys2d.other_anchor),
        "physics2d.axis" => vec2(entity.phys2d.axis),
        "physics2d.limits" => vec2(entity.phys2d.limits),
        "physics2d.length" => num(entity.phys2d.length),
        "physics2d.stiffness" => num(entity.phys2d.stiffness),
        "physics2d.damping" => num(entity.phys2d.damping),
        "physics2d.motor" => vec2(entity.phys2d.motor),
        "nav_agent2d.on" => flag(entity.nav2d.on),
        "nav_agent2d.target" => text(&entity.nav2d.target),
        "nav_agent2d.speed" => num(entity.nav2d.speed),
        "nav_agent2d.radius" => num(entity.nav2d.radius),
        "nav_agent2d.avoid" => flag(entity.nav2d.avoid),
        "shape2d.kind" => text(&entity.shape2d.kind),
        "shape2d.width" => num(entity.shape2d.width),
        "shape2d.color" => color4(entity.shape2d.color),
        "shape2d.closed" => flag(entity.shape2d.closed),
        "path_follow.path" => text(&entity.follow.path),
        "path_follow.speed" => num(entity.follow.speed),
        "path_follow.mode" => text(&entity.follow.mode),
        "path_follow.rotate" => flag(entity.follow.rotate),
        "path_follow.start" => num(entity.follow.start),
        "material2d.effect" => text(&entity.material2d.effect),
        "material2d.color" => color4(entity.material2d.color),
        "material2d.amount" => num(entity.material2d.amount),
        "material2d.speed" => num(entity.material2d.speed),
        "material2d.shader" => text(&entity.material2d.shader),
        "material2d.blend" => text(&entity.material2d.blend),
        "material2d.light_mode" => text(&entity.material2d.light_mode),
        _ => return None,
    })
}

pub fn write_entity_field(entity: &mut Entity, path: &str, value: &PropValue) -> bool {
    match path {
        "transform.translation" => {
            entity.translation = match arr3(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "transform.translation.x" => {
            entity.translation[0] = match num_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "transform.translation.y" => {
            entity.translation[1] = match num_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "transform.translation.z" => {
            entity.translation[2] = match num_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "transform.rotation" => {
            entity.rotation = match angles_of(value, 3) {
                Some(v) => v,
                None => return false,
            }
        }
        "transform.rotation.x" => {
            entity.rotation[0] = match num_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "transform.rotation.y" => {
            entity.rotation[1] = match num_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "transform.rotation.z" => {
            entity.rotation[2] = match num_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "transform.scale" => {
            entity.scale = match arr3(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "transform.scale.x" => {
            entity.scale[0] = match num_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "transform.scale.y" => {
            entity.scale[1] = match num_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "transform.scale.z" => {
            entity.scale[2] = match num_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "look.shape" => {
            let Some(text) = text_of(value) else { return false };
            entity.shape = crate::Shape::parse(&text);
        }
        "look.color" | "light.color" => {
            entity.color = match color_of(value, 3) {
                Some(v) => v,
                None => return false,
            }
        }
        "look.color.r" | "light.color.r" => {
            entity.color[0] = match unit(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "look.color.g" | "light.color.g" => {
            entity.color[1] = match unit(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "look.color.b" | "light.color.b" => {
            entity.color[2] = match unit(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "look.roughness" => {
            entity.roughness = match num_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "look.metal" => {
            entity.metal = match num_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "light.intensity" => {
            entity.intensity = match num_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "camera.fov" => {
            entity.fov = match num_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "camera.near" => {
            entity.near = match num_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "camera.far" => {
            entity.far = match num_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "body.dynamic" => {
            entity.dynamic = match flag_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "body.friction" => {
            entity.friction = match num_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "body.restitution" => {
            entity.restitution = match num_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "body.walk_speed" => {
            entity.walk_speed = match num_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "editor.hidden" => {
            entity.hidden = match flag_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "editor.locked" => {
            entity.locked = match flag_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "sprite_image.image" => {
            entity.sprite = match text_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "sprite_image.layer" => {
            entity.layer = match num_of(value) {
                Some(v) => v.round() as i32,
                None => return false,
            }
        }
        "sprite_image.frames" => {
            entity.frames = match num_of(value) {
                Some(v) => v.max(0.0).round() as u32,
                None => return false,
            }
        }
        "sprite_image.fps" => {
            entity.fps = match num_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "sprite_image.animation" => {
            entity.animation = match text_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "sprite_image.flip_x" => {
            entity.flip[0] = match flag_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "sprite_image.flip_y" => {
            entity.flip[1] = match flag_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "sprite_image.tint" => {
            entity.tint = match color4_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "sprite_image.tint.r" => {
            entity.tint[0] = match unit(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "sprite_image.tint.g" => {
            entity.tint[1] = match unit(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "sprite_image.tint.b" => {
            entity.tint[2] = match unit(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "sprite_image.tint.a" | "sprite_image.opacity" => {
            entity.tint[3] = match unit(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "sprite_image.order" => {
            entity.order = match num_of(value) {
                Some(v) => v.round() as i32,
                None => return false,
            }
        }
        "sprite_image.slice" => {
            entity.slice = match arr4(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "sprite_image.slice_mode" => {
            let text = match text_of(value) {
                Some(v) => v,
                None => return false,
            };
            if !crate::sprite_slice::MODES.contains(&text.as_str()) {
                return false;
            }
            entity.slice_mode = text;
        }
        "sprite_image.slice_size" => {
            entity.slice_size = match arr2(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "sprite_image.pivot" => {
            entity.pivot = match arr2(value) {
                Some(v) if v.iter().all(|x| (0.0..=1.0).contains(x)) => v,
                _ => return false,
            }
        }
        "sprite_image.sort_mode" => {
            let text = match text_of(value) {
                Some(v) => v,
                None => return false,
            };
            if !crate::sprite_sort::SORT_MODES.contains(&text.as_str()) {
                return false;
            }
            entity.sort_mode = crate::sprite_sort::ObjectSortMode::parse(&text);
        }
        "sprite_image.normal_map" => {
            entity.normal_map = match text_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "sprite_image.parallax" => {
            entity.parallax = match arr2(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "sprite_image.repeat_x" => {
            entity.repeat_x = match flag_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "camera2d.on" => {
            entity.camera2d.on = match flag_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "camera2d.follow" => {
            entity.camera2d.follow = match text_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "camera2d.smoothing" => {
            entity.camera2d.smoothing = match num_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "camera2d.zoom" => {
            entity.camera2d.zoom = match num_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "camera2d.limits" => {
            entity.camera2d.limits = match arr4(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "camera2d.pixel_snap" => {
            entity.camera2d.pixel_snap = match num_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "light2d.kind" => {
            entity.light2d.kind = match text_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "light2d.range" => {
            entity.light2d.range = match num_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "light2d.cone" => {
            entity.light2d.cone = match num_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "light2d.shadows" => {
            entity.light2d.shadows = match flag_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "light2d.blend" => {
            entity.light2d.blend = match text_of(value) {
                Some(v) => crate::light_fields::blend_name(&v),
                None => return false,
            }
        }
        "light2d.falloff" => {
            entity.light2d.falloff = match num_of(value) {
                Some(v) => v.clamp(0.0, 10000.0),
                None => return false,
            }
        }
        "light2d.points" => {
            entity.light2d.points = match text_of(value) {
                Some(v) => match crate::phys2d::parse_points(&v) {
                    Some(points) => points,
                    None => return false,
                },
                None => return false,
            }
        }
        "physics2d.body" => {
            entity.phys2d.body = match text_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "physics2d.shape" => {
            entity.phys2d.shape = match text_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "physics2d.size" => {
            entity.phys2d.size = match arr2(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "physics2d.layer" => {
            entity.phys2d.layer = match bits(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "physics2d.mask" => {
            entity.phys2d.mask = match bits(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "physics2d.one_way" => {
            entity.phys2d.one_way = match flag_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "physics2d.surface" => {
            entity.phys2d.surface = match text_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "physics2d.ccd" => {
            entity.phys2d.ccd = match flag_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "physics2d.joint" => {
            entity.phys2d.joint = match text_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "physics2d.joint_to" => {
            entity.phys2d.joint_to = match text_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "physics2d.anchor" => {
            entity.phys2d.anchor = match arr2(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "physics2d.other_anchor" => {
            entity.phys2d.other_anchor = match arr2(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "physics2d.axis" => {
            entity.phys2d.axis = match arr2(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "physics2d.limits" => {
            entity.phys2d.limits = match arr2(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "physics2d.length" => {
            entity.phys2d.length = match num_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "physics2d.stiffness" => {
            entity.phys2d.stiffness = match num_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "physics2d.damping" => {
            entity.phys2d.damping = match num_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "physics2d.motor" => {
            entity.phys2d.motor = match arr2(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "nav_agent2d.on" => {
            entity.nav2d.on = match flag_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "nav_agent2d.target" => {
            entity.nav2d.target = match text_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "nav_agent2d.speed" => {
            entity.nav2d.speed = match num_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "nav_agent2d.radius" => {
            entity.nav2d.radius = match num_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "nav_agent2d.avoid" => {
            entity.nav2d.avoid = match flag_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "shape2d.kind" => {
            entity.shape2d.kind = match text_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "shape2d.width" => {
            entity.shape2d.width = match num_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "shape2d.color" => {
            entity.shape2d.color = match color4_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "shape2d.closed" => {
            entity.shape2d.closed = match flag_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "path_follow.path" => {
            entity.follow.path = match text_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "path_follow.speed" => {
            entity.follow.speed = match num_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "path_follow.mode" => {
            entity.follow.mode = match text_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "path_follow.rotate" => {
            entity.follow.rotate = match flag_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "path_follow.start" => {
            entity.follow.start = match num_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "material2d.effect" => {
            entity.material2d.effect = match text_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "material2d.color" => {
            entity.material2d.color = match color4_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "material2d.amount" => {
            entity.material2d.amount = match num_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "material2d.speed" => {
            entity.material2d.speed = match num_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "material2d.shader" => {
            entity.material2d.shader = match text_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "material2d.blend" => {
            entity.material2d.blend = match text_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "material2d.light_mode" => {
            entity.material2d.light_mode = match text_of(value) {
                Some(v) => v,
                None => return false,
            }
        }
        _ => return false,
    }
    true
}

fn num(v: f32) -> PropValue {
    PropValue::Number(v)
}
fn angle(v: f32) -> PropValue {
    PropValue::Angle(v)
}
fn flag(v: bool) -> PropValue {
    PropValue::Bool(v)
}
fn text(v: &str) -> PropValue {
    PropValue::Text(v.to_string())
}
fn vec2(v: [f32; 2]) -> PropValue {
    PropValue::Vector(v.to_vec())
}
fn vec3(v: [f32; 3]) -> PropValue {
    PropValue::Vector(v.to_vec())
}
fn vec4(v: [f32; 4]) -> PropValue {
    PropValue::Vector(v.to_vec())
}
fn angles3(v: [f32; 3]) -> PropValue {
    PropValue::Angles(v.to_vec())
}
fn color3(v: [f32; 3]) -> PropValue {
    PropValue::Color(v.to_vec())
}
fn color4(v: [f32; 4]) -> PropValue {
    PropValue::Color(v.to_vec())
}

fn num_of(value: &PropValue) -> Option<f32> {
    match value {
        PropValue::Number(v) | PropValue::Angle(v) if v.is_finite() => Some(*v),
        _ => None,
    }
}
fn unit(value: &PropValue) -> Option<f32> {
    num_of(value).map(|v| v.clamp(0.0, 1.0))
}
fn flag_of(value: &PropValue) -> Option<bool> {
    match value {
        PropValue::Bool(v) => Some(*v),
        _ => None,
    }
}
fn text_of(value: &PropValue) -> Option<String> {
    match value {
        PropValue::Text(v) => Some(v.clone()),
        _ => None,
    }
}
fn arr(value: &PropValue) -> Option<&[f32]> {
    match value {
        PropValue::Vector(v) | PropValue::Angles(v) | PropValue::Color(v)
            if v.iter().all(|n| n.is_finite()) =>
        {
            Some(v)
        }
        _ => None,
    }
}
fn arr2(value: &PropValue) -> Option<[f32; 2]> {
    let v = arr(value)?;
    (v.len() == 2).then(|| [v[0], v[1]])
}
fn arr3(value: &PropValue) -> Option<[f32; 3]> {
    let v = arr(value)?;
    (v.len() == 3).then(|| [v[0], v[1], v[2]])
}
fn arr4(value: &PropValue) -> Option<[f32; 4]> {
    let v = arr(value)?;
    (v.len() == 4).then(|| [v[0], v[1], v[2], v[3]])
}
fn angles_of(value: &PropValue, n: usize) -> Option<[f32; 3]> {
    match value {
        PropValue::Angles(v) | PropValue::Vector(v) if v.len() == n && n == 3 => {
            Some([v[0], v[1], v[2]])
        }
        _ => None,
    }
}
fn color_of(value: &PropValue, n: usize) -> Option<[f32; 3]> {
    match value {
        PropValue::Color(v) if v.len() == n && v.iter().all(|c| c.is_finite()) => {
            Some([v[0].clamp(0.0, 1.0), v[1].clamp(0.0, 1.0), v[2].clamp(0.0, 1.0)])
        }
        _ => None,
    }
}
fn color4_of(value: &PropValue) -> Option<[f32; 4]> {
    match value {
        PropValue::Color(v) if v.len() == 4 && v.iter().all(|c| c.is_finite()) => Some([
            v[0].clamp(0.0, 1.0),
            v[1].clamp(0.0, 1.0),
            v[2].clamp(0.0, 1.0),
            v[3].clamp(0.0, 1.0),
        ]),
        PropValue::Color(v) if v.len() == 3 && v.iter().all(|c| c.is_finite()) => {
            Some([v[0].clamp(0.0, 1.0), v[1].clamp(0.0, 1.0), v[2].clamp(0.0, 1.0), 1.0])
        }
        _ => None,
    }
}
fn bits(value: &PropValue) -> Option<u32> {
    let n = num_of(value)?;
    (n >= 0.0).then(|| n.round() as u32)
}
