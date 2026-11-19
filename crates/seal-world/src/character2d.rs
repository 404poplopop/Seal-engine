//! Kinematic 2D character: slides along solid colliders instead of using a dynamic body.
//!
//! The inspector, the scene file and copy-paste share [`fields`], [`check`] and
//! [`apply`]. During play, [`Motion::step`] moves every object whose Character 2D
//! is on. Objects without it are left alone, including the side-view player.

use std::collections::BTreeMap;

use seal_scene::Character2d;

use crate::{format_num, phys2d, Entity};

/// Movement modes the inspector and the Set Movement Mode node accept.
pub const MODES: &[&str] = &["walk", "fly", "swim", "climb"];

/// Inspector fields, in order. The rest stay hidden until the component is on.
pub const KEYS: &[&str] = &[
    "char_on",
    "char_mode",
    "char_speed",
    "char_fly_speed",
    "char_swim_speed",
    "char_climb_speed",
    "char_jump_speed",
    "char_gravity",
    "char_floor_angle",
    "char_step",
    "char_snap",
    "char_coyote",
    "char_buffer",
    "char_size",
    "char_crouch_height",
    "char_crouch",
];

const STEP: f32 = 1.0 / 60.0;
const SKIN: f32 = 0.002;
const MAX_FALL: f32 = 40.0;

fn yes(on: bool) -> String {
    if on { "yes" } else { "no" }.into()
}

fn finite(text: &str) -> Option<f32> {
    text.trim().parse::<f32>().ok().filter(|n| n.is_finite())
}

/// The fields the inspector shows.
pub fn fields(c: &Character2d) -> Vec<(String, String)> {
    let mut out = vec![("char_on".into(), yes(c.on))];
    if !c.on {
        return out;
    }
    let mut push = |k: &str, v: String| out.push((k.into(), v));
    push("char_mode", c.mode.clone());
    push("char_speed", format_num(c.speed));
    push("char_fly_speed", format_num(c.fly_speed));
    push("char_swim_speed", format_num(c.swim_speed));
    push("char_climb_speed", format_num(c.climb_speed));
    push("char_jump_speed", format_num(c.jump_speed));
    push("char_gravity", format_num(c.gravity));
    push("char_floor_angle", format_num(c.max_floor_angle));
    push("char_step", format_num(c.step_height));
    push("char_snap", format_num(c.floor_snap));
    push("char_coyote", format_num(c.coyote_time));
    push("char_buffer", format_num(c.jump_buffer));
    push("char_size", phys2d::pair_text(&c.size));
    push("char_crouch_height", format_num(c.crouch_height));
    push("char_crouch", yes(c.crouch));
    out
}

/// The text form of one field, whether or not [`fields`] would list it.
pub fn field_text(c: &Character2d, key: &str) -> Option<String> {
    let shown = fields(&Character2d { on: true, ..c.clone() });
    let value = shown.into_iter().find(|(k, _)| k == key).map(|(_, v)| v)?;
    if key == "char_on" {
        Some(yes(c.on))
    } else {
        Some(value)
    }
}

/// Why `value` is refused for `key`, or `None` when it is fine.
pub fn check(key: &str, value: &str) -> Option<&'static str> {
    let v = value.trim();
    let range = |lo: f32, hi: f32| finite(v).is_some_and(|n| (lo..=hi).contains(&n));
    let ok = match key {
        "char_on" | "char_crouch" => matches!(v, "yes" | "no"),
        "char_mode" => MODES.contains(&v),
        "char_speed" | "char_fly_speed" | "char_swim_speed" | "char_climb_speed" => {
            range(0.0, 100.0)
        }
        "char_jump_speed" => range(0.0, 100.0),
        "char_gravity" => range(0.0, 200.0),
        "char_floor_angle" => range(0.0, 89.0),
        "char_step" | "char_snap" => range(0.0, 5.0),
        "char_coyote" | "char_buffer" => range(0.0, 2.0),
        "char_size" => phys2d::parse_pair(v)
            .is_some_and(|p| (0.05..=10.0).contains(&p[0]) && (0.05..=10.0).contains(&p[1])),
        "char_crouch_height" => range(0.05, 10.0),
        _ => false,
    };
    (!ok).then_some("inspector-value-out-of-range")
}

