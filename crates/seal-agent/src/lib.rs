//! An MCP server over a Seal world, so AI agents can inspect and edit it.
//!
//! Messages are JSON-RPC 2.0, one per line (the MCP stdio transport). Every
//! tool that changes the world is all-or-nothing: if it fails part way the
//! world is put back, and each successful change is one undo step. `batch`
//! runs several tools as a single all-or-nothing, single-undo step.
//!
//! Entities are written as `"5v1"` (index v generation). Component values
//! are JSON objects whose fields match the component's SealText fields;
//! references to other entities inside them use the same `"5v1"` form.
#![forbid(unsafe_code)]

use std::path::Path;

use seal_ecs::{Entity, History, Prefab, PrefabLibrary, SealValue, Vocabulary, World};
use serde_json::{json, Map, Value};

pub const PROTOCOL_VERSION: &str = "2025-06-18";

pub struct Session {
    pub world: World,
    /// Prefabs (classes) available to instantiate. Not part of undo.
    pub library: PrefabLibrary,
    history: History,
}

type ToolResult = Result<Value, String>;

fn entity(args: &Value, key: &str) -> Result<Entity, String> {
    let text = args
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing \"{key}\" (an entity id like \"5v1\")"))?;
    Entity::parse(text).ok_or_else(|| format!("\"{text}\" is not an entity id like \"5v1\""))
}

fn string<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key).and_then(Value::as_str).ok_or_else(|| format!("missing \"{key}\""))
}

fn text_or_path(args: &Value) -> Result<String, String> {
    match (args.get("text").and_then(Value::as_str), args.get("path").and_then(Value::as_str)) {
        (Some(text), _) => Ok(text.to_string()),
        (None, Some(path)) => {
            std::fs::read_to_string(Path::new(path)).map_err(|e| format!("{path}: {e}"))
        }
        (None, None) => Err("give text or path".into()),
    }
}

fn id(e: Entity) -> Value {
    Value::String(format!("{e:?}"))
}

/// Tool descriptions for `tools/list`.
pub fn tool_list() -> Value {
    let entity = json!({ "type": "string", "description": "Entity id like \"5v1\"" });
    let tools = [
        ("world_info", "Entity count, every component type with its fields, and whether undo or redo is available.", json!({})),
        (
            "find_entities",
            "List entities (id, name, parent). Filter by components they all have and by part of their name.",
            json!({ "with": { "type": "array", "items": { "type": "string" } }, "name_contains": { "type": "string" }, "limit": { "type": "integer", "default": 100 } }),
        ),
        ("get_entity", "Every component of one entity, plus its parent and children.", json!({ "id": entity })),
        (
            "spawn",
            "Create an entity with the given components, optionally under a parent. Returns its id.",
            json!({ "components": { "type": "object", "description": "Component name to value" }, "parent": entity }),
        ),
        ("despawn", "Delete an entity; with recursive, its descendants too (otherwise they become roots).", json!({ "id": entity, "recursive": { "type": "boolean" } })),
        ("set_component", "Add or replace a whole component.", json!({ "id": entity, "component": { "type": "string" }, "value": {} })),
        ("set_field", "Change one field of a component, keeping the others.", json!({ "id": entity, "component": { "type": "string" }, "field": { "type": "string" }, "value": {} })),
        ("remove_component", "Remove a component from an entity.", json!({ "id": entity, "component": { "type": "string" } })),
        ("set_parent", "Attach an entity under a parent, or detach it with parent null. Cycles are refused.", json!({ "id": entity, "parent": { "type": ["string", "null"] } })),
        ("undo", "Undo the last change.", json!({})),
        ("redo", "Redo the last undone change.", json!({})),
        (
            "save_scene",
            "Return the world as SealText, or write it to path. compact stores only prefab overrides; vocabulary picks scene-graph or class words (same data).",
            json!({ "path": { "type": "string" }, "compact": { "type": "boolean" }, "vocabulary": { "type": "string", "enum": ["scene_graph", "classic"] } }),
        ),
        ("load_scene", "Add the entities from a SealText scene (text or path, either vocabulary, full or compact). Returns their ids.", json!({ "text": { "type": "string" }, "path": { "type": "string" } })),
        ("list_prefabs", "Every prefab (class) with its base chain, root first.", json!({})),
        ("add_prefab", "Add or replace a prefab from a prefab or class file (text or path).", json!({ "text": { "type": "string" }, "path": { "type": "string" } })),
        ("instantiate", "Spawn an instance of a prefab (an instance of a class) with its attached parts, optionally under a parent.", json!({ "prefab": { "type": "string" }, "parent": entity })),
        ("get_overrides", "What an instance changes relative to its prefab (null means a removed component).", json!({ "id": entity })),
        ("revert_to_prefab", "Put one component of an instance back to the prefab's value.", json!({ "id": entity, "component": { "type": "string" } })),
        (
            "batch",
            "Run several tools in order as one change: if any step fails, none of them happen. One undo step.",
            json!({ "steps": { "type": "array", "items": { "type": "object", "properties": { "tool": { "type": "string" }, "arguments": { "type": "object" } }, "required": ["tool"] } } }),
        ),
    ];
    let required = |name: &str| -> Vec<&str> {
        match name {
            "get_entity" | "despawn" => vec!["id"],
            "set_component" => vec!["id", "component", "value"],
            "set_field" => vec!["id", "component", "field", "value"],
            "remove_component" => vec!["id", "component"],
            "set_parent" => vec!["id", "parent"],
            "batch" => vec!["steps"],
            "instantiate" => vec!["prefab"],
            "get_overrides" => vec!["id"],
            "revert_to_prefab" => vec!["id", "component"],
            _ => vec![],
        }
    };
    Value::Array(
        tools
            .iter()
            .map(|(name, description, properties)| {
                json!({
                    "name": name,
                    "description": description,
                    "inputSchema": { "type": "object", "properties": properties, "required": required(name) },
                })
            })
            .collect(),
    )
}

