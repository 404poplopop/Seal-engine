//! Apply a sampled property track to the live scene.
//!
//! A path that is not a component field, a running script variable, or a
//! shown UI control is skipped. The caller records one warning per path.

use seal_graph::{Ty, Value, V2, V3};
use seal_scene::sequence::{PropValue, Sequence};

use crate::gameui::{Node, Screens};
use crate::prop_fields::{read_entity_field, write_entity_field};
use crate::World;

impl World {
    pub(crate) fn apply_sequence_properties(&mut self, seq: &Sequence, t: f32) {
        let samples: Vec<(String, String, PropValue)> = seq
            .properties
            .iter()
            .filter_map(|track| {
                track
                    .sample(seq.warp_time(t))
                    .map(|value| (track.entity.clone(), track.path.clone(), value))
            })
            .collect();
        for (entity, path, value) in samples {
            if !self.apply_property(&entity, &path, &value) {
                self.warn_missing_property(&entity, &path);
            }
        }
    }

    fn warn_missing_property(&mut self, entity: &str, path: &str) {
        if !self.sequence_warned.insert(path.to_string()) {
            return;
        }
        self.play_log.push(format!("warning: {path} is not a field on {entity}"));
    }

    fn apply_property(&mut self, entity: &str, path: &str, value: &PropValue) -> bool {
        if let Some(name) = path.strip_prefix("script.") {
            return self.apply_script_var(entity, name, value);
        }
        if let Some(rest) = path.strip_prefix("ui.") {
            let Some((node, prop)) = rest.rsplit_once('.') else { return false };
            return apply_ui(&mut self.game_screens, node, prop, value);
        }
        let Some(index) = self.entities.iter().position(|e| e.id == entity || e.name == entity)
        else {
            return false;
        };
        write_entity_field(&mut self.entities[index], path, value)
    }

    fn apply_script_var(&mut self, entity: &str, name: &str, value: &PropValue) -> bool {
        if name.is_empty() {
            return false;
        }
        let Some(scripts) = self.scripts.as_mut() else { return false };
        let Some(current) = scripts.named_var(&self.entities, entity, name) else { return false };
        let Some(next) = to_graph(value, current.ty()) else { return false };
        scripts.set_named_var(&self.entities, entity, name, next)
    }

    /// Current value of a property path, for the Timeline key button.
    pub fn read_property(&self, entity: &str, path: &str) -> Option<PropValue> {
        if let Some(name) = path.strip_prefix("script.") {
            let value = self.scripts.as_ref()?.named_var(&self.entities, entity, name)?;
            return from_graph(&value);
        }
        if let Some(rest) = path.strip_prefix("ui.") {
            let (node, prop) = rest.rsplit_once('.')?;
            return read_ui(&self.game_screens, node, prop);
        }
        let found = self.entities.iter().find(|e| e.id == entity || e.name == entity)?;
        read_entity_field(found, path)
    }
}

fn to_graph(value: &PropValue, ty: Ty) -> Option<Value> {
    match ty {
        Ty::Bool => match value {
            PropValue::Bool(v) => Some(Value::Bool(*v)),
            _ => None,
        },
        Ty::Int => number(value).map(|n| Value::Int(n.round() as i64)),
        Ty::Float => number(value).map(|n| Value::Float(n as f64)),
        Ty::Str => match value {
            PropValue::Text(v) => Some(Value::Str(v.clone())),
            _ => None,
        },
        Ty::Vec2 => pair(value).map(|v| Value::Vec2(V2 { x: v[0] as f64, y: v[1] as f64 })),
        Ty::Vec3 => triple(value)
            .map(|v| Value::Vec3(V3 { x: v[0] as f64, y: v[1] as f64, z: v[2] as f64 })),
        _ => None,
    }
}

fn from_graph(value: &Value) -> Option<PropValue> {
    Some(match value {
        Value::Bool(v) => PropValue::Bool(*v),
        Value::Int(v) => PropValue::Number(*v as f32),
        Value::Float(v) => PropValue::Number(*v as f32),
        Value::Str(v) => PropValue::Text(v.clone()),
        Value::Vec2(v) => PropValue::Vector(vec![v.x as f32, v.y as f32]),
        Value::Vec3(v) => PropValue::Vector(vec![v.x as f32, v.y as f32, v.z as f32]),
        _ => return None,
    })
}

fn number(value: &PropValue) -> Option<f32> {
    match value {
        PropValue::Number(v) | PropValue::Angle(v) if v.is_finite() => Some(*v),
        _ => None,
    }
}

fn pair(value: &PropValue) -> Option<[f32; 2]> {
    match value {
        PropValue::Vector(v) | PropValue::Color(v) if v.len() == 2 => Some([v[0], v[1]]),
        _ => None,
    }
}

fn triple(value: &PropValue) -> Option<[f32; 3]> {
    match value {
        PropValue::Vector(v) | PropValue::Color(v) if v.len() == 3 => Some([v[0], v[1], v[2]]),
        _ => None,
    }
}