/// Set one field from its text. Returns false when the key is unknown or the text is refused.
pub fn apply(c: &mut Character2d, key: &str, value: &str) -> bool {
    if check(key, value).is_some() {
        return false;
    }
    let v = value.trim();
    let num = |text: &str| text.parse::<f32>().unwrap_or(0.0);
    match key {
        "char_on" => c.on = v == "yes",
        "char_mode" => c.mode = v.into(),
        "char_speed" => c.speed = num(v),
        "char_fly_speed" => c.fly_speed = num(v),
        "char_swim_speed" => c.swim_speed = num(v),
        "char_climb_speed" => c.climb_speed = num(v),
        "char_jump_speed" => c.jump_speed = num(v),
        "char_gravity" => c.gravity = num(v),
        "char_floor_angle" => c.max_floor_angle = num(v),
        "char_step" => c.step_height = num(v),
        "char_snap" => c.floor_snap = num(v),
        "char_coyote" => c.coyote_time = num(v),
        "char_buffer" => c.jump_buffer = num(v),
        "char_size" => {
            c.size = phys2d::parse_pair(v).unwrap_or(c.size);
            if c.crouch_height > c.size[1] {
                c.crouch_height = c.size[1];
            }
        }
        "char_crouch_height" => c.crouch_height = num(v).min(c.size[1].max(0.05)),
        "char_crouch" => c.crouch = v == "yes",
        _ => return false,
    }
    true
}

/// Fill in a freshly added Character 2D object.
pub fn configure(entity: &mut Entity) {
    entity.character2d = Character2d { on: true, ..Character2d::default() };
    entity.color = [0.35, 0.72, 0.55];
    entity.dynamic = false;
    entity.shape = None;
    entity.translation[1] = entity.character2d.size[1] * 0.5;
}

/// What a graph or the play step can ask of one character before the next step.
#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    Move { id: String, wish: [f32; 2] },
    Jump { id: String },
}

/// Floor, wall, ceiling and velocity after the last step.
#[derive(Clone, Debug, PartialEq)]
pub struct View {
    pub on_floor: bool,
    pub on_wall: bool,
    pub on_ceiling: bool,
    pub velocity: [f32; 2],
    pub mode: String,
    pub crouch_blocked: bool,
    pub crouched: bool,
}

#[derive(Clone, Debug)]
struct BodyState {
    wish: [f32; 2],
    jump_edge: bool,
    vel: [f32; 2],
    on_floor: bool,
    on_wall: bool,
    on_ceiling: bool,
    floor_id: Option<String>,
    floor_normal: [f32; 2],
    platform_vel: [f32; 2],
    coyote: f32,
    buffer: f32,
    crouched: bool,
    crouch_blocked: bool,
    mode: String,
}

impl Default for BodyState {
    fn default() -> Self {
        Self {
            wish: [0.0, 0.0],
            jump_edge: false,
            vel: [0.0, 0.0],
            on_floor: false,
            on_wall: false,
            on_ceiling: false,
            floor_id: None,
            floor_normal: [0.0, 1.0],
            platform_vel: [0.0, 0.0],
            coyote: 0.0,
            buffer: 0.0,
            crouched: false,
            crouch_blocked: false,
            mode: "walk".into(),
        }
    }
}

impl BodyState {
    fn view(&self) -> View {
        View {
            on_floor: self.on_floor,
            on_wall: self.on_wall,
            on_ceiling: self.on_ceiling,
            velocity: self.vel,
            mode: self.mode.clone(),
            crouch_blocked: self.crouch_blocked,
            crouched: self.crouched,
        }
    }
}

/// Play state for every Character 2D. Not saved with the scene.
#[derive(Clone, Debug, Default)]
pub struct Motion {
    states: BTreeMap<String, BodyState>,
    prev: BTreeMap<String, [f32; 2]>,
    pending: Vec<Cmd>,
    carry: f32,
}

impl Motion {
    pub fn command(&mut self, cmd: Cmd) {
        self.pending.push(cmd);
    }