fn changes_world(tool: &str) -> bool {
    matches!(
        tool,
        "spawn"
            | "despawn"
            | "set_component"
            | "set_field"
            | "remove_component"
            | "set_parent"
            | "load_scene"
            | "batch"
            | "instantiate"
            | "revert_to_prefab"
    )
}

impl Session {
    pub fn new(world: World) -> Self {
        Self { world, library: PrefabLibrary::new(), history: History::with_limit(200) }
    }

    /// Handle one JSON-RPC message. Notifications get no reply.
    pub fn handle(&mut self, message: &Value) -> Option<Value> {
        let request_id = message.get("id").cloned()?;
        let method = message.get("method").and_then(Value::as_str).unwrap_or("");
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        let result = match method {
            "initialize" => Ok(json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "seal-agent", "version": env!("CARGO_PKG_VERSION") },
                "instructions": "Inspect and edit a Seal world. Start with world_info. Every change can be undone; use batch for multi-step edits that must all succeed.",
            })),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tool_list() })),
            "tools/call" => {
                let name = params.get("name").and_then(Value::as_str).unwrap_or("");
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                let (text, is_error) = match self.call(name, &args) {
                    Ok(value) => (value.to_string(), false),
                    Err(message) => (message, true),
                };
                Ok(json!({ "content": [{ "type": "text", "text": text }], "isError": is_error }))
            }
            _ => Err(json!({ "code": -32601, "message": format!("unknown method {method}") })),
        };
        Some(match result {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": request_id, "result": result }),
            Err(error) => json!({ "jsonrpc": "2.0", "id": request_id, "error": error }),
        })
    }

    /// Run one tool. Changes are all-or-nothing and recorded for undo.
    pub fn call(&mut self, tool: &str, args: &Value) -> ToolResult {
        if !changes_world(tool) {
            return self.run(tool, args);
        }
        let before = self.world.snapshot().map_err(|e| e.to_string())?;
        match self.run(tool, args) {
            Ok(value) => {
                self.history.push(before);
                Ok(value)
            }
            Err(message) => {
                self.world.restore(&before);
                Err(message)
            }
        }
    }

    fn run(&mut self, tool: &str, args: &Value) -> ToolResult {
        let w = &mut self.world;
        match tool {
            "world_info" => {
                let components: Map<String, Value> = w
                    .text_component_names()
                    .into_iter()
                    .map(|n| (n.to_string(), json!(w.component_fields(n).unwrap_or(&[]))))
                    .collect();
                Ok(json!({
                    "entities": w.len(),
                    "components": components,
                    "can_undo": self.history.can_undo(),
                    "can_redo": self.history.can_redo(),
                }))
            }
            "find_entities" => {
                let with: Vec<&str> = args
                    .get("with")
                    .and_then(Value::as_array)
                    .map_or(vec![], |a| a.iter().filter_map(Value::as_str).collect());
                for name in &with {
                    if w.component_fields(name).is_none() {
                        return Err(format!("unknown component {name}"));
                    }
                }
                let needle =
                    args.get("name_contains").and_then(Value::as_str).map(str::to_lowercase);
                let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(100) as usize;
                let mut found = Vec::new();
                let mut total = 0;
                for e in w.entities() {
                    let name = w.get::<seal_ecs::core::Name>(e).map(|n| n.0.clone());
                    if !with.iter().all(|c| w.component_value(e, c).is_some()) {
                        continue;
                    }
                    if let Some(needle) = &needle {
                        if !name.as_deref().unwrap_or("").to_lowercase().contains(needle) {
                            continue;
                        }
                    }
                    total += 1;
                    if found.len() < limit {
                        found.push(
                            json!({ "id": id(e), "name": name, "parent": w.parent(e).map(id) }),
                        );
                    }
                }
                Ok(json!({ "total": total, "entities": found }))
            }
            "get_entity" => {
                let e = entity(args, "id")?;
                let values = w.entity_values(e).ok_or_else(|| format!("no entity {e:?}"))?;
                let components: Map<String, Value> =
                    values.into_iter().map(|(n, v)| (n.to_string(), v.to_json())).collect();
                Ok(json!({
                    "id": id(e),
                    "components": components,
                    "parent": w.parent(e).map(id),
                    "children": w.children(e).iter().map(|c| id(*c)).collect::<Vec<_>>(),
                }))
            }
            "spawn" => {
                let e = w.spawn(());
                if let Some(components) = args.get("components") {
                    let map = components.as_object().ok_or("components must be an object")?;
                    for (name, value) in map {
                        if name == "child_of" {
                            return Err("use the parent argument instead of child_of".into());
                        }
                        w.set_component_value(e, name, &SealValue::from_json(value))?;
                    }
                }
                if args.get("parent").is_some_and(|p| !p.is_null()) {
                    let parent = entity(args, "parent")?;
                    w.set_parent(e, Some(parent)).map_err(|err| format!("{err:?}"))?;
                }
                Ok(json!({ "id": id(e) }))
            }
            "despawn" => {
                let e = entity(args, "id")?;
                if !w.contains(e) {
                    return Err(format!("no entity {e:?}"));
                }
                let removed = if args.get("recursive").and_then(Value::as_bool).unwrap_or(false) {
                    w.despawn_recursive(e)
                } else {
                    usize::from(w.despawn(e))
                };
                Ok(json!({ "removed": removed }))
            }
            "set_component" => {
                let e = entity(args, "id")?;
                let component = string(args, "component")?;
                if component == "child_of" {
                    return Err("use set_parent to change parents".into());
                }
                let value = args.get("value").ok_or("missing \"value\"")?;
                w.set_component_value(e, component, &SealValue::from_json(value))?;
                Ok(json!({ "ok": true }))
            }
            "set_field" => {
                let e = entity(args, "id")?;
                let component = string(args, "component")?;
                if component == "child_of" {
                    return Err("use set_parent to change parents".into());
                }
                let value = args.get("value").ok_or("missing \"value\"")?;
                w.set_component_field(
                    e,
                    component,
                    string(args, "field")?,
                    SealValue::from_json(value),
                )?;
                Ok(json!({ "value": w.component_value(e, component).map(|v| v.to_json()) }))
            }
            "remove_component" => {
                let e = entity(args, "id")?;
                let component = string(args, "component")?;
                if component == "child_of" {
                    return Err("use set_parent with parent null to detach".into());
                }
                Ok(json!({ "removed": w.remove_component_named(e, component)? }))
            }
            "set_parent" => {
                let e = entity(args, "id")?;
                let parent = match args.get("parent") {
                    None | Some(Value::Null) => None,
                    Some(_) => Some(entity(args, "parent")?),
                };
                w.set_parent(e, parent).map_err(|err| format!("{err:?}"))?;
                Ok(json!({ "ok": true }))
            }
            "undo" => Ok(json!({ "undone": self.history.undo(&mut self.world) })),
            "redo" => Ok(json!({ "redone": self.history.redo(&mut self.world) })),
            "save_scene" => {
                let vocabulary = match args.get("vocabulary").and_then(Value::as_str) {
                    None | Some("scene_graph") => Vocabulary::SceneGraph,
                    Some("classic") => Vocabulary::Classic,
                    Some(other) => {
                        return Err(format!(
                            "unknown vocabulary {other}; use scene_graph or classic"
                        ))
                    }
                };
                let (text, skipped) = if args
                    .get("compact")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    (w.save_scene_compact(&self.library, vocabulary)?, w.save_scene(None).skipped)
                } else {
                    let saved = w.save_scene(None);
                    let text = match vocabulary {
                        Vocabulary::SceneGraph => saved.text,
                        Vocabulary::Classic => seal_ecs::to_classic(&saved.text)?,
                    };
                    (text, saved.skipped)
                };
                match args.get("path").and_then(Value::as_str) {
                    Some(path) => {
                        std::fs::write(Path::new(path), &text)
                            .map_err(|e| format!("{path}: {e}"))?;
                        Ok(json!({ "path": path, "bytes": text.len(), "skipped": skipped }))
                    }
                    None => Ok(json!({ "text": text, "skipped": skipped })),
                }
            }
            "load_scene" => {
                let text = match (
                    args.get("text").and_then(Value::as_str),
                    args.get("path").and_then(Value::as_str),
                ) {
                    (Some(text), _) => text.to_string(),
                    (None, Some(path)) => std::fs::read_to_string(Path::new(path))
                        .map_err(|e| format!("{path}: {e}"))?,
                    (None, None) => return Err("give text or path".into()),
                };
                let ids = w.load_scene_with(&self.library, &text)?;
                Ok(json!({ "ids": ids.into_iter().map(id).collect::<Vec<_>>() }))
            }
            "list_prefabs" => {
                let list: Vec<Value> = self
                    .library
                    .names()
                    .map(|n| json!({ "prefab": n, "chain": self.library.class_chain(n).unwrap_or_default() }))
                    .collect();
                Ok(Value::Array(list))
            }
            "add_prefab" => {
                let text = text_or_path(args)?;
                let prefab = Prefab::from_text(&text)?;
                let name = prefab.name.clone();
                let previous = self.library.insert(prefab);
                if let Err(message) = self.library.resolve(&name) {
                    match previous {
                        Some(p) => self.library.insert(p),
                        None => self.library.remove(&name),
                    };
                    return Err(message);
                }
                Ok(json!({ "prefab": name, "chain": self.library.class_chain(&name)? }))
            }
            "instantiate" => {
                let e = w.instantiate(&self.library, string(args, "prefab")?)?;
                if args.get("parent").is_some_and(|p| !p.is_null()) {
                    w.set_parent(e, Some(entity(args, "parent")?)).map_err(|e| format!("{e:?}"))?;
                }
                Ok(
                    json!({ "id": id(e), "children": w.descendants(e).into_iter().map(id).collect::<Vec<_>>() }),
                )
            }
            "get_overrides" => {
                let e = entity(args, "id")?;
                match w.instance_overrides(&self.library, e)? {
                    Some(patch) => Ok(
                        json!({ "prefab": w.get::<seal_ecs::PrefabInstance>(e).map(|p| p.0.clone()), "overrides": patch.to_json() }),
                    ),
                    None => Err("this entity does not come from a prefab".into()),
                }
            }
            "revert_to_prefab" => {
                let e = entity(args, "id")?;
                w.revert_to_prefab(&self.library, e, string(args, "component")?)?;
                Ok(
                    json!({ "value": w.component_value(e, string(args, "component")?).map(|v| v.to_json()) }),
                )
            }
            "batch" => {
                let steps =
                    args.get("steps").and_then(Value::as_array).ok_or("missing \"steps\"")?;
                let mut results = Vec::new();
                for (index, step) in steps.iter().enumerate() {
                    let tool = step
                        .get("tool")
                        .and_then(Value::as_str)
                        .ok_or(format!("step {index}: missing tool"))?;
                    if matches!(tool, "batch" | "undo" | "redo") {
                        return Err(format!("step {index}: {tool} is not allowed inside batch"));
                    }
                    let args = step.get("arguments").cloned().unwrap_or(json!({}));
                    results.push(
                        self.run(tool, &args).map_err(|e| {
                            format!("step {index} ({tool}): {e}; nothing was changed")
                        })?,
                    );
                }
                Ok(Value::Array(results))
            }
            other => Err(format!("unknown tool {other}")),
        }
    }
}