fn apply_ui(screens: &mut Screens, node: &str, prop: &str, value: &PropValue) -> bool {
    if node.is_empty() || prop.is_empty() {
        return false;
    }
    let mut hit = false;
    for (_, live) in &mut screens.shown {
        if let Some(index) = live.screen.find(node) {
            if write_node(&mut live.screen.nodes[index], prop, value) {
                hit = true;
            }
        }
    }
    hit
}

fn read_ui(screens: &Screens, node: &str, prop: &str) -> Option<PropValue> {
    for (_, live) in &screens.shown {
        if let Some(index) = live.screen.find(node) {
            return read_node(&live.screen.nodes[index], prop);
        }
    }
    None
}

fn write_node(node: &mut Node, prop: &str, value: &PropValue) -> bool {
    match prop {
        "text" => {
            node.text = match text(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "placeholder" => {
            node.placeholder = match text(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "picture" => {
            node.image = match text(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "font" => {
            node.font = match text(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "align" => {
            node.align = match text(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "value" => {
            node.value = match number(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "min" => {
            node.min = match number(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "max" => {
            node.max = match number(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "step" => {
            node.step = match number(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "font_size" => {
            node.font_size = match number(value) {
                Some(v) => Some(v),
                None => return false,
            }
        }
        "spacing" => {
            node.spacing = match number(value) {
                Some(v) => Some(v),
                None => return false,
            }
        }
        "padding" => {
            node.padding = match number(value) {
                Some(v) => Some(v),
                None => return false,
            }
        }
        "columns" => {
            node.columns = match number(value) {
                Some(v) if v >= 0.0 => v.round() as usize,
                _ => return false,
            }
        }
        "selected" => {
            node.selected = match number(value) {
                Some(v) if v >= 0.0 => v.round() as usize,
                _ => return false,
            }
        }
        "checked" => {
            node.checked = match flag(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "visible" => {
            node.visible = match flag(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "enabled" => {
            node.enabled = match flag(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "color" => {
            node.color = match color4(value) {
                Some(v) => Some(v),
                None => return false,
            }
        }
        "anchor" => {
            node.anchor = match quad(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "offset" => {
            node.offset = match quad(value) {
                Some(v) => v,
                None => return false,
            }
        }
        "size" => {
            node.size = match pair(value) {
                Some(v) => v,
                None => return false,
            }
        }
        _ => return false,
    }
    true
}

/// The value of one UI control property, for the Timeline key button.
pub fn ui_node_value(node: &Node, prop: &str) -> Option<PropValue> {
    read_node(node, prop)
}

fn read_node(node: &Node, prop: &str) -> Option<PropValue> {
    Some(match prop {
        "text" => PropValue::Text(node.text.clone()),
        "placeholder" => PropValue::Text(node.placeholder.clone()),
        "picture" => PropValue::Text(node.image.clone()),
        "font" => PropValue::Text(node.font.clone()),
        "align" => PropValue::Text(node.align.clone()),
        "value" => PropValue::Number(node.value),
        "min" => PropValue::Number(node.min),
        "max" => PropValue::Number(node.max),
        "step" => PropValue::Number(node.step),
        "font_size" => PropValue::Number(node.font_size.unwrap_or(0.0)),
        "spacing" => PropValue::Number(node.spacing.unwrap_or(0.0)),
        "padding" => PropValue::Number(node.padding.unwrap_or(0.0)),
        "columns" => PropValue::Number(node.columns as f32),
        "selected" => PropValue::Number(node.selected as f32),
        "checked" => PropValue::Bool(node.checked),
        "visible" => PropValue::Bool(node.visible),
        "enabled" => PropValue::Bool(node.enabled),
        "color" => PropValue::Color(node.color.unwrap_or([1.0; 4]).to_vec()),
        "anchor" => PropValue::Vector(node.anchor.to_vec()),
        "offset" => PropValue::Vector(node.offset.to_vec()),
        "size" => PropValue::Vector(node.size.to_vec()),
        _ => return None,
    })
}

fn text(value: &PropValue) -> Option<String> {
    match value {
        PropValue::Text(v) => Some(v.clone()),
        _ => None,
    }
}

fn flag(value: &PropValue) -> Option<bool> {
    match value {
        PropValue::Bool(v) => Some(*v),
        _ => None,
    }
}

fn color4(value: &PropValue) -> Option<[f32; 4]> {
    match value {
        PropValue::Color(v) if v.len() == 4 => Some([
            v[0].clamp(0.0, 1.0),
            v[1].clamp(0.0, 1.0),
            v[2].clamp(0.0, 1.0),
            v[3].clamp(0.0, 1.0),
        ]),
        PropValue::Color(v) if v.len() == 3 => {
            Some([v[0].clamp(0.0, 1.0), v[1].clamp(0.0, 1.0), v[2].clamp(0.0, 1.0), 1.0])
        }
        _ => None,
    }
}

fn quad(value: &PropValue) -> Option<[f32; 4]> {
    match value {
        PropValue::Vector(v) if v.len() == 4 => Some([v[0], v[1], v[2], v[3]]),
        _ => None,
    }
}