    pub fn view(&self, id: &str) -> Option<View> {
        self.states.get(id).map(BodyState::view)
    }

    pub fn views(&self) -> BTreeMap<String, View> {
        self.states.iter().map(|(id, s)| (id.clone(), s.view())).collect()
    }

    /// Advance every turned-on character. Objects without the component are not written.
    pub fn step(&mut self, dt: f32, entities: &mut [Entity]) {
        self.states.retain(|id, _| entities.iter().any(|e| e.id == *id && e.character2d.on));
        let pending = std::mem::take(&mut self.pending);
        for cmd in pending {
            let id = match &cmd {
                Cmd::Move { id, .. } | Cmd::Jump { id } => id.clone(),
            };
            if !entities.iter().any(|e| e.id == id && e.character2d.on) {
                continue;
            }
            let state = self.states.entry(id).or_default();
            match cmd {
                Cmd::Move { wish, .. } => state.wish = wish,
                Cmd::Jump { .. } => state.jump_edge = true,
            }
        }
        if dt <= 0.0 || entities.iter().all(|e| !e.character2d.on) {
            return;
        }
        self.carry += dt;
        let mut stepped = false;
        let mut n = 0;
        while self.carry + 1e-5 >= STEP && n < 8 {
            self.carry -= STEP;
            if self.carry < 1e-5 {
                self.carry = 0.0;
            }
            self.substep(STEP, entities);
            stepped = true;
            n += 1;
        }
        if stepped {
            for state in self.states.values_mut() {
                state.wish = [0.0, 0.0];
                state.jump_edge = false;
            }
        }
    }

    fn substep(&mut self, dt: f32, entities: &mut [Entity]) {
        let solids = collect_solids(entities, &self.prev);
        let ids: Vec<String> = entities
            .iter()
            .filter(|e| e.character2d.on && !e.hidden)
            .map(|e| e.id.clone())
            .collect();
        for id in ids {
            let Some(index) = entities.iter().position(|e| e.id == id) else { continue };
            let spec = entities[index].character2d.clone();
            let scale = [entities[index].scale[0].abs(), entities[index].scale[1].abs()];
            let mut pos = [entities[index].translation[0], entities[index].translation[1]];
            let state = self.states.entry(id.clone()).or_default();
            let mine = solids.iter().filter(|s| s.id != id).collect::<Vec<_>>();
            pos = integrate(state, &spec, scale, pos, dt, &mine);
            entities[index].translation[0] = pos[0];
            entities[index].translation[1] = pos[1];
            entities[index].velocity[0] = state.vel[0];
            entities[index].velocity[1] = state.vel[1];
        }
        for solid in &solids {
            self.prev.insert(solid.id.clone(), solid.center);
        }
    }
}

fn integrate(
    state: &mut BodyState,
    spec: &Character2d,
    scale: [f32; 2],
    mut pos: [f32; 2],
    dt: f32,
    solids: &[&Solid],
) -> [f32; 2] {
    let stand_h = (spec.size[1] * scale[1]).max(0.05);
    let crouch_h = (spec.crouch_height * scale[1]).clamp(0.05, stand_h);
    let width = (spec.size[0] * scale[0]).max(0.05);
    let old_h = if state.crouched { crouch_h } else { stand_h };
    let feet = pos[1] - old_h * 0.5;
    if spec.crouch && !state.crouched {
        state.crouched = true;
        state.crouch_blocked = false;
        pos[1] = feet + crouch_h * 0.5;
    } else if !spec.crouch && state.crouched {
        let stand_pos = [pos[0], feet + stand_h * 0.5];
        let stand_half = [width * 0.5, stand_h * 0.5];
        let blocked = solids.iter().any(|s| blocked_by(stand_pos, stand_half, s));
        if blocked {
            state.crouch_blocked = true;
            pos[1] = feet + crouch_h * 0.5;
        } else {
            state.crouched = false;
            state.crouch_blocked = false;
            pos[1] = stand_pos[1];
        }
    } else {
        state.crouch_blocked = false;
    }
    let height = if state.crouched { crouch_h } else { stand_h };
    let half = [width * 0.5, height * 0.5];
    let water = in_area(pos, half, solids, AreaKind::Water);
    let climb = in_area(pos, half, solids, AreaKind::Climb);
    let mode = effective_mode(&spec.mode, water, climb);
    state.mode = mode.into();
    let floor_cos = spec.max_floor_angle.to_radians().cos();

    if let Some(fid) = state.floor_id.clone() {
        if let Some(solid) = solids.iter().find(|s| s.id == fid) {
            pos[0] += solid.delta[0];
            pos[1] += solid.delta[1];
            state.platform_vel =
                if dt > 1e-6 { [solid.delta[0] / dt, solid.delta[1] / dt] } else { [0.0, 0.0] };
        }
    } else {
        state.platform_vel = [0.0, 0.0];
    }
    pos = separate(pos, half, solids);

    if state.on_floor {
        state.coyote = spec.coyote_time;
    } else {
        state.coyote = (state.coyote - dt).max(0.0);
    }
    if state.jump_edge {
        state.buffer = spec.jump_buffer;
        state.jump_edge = false;
    } else {
        state.buffer = (state.buffer - dt).max(0.0);
    }

    let wish = state.wish;
    match mode {
        "fly" => state.vel = [wish[0] * spec.fly_speed, wish[1] * spec.fly_speed],
        "swim" => {
            let sink = if wish[1].abs() < 1e-4 { -spec.gravity * 0.04 } else { 0.0 };
            state.vel = [wish[0] * spec.swim_speed, wish[1] * spec.swim_speed + sink];
        }
        "climb" => state.vel = [wish[0] * spec.climb_speed, wish[1] * spec.climb_speed],
        _ => {
            if state.on_floor {
                state.vel = [wish[0] * spec.speed, 0.0];
            } else {
                let target = wish[0] * spec.speed;
                state.vel[0] += (target - state.vel[0]) * 0.25;
                state.vel[1] = (state.vel[1] - spec.gravity * dt).max(-MAX_FALL);
            }
        }
    }
    let mut jumped = false;
    let can_jump = state.on_floor || state.coyote > 0.0;
    if state.buffer > 0.0 && can_jump && matches!(mode, "walk" | "swim") {
        let boost = if mode == "swim" { spec.jump_speed * 0.45 } else { spec.jump_speed };
        state.vel[1] = boost + state.platform_vel[1];
        if mode == "walk" {
            state.vel[0] += state.platform_vel[0];
        }
        state.on_floor = false;
        state.coyote = 0.0;
        state.buffer = 0.0;
        state.floor_id = None;
        jumped = true;
    }

    let was_floor = state.on_floor && !jumped;
    let allow_step = mode == "walk" && was_floor;
    let (mut next, mut contacts) =
        move_x(pos, state.vel[0] * dt, half, solids, spec.step_height, allow_step, floor_cos);
    let (vert, vcontacts) = slide(next, [0.0, state.vel[1] * dt], half, solids, floor_cos);
    next = vert;
    contacts.extend(vcontacts);
    if mode == "walk" && was_floor && state.vel[1] <= 0.05 && spec.floor_snap > 0.0 {
        if let Some((snapped, scontacts)) =
            drop_onto_floor(next, spec.floor_snap, half, solids, floor_cos)
        {
            next = snapped;
            contacts.extend(scontacts);
        }
    }

    state.on_floor = false;
    state.on_wall = false;
    state.on_ceiling = false;
    state.floor_id = None;
    let mut wall_n = [0.0, 0.0];
    for hit in &contacts {
        if hit.normal[1] >= floor_cos {
            state.on_floor = true;
            state.floor_normal = hit.normal;
            state.floor_id = Some(hit.id.clone());
        } else if hit.normal[1] <= -floor_cos {
            state.on_ceiling = true;
        } else {
            state.on_wall = true;
            wall_n = hit.normal;
        }
    }
    if state.on_floor && state.vel[1] < 0.0 {
        state.vel[1] = 0.0;
    }
    if state.on_ceiling && state.vel[1] > 0.0 {
        state.vel[1] = 0.0;
    }
    if state.on_wall {
        let into = state.vel[0] * wall_n[0] + state.vel[1] * wall_n[1];
        if into < 0.0 {
            state.vel[0] -= wall_n[0] * into;
            state.vel[1] -= wall_n[1] * into;
        }
    }
    next
}

fn effective_mode(base: &str, water: bool, climb: bool) -> &'static str {
    match base {
        "fly" => "fly",
        "swim" => {
            if climb && !water {
                "climb"
            } else {
                "swim"
            }
        }
        "climb" => {
            if water && !climb {
                "swim"
            } else {
                "climb"
            }
        }
        _ => {
            if climb {
                "climb"
            } else if water {
                "swim"
            } else {
                "walk"
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AreaKind {
    Water,
    Climb,
}

fn in_area(pos: [f32; 2], half: [f32; 2], solids: &[&Solid], kind: AreaKind) -> bool {
    solids
        .iter()
        .filter(|s| s.area == Some(kind))
        .any(|s| overlap_aabb(pos, half, s.center, s.area_half))
}

fn overlap_aabb(a: [f32; 2], ah: [f32; 2], b: [f32; 2], bh: [f32; 2]) -> bool {
    (a[0] - b[0]).abs() <= ah[0] + bh[0] && (a[1] - b[1]).abs() <= ah[1] + bh[1]
}

struct Contact {
    normal: [f32; 2],
    id: String,
}

struct Solid {
    id: String,
    center: [f32; 2],
    delta: [f32; 2],
    shape: Shape,
    /// Real collider. Area volumes that only switch movement mode do not block.
    blocks: bool,
    one_way: bool,
    top: f32,
    area: Option<AreaKind>,
    area_half: [f32; 2],
}

enum Shape {
    Aabb { half: [f32; 2] },
    Poly { pts: Vec<[f32; 2]> },
}

fn collect_solids(entities: &[Entity], prev: &BTreeMap<String, [f32; 2]>) -> Vec<Solid> {
    let mut out = Vec::new();
    for e in entities {
        if e.hidden || e.character2d.on {
            continue;
        }
        let center = [e.translation[0], e.translation[1]];
        let delta =
            prev.get(&e.id).map(|p| [center[0] - p[0], center[1] - p[1]]).unwrap_or([0.0, 0.0]);
        let (area, area_half) = area_of(e);
        if let Some(shape) = solid_shape(e) {
            let top = shape_top(&shape, center);
            out.push(Solid {
                id: e.id.clone(),
                center,
                delta,
                shape,
                blocks: true,
                one_way: e.phys2d.one_way,
                top,
                area,
                area_half,
            });
        } else if area.is_some() {
            out.push(Solid {
                id: e.id.clone(),
                center,
                delta,
                shape: Shape::Aabb { half: area_half },
                blocks: false,
                one_way: false,
                top: center[1] + area_half[1],
                area,
                area_half,
            });
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

fn area_of(e: &Entity) -> (Option<AreaKind>, [f32; 2]) {
    let kind = if e.area.mode == "buoyancy" || e.tag == "water" {
        Some(AreaKind::Water)
    } else if e.tag == "climb" {
        Some(AreaKind::Climb)
    } else {
        None
    };
    let half = volume_half(e);
    (kind, half)
}

fn volume_half(e: &Entity) -> [f32; 2] {
    let (w, h) = if !e.phys2d.body.is_empty() {
        let sx = e.scale[0].abs();
        let sy = e.scale[1].abs();
        if e.phys2d.shape == "circle" {
            let d = e.phys2d.size[0] * sx;
            (d, d)
        } else {
            (e.phys2d.size[0] * sx, e.phys2d.size[1] * sy)
        }
    } else {
        (e.scale[0].abs(), e.scale[1].abs())
    };
    [w.max(0.0) * 0.5, h.max(0.0) * 0.5]
}

fn solid_shape(e: &Entity) -> Option<Shape> {
    if !e.area.mode.is_empty() {
        return None;
    }
    let sx = e.scale[0].abs();
    let sy = e.scale[1].abs();
    if !e.phys2d.body.is_empty() {
        let full = [e.phys2d.size[0] * sx, e.phys2d.size[1] * sy];
        if full[0] < 1e-4 || full[1] < 1e-4 {
            return None;
        }
        let half = [full[0] * 0.5, full[1] * 0.5];
        let rot = e.rotation[2];
        return Some(match e.phys2d.shape.as_str() {
            "polygon" => {
                let pts = world_points(e, &e.phys2d.points);
                let pts = ensure_ccw(pts)?;
                Shape::Poly { pts }
            }
            "circle" => {
                let r = half[0];
                let pts = (0..12)
                    .map(|i| {
                        let a = i as f32 / 12.0 * std::f32::consts::TAU;
                        [e.translation[0] + a.cos() * r, e.translation[1] + a.sin() * r]
                    })
                    .collect();
                Shape::Poly { pts }
            }
            "edge" => return None,
            _ if rot.abs() > 0.05 => {
                let local = [e.phys2d.size[0] * 0.5, e.phys2d.size[1] * 0.5];
                let corners = [
                    [-local[0], -local[1]],
                    [local[0], -local[1]],
                    [local[0], local[1]],
                    [-local[0], local[1]],
                ];
                let pts = ensure_ccw(world_points(e, &corners))?;
                Shape::Poly { pts }
            }
            _ => Shape::Aabb { half },
        });
    }
    if e.tag == "ground" {
        let half = [sx * 0.5, sy * 0.5];
        if half[0] < 1e-4 || half[1] < 1e-4 {
            return None;
        }
        return Some(Shape::Aabb { half });
    }
    None
}

fn world_points(e: &Entity, local: &[[f32; 2]]) -> Vec<[f32; 2]> {
    let rz = e.rotation[2].to_radians();
    let (c, s) = (rz.cos(), rz.sin());
    let sx = e.scale[0].abs();
    let sy = e.scale[1].abs();
    local
        .iter()
        .map(|p| {
            let x = p[0] * sx;
            let y = p[1] * sy;
            [e.translation[0] + x * c - y * s, e.translation[1] + x * s + y * c]
        })
        .collect()
}

fn ensure_ccw(mut pts: Vec<[f32; 2]>) -> Option<Vec<[f32; 2]>> {
    if pts.len() < 3 {
        return None;
    }
    let mut area = 0.0;
    for i in 0..pts.len() {
        let a = pts[i];
        let b = pts[(i + 1) % pts.len()];
        area += a[0] * b[1] - b[0] * a[1];
    }
    if area.abs() < 1e-6 {
        return None;
    }
    if area < 0.0 {
        pts.reverse();
    }
    Some(pts)
}

fn shape_top(shape: &Shape, center: [f32; 2]) -> f32 {
    match shape {
        Shape::Aabb { half } => center[1] + half[1],
        Shape::Poly { pts } => pts.iter().map(|p| p[1]).fold(f32::MIN, f32::max),
    }
}

fn separate(mut pos: [f32; 2], half: [f32; 2], solids: &[&Solid]) -> [f32; 2] {
    for _ in 0..4 {
        let mut pushed = false;
        for solid in solids {
            if !solid.blocks {
                continue;
            }
            if solid.one_way && pos[1] - half[1] < solid.top - SKIN {
                continue;
            }
            if let Some(push) = push_out(pos, half, solid) {
                pos[0] += push[0];
                pos[1] += push[1];
                pushed = true;
            }
        }
        if !pushed {
            break;
        }
    }
    pos
}

fn blocked_by(pos: [f32; 2], half: [f32; 2], solid: &Solid) -> bool {
    solid.blocks && !solid.one_way && push_out(pos, half, solid).is_some()
}

fn push_out(center: [f32; 2], half: [f32; 2], solid: &Solid) -> Option<[f32; 2]> {
    let axes = axes_of(solid);
    let mut best = f32::MAX;
    let mut push = [0.0, 0.0];
    for axis in axes {
        let (c0, c1) = project_aabb(center, half, axis);
        let (o0, o1) = project_solid(solid, axis);
        let overlap = c1.min(o1) - c0.max(o0);
        if overlap <= SKIN {
            return None;
        }
        if overlap < best {
            best = overlap;
            let sign = if (c0 + c1) < (o0 + o1) { -1.0 } else { 1.0 };
            push = [axis[0] * sign * (overlap + SKIN), axis[1] * sign * (overlap + SKIN)];
        }
    }
    if best.is_finite() && push[0].is_finite() && push[1].is_finite() {
        Some(push)
    } else {
        None
    }
}

/// Drop straight down onto a walkable surface and stop. The leftover is not
/// slid along the slope, so a body standing still does not creep downhill.
fn drop_onto_floor(
    pos: [f32; 2],
    dist: f32,
    half: [f32; 2],
    solids: &[&Solid],
    floor_cos: f32,
) -> Option<([f32; 2], Vec<Contact>)> {
    let delta = [0.0, -dist];
    let mut best: Option<(f32, [f32; 2], usize)> = None;
    for (i, solid) in solids.iter().enumerate() {
        if !solid.blocks {
            continue;
        }
        let Some(hit) = sweep(pos, delta, half, solid) else { continue };
        if hit.normal[1] < floor_cos {
            continue;
        }
        if solid.one_way && pos[1] - half[1] < solid.top - 0.02 {
            continue;
        }
        let take = match best {
            None => true,
            Some((toi, _, j)) => {
                hit.toi < toi - 1e-5 || ((hit.toi - toi).abs() <= 1e-5 && solid.id < solids[j].id)
            }
        };
        if take {
            best = Some((hit.toi, hit.normal, i));
        }
    }
    let (toi, normal, i) = best?;
    let travel = (toi * dist - SKIN).max(0.0);
    let next = [pos[0], pos[1] - travel];
    Some((next, vec![Contact { normal, id: solids[i].id.clone() }]))
}

fn move_x(
    pos: [f32; 2],
    dx: f32,
    half: [f32; 2],
    solids: &[&Solid],
    step: f32,
    allow_step: bool,
    floor_cos: f32,
) -> ([f32; 2], Vec<Contact>) {
    let (direct, contacts) = slide(pos, [dx, 0.0], half, solids, floor_cos);
    if !allow_step || step <= 0.01 || dx.abs() < 1e-6 {
        return (direct, contacts);
    }
    let gained = (direct[0] - pos[0]).abs();
    let hit_wall = contacts.iter().any(|c| c.normal[1] < floor_cos);
    if !hit_wall || gained > dx.abs() * 0.6 {
        return (direct, contacts);
    }
    let (up, _) = slide(pos, [0.0, step], half, solids, floor_cos);
    if up[1] - pos[1] < 0.01 {
        return (direct, contacts);
    }
    let (across, _) = slide(up, [dx, 0.0], half, solids, floor_cos);
    if (across[0] - pos[0]).abs() <= gained + 0.001 {
        return (direct, contacts);
    }
    let drop = up[1] - pos[1] + SKIN;
    let (down, landed) = slide(across, [0.0, -drop], half, solids, floor_cos);
    if landed.iter().any(|c| c.normal[1] >= floor_cos) && down[1] <= pos[1] + step + 0.05 {
        return (down, landed);
    }
    (direct, contacts)
}

fn slide(
    mut pos: [f32; 2],
    mut delta: [f32; 2],
    half: [f32; 2],
    solids: &[&Solid],
    floor_cos: f32,
) -> ([f32; 2], Vec<Contact>) {
    let mut contacts = Vec::new();
    for _ in 0..4 {
        pos = separate(pos, half, solids);
        let len = (delta[0] * delta[0] + delta[1] * delta[1]).sqrt();
        if len < 1e-6 {
            break;
        }
        let mut best: Option<(f32, [f32; 2], usize)> = None;
        for (i, solid) in solids.iter().enumerate() {
            if !solid.blocks {
                continue;
            }
            let Some(hit) = sweep(pos, delta, half, solid) else { continue };
            if solid.one_way && (hit.normal[1] < 0.5 || pos[1] - half[1] < solid.top - 0.02) {
                continue;
            }
            let take = match best {
                None => true,
                Some((toi, _, j)) => {
                    hit.toi < toi - 1e-5
                        || ((hit.toi - toi).abs() <= 1e-5 && solid.id < solids[j].id)
                }
            };
            if take {
                best = Some((hit.toi, hit.normal, i));
            }
        }
        let Some((toi, normal, i)) = best else {
            pos[0] += delta[0];
            pos[1] += delta[1];
            break;
        };
        let travel = (toi * len - SKIN).max(0.0);
        pos[0] += delta[0] / len * travel;
        pos[1] += delta[1] / len * travel;
        contacts.push(Contact { normal, id: solids[i].id.clone() });
        let remain = (1.0 - toi).max(0.0);
        let mut left = [delta[0] * remain, delta[1] * remain];
        let dot = left[0] * normal[0] + left[1] * normal[1];
        if dot < 0.0 {
            left[0] -= normal[0] * dot;
            left[1] -= normal[1] * dot;
        }
        if normal[1] < floor_cos && left[1] > 0.0 {
            left[1] = 0.0;
        }
        if left[0] * delta[0] + left[1] * delta[1] <= 0.0 {
            break;
        }
        delta = left;
    }
    (pos, contacts)
}

struct Hit {
    toi: f32,
    normal: [f32; 2],
}

fn sweep(from: [f32; 2], delta: [f32; 2], half: [f32; 2], solid: &Solid) -> Option<Hit> {
    let axes = axes_of(solid);
    let mut t_enter = f32::NEG_INFINITY;
    let mut t_exit = 1.0f32;
    let mut normal = [0.0f32, 1.0];
    for axis in axes {
        let (c0, c1) = project_aabb(from, half, axis);
        let (o0, o1) = project_solid(solid, axis);
        let v = delta[0] * axis[0] + delta[1] * axis[1];
        if v.abs() < 1e-8 {
            if c1 < o0 - 1e-5 || c0 > o1 + 1e-5 {
                return None;
            }
            continue;
        }
        let (enter, exit, nsign) = if v > 0.0 {
            ((o0 - c1) / v, (o1 - c0) / v, -1.0)
        } else {
            ((o1 - c0) / v, (o0 - c1) / v, 1.0)
        };
        let n = [axis[0] * nsign, axis[1] * nsign];
        if enter > t_enter + 1e-5
            || ((enter - t_enter).abs() <= 1e-5 && n[1].abs() > normal[1].abs())
        {
            t_enter = enter;
            normal = n;
        }
        if exit < t_exit {
            t_exit = exit;
        }
        if t_enter > t_exit + 1e-4 {
            return None;
        }
    }
    if t_enter < -1e-4 || t_enter > 1.0 || t_enter > t_exit + 1e-4 {
        return None;
    }
    let len = (normal[0] * normal[0] + normal[1] * normal[1]).sqrt();
    if len < 1e-6 {
        return None;
    }
    Some(Hit { toi: t_enter.max(0.0), normal: [normal[0] / len, normal[1] / len] })
}

fn axes_of(solid: &Solid) -> Vec<[f32; 2]> {
    let mut axes = vec![[1.0, 0.0], [0.0, 1.0]];
    if let Shape::Poly { pts } = &solid.shape {
        for i in 0..pts.len() {
            let a = pts[i];
            let b = pts[(i + 1) % pts.len()];
            let edge = [b[0] - a[0], b[1] - a[1]];
            let n = [edge[1], -edge[0]];
            let len = (n[0] * n[0] + n[1] * n[1]).sqrt();
            if len < 1e-6 {
                continue;
            }
            axes.push([n[0] / len, n[1] / len]);
        }
    }
    axes
}

fn project_aabb(center: [f32; 2], half: [f32; 2], axis: [f32; 2]) -> (f32, f32) {
    let mid = center[0] * axis[0] + center[1] * axis[1];
    let r = half[0] * axis[0].abs() + half[1] * axis[1].abs();
    (mid - r, mid + r)
}

fn project_solid(solid: &Solid, axis: [f32; 2]) -> (f32, f32) {
    match &solid.shape {
        Shape::Aabb { half } => project_aabb(solid.center, *half, axis),
        Shape::Poly { pts } => {
            let mut lo = f32::MAX;
            let mut hi = f32::MIN;
            for p in pts {
                let d = p[0] * axis[0] + p[1] * axis[1];
                lo = lo.min(d);
                hi = hi.max(d);
            }
            (lo, hi)
        }
    }
}
