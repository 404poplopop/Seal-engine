//! Help > Recipes: small examples imported into the open project.

use std::path::Path;

use seal_world::{Shape, World};

use crate::error::CoreError;
use crate::session::EditorSession;

#[derive(Clone, Debug)]
pub struct Recipe {
    pub id: &'static str,
    pub title_key: &'static str,
    pub description_key: &'static str,
}

pub const RECIPES: &[Recipe] = &[
    Recipe {
        id: "door_key",
        title_key: "recipe-door-key-title",
        description_key: "recipe-door-key-desc",
    },
    Recipe {
        id: "inventory_pickup",
        title_key: "recipe-inventory-title",
        description_key: "recipe-inventory-desc",
    },
    Recipe {
        id: "moving_platform",
        title_key: "recipe-platform-title",
        description_key: "recipe-platform-desc",
    },
    Recipe {
        id: "checkpoint",
        title_key: "recipe-checkpoint-title",
        description_key: "recipe-checkpoint-desc",
    },
];

pub fn recipe_by_id(id: &str) -> Option<&'static Recipe> {
    RECIPES.iter().find(|r| r.id == id)
}

pub fn import_recipe(session: &mut EditorSession, id: &str) -> Result<(), CoreError> {
    let root = session
        .project
        .as_ref()
        .map(|p| p.root.clone())
        .ok_or_else(|| CoreError::new("open a project first"))?;
    recipe_by_id(id).ok_or_else(|| CoreError::new(format!("unknown recipe {id}")))?;
    std::fs::create_dir_all(root.join("logic"))?;
    std::fs::create_dir_all(root.join("recipes"))?;
    write_recipe_files(&root, id)?;
    apply_recipe_to_world(&mut session.world, id)?;
    session.scene_dirty = true;
    session.models.panel_states.insert("recipe.last_import".into(), id.to_string());
    session.notify(session.messages.format("recipe-imported", &[("id", id)]), false);
    Ok(())
}

fn write_recipe_files(root: &Path, id: &str) -> Result<(), CoreError> {
    std::fs::create_dir_all(root.join("logic"))?;
    std::fs::create_dir_all(root.join("recipes"))?;
    let (graph_name, graph) = match id {
        "door_key" => ("door_key.graph", DOOR_KEY_GRAPH),
        "inventory_pickup" => ("inventory.graph", INVENTORY_GRAPH),
        "moving_platform" => ("platform.graph", PLATFORM_GRAPH),
        "checkpoint" => ("checkpoint.graph", CHECKPOINT_GRAPH),
        _ => return Err(CoreError::new("unknown recipe")),
    };
    std::fs::write(root.join("logic").join(graph_name), graph)?;
    std::fs::write(
        root.join("recipes").join(format!("{id}.sealrecipe")),
        format!("schema_version = 1\nrecipe = \"{id}\"\n"),
    )?;
    Ok(())
}

fn apply_recipe_to_world(world: &mut World, id: &str) -> Result<(), CoreError> {
    let folder = world
        .entities
        .iter()
        .find(|e| e.id == "gameplay")
        .map(|_| "gameplay".to_string())
        .unwrap_or_else(|| "root".to_string());
    match id {
        "door_key" => {
            add_prop(
                world,
                &folder,
                "recipe_door",
                "Door",
                [4.0, 1.0, 0.0],
                Shape::Cube,
                "logic/door_key.graph",
            );
            add_prop(
                world,
                &folder,
                "recipe_key",
                "Key",
                [-2.0, 0.6, 2.0],
                Shape::Cube,
                "logic/door_key.graph",
            );
            if let Some(door) = world.entities.iter_mut().find(|e| e.id == "recipe_door") {
                door.tag = "door".into();
                door.source_asset = "door".into();
                door.scale = [1.0, 2.0, 0.2];
                door.color = [0.45, 0.32, 0.22];
            }
            if let Some(k) = world.entities.iter_mut().find(|e| e.id == "recipe_key") {
                k.tag = "key".into();
                k.source_asset = "key".into();
                k.scale = [0.3, 0.1, 0.5];
                k.color = [0.9, 0.8, 0.2];
                k.volume.kind = "trigger".into();
                k.volume.event = "key_taken".into();
                k.volume.tag = "player".into();
            }
        }
        "inventory_pickup" => {
            let eid = add_prop(
                world,
                &folder,
                "recipe_pickup",
                "Pickup",
                [0.0, 0.5, 3.0],
                Shape::Sphere,
                "logic/inventory.graph",
            );
            if let Some(e) = world.entity_mut(&eid) {
                e.volume.kind = "trigger".into();
                e.volume.event = "pickup".into();
                e.volume.tag = "player".into();
            }
        }
        "moving_platform" => {
            let id = world.add("cube");
            if let Some(e) = world.entity_mut(&id) {
                e.id = "recipe_platform".into();
                e.name = "Moving Platform".into();
                e.parent = Some(folder);
                e.translation = [-2.0, 0.3, -2.0];
                e.scale = [2.0, 0.2, 1.0];
                e.dynamic = true;
                e.script = "logic/platform.graph".into();
                e.color = [0.5, 0.55, 0.6];
            }
        }
        "checkpoint" => {
            let eid = add_prop(
                world,
                &folder,
                "recipe_checkpoint",
                "Checkpoint",
                [2.0, 0.2, -3.0],
                Shape::Cylinder,
                "logic/checkpoint.graph",
            );
            if let Some(e) = world.entity_mut(&eid) {
                e.volume.kind = "trigger".into();
                e.volume.event = "save_here".into();
                e.volume.tag = "player".into();
            }
        }
        _ => return Err(CoreError::new("unknown recipe")),
    }
    Ok(())
}

fn add_prop(
    world: &mut World,
    parent: &str,
    id: &str,
    name: &str,
    at: [f32; 3],
    shape: Shape,
    script: &str,
) -> String {
    let eid = world.add("cube");
    let id_owned = if let Some(e) = world.entity_mut(&eid) {
        e.id = id.into();
        e.name = name.into();
        e.parent = Some(parent.into());
        e.translation = at;
        e.shape = Some(shape);
        e.script = script.into();
        e.id.clone()
    } else {
        eid
    };
    id_owned
}

const DOOR_KEY_GRAPH: &str = r#"schema_version = 1
seal_graph = "door_key"
vars = []
nodes = [
  { id = 1, kind = "event", name = "key_taken", params = [{ name = "other", type = "entity" }] },
  { id = 2, kind = "set_saved", key = "has_key", value = "1" },
  { id = 3, kind = "self" },
  { id = 4, kind = "despawn" },
  { id = 10, kind = "event", name = "tick" },
  { id = 11, kind = "saved", key = "has_key" },
  { id = 12, kind = "compare", type = "str", op = "eq", b = "1" },
  { id = 13, kind = "branch" },
  { id = 14, kind = "find_first", prefab = "door" },
  { id = 15, kind = "world_set", component = "sprite", field = "hidden", type = "bool", value = true },
]
links = [
  "1.then -> 2.in",
  "2.then -> 4.in",
  "3.out -> 4.entity",
  "10.then -> 13.in",
  "11.out -> 12.a",
  "12.out -> 13.cond",
  "13.true -> 15.in",
  "14.out -> 15.entity",
]
"#;

const INVENTORY_GRAPH: &str = r#"schema_version = 1
seal_graph = "inventory"
vars = []
nodes = [
  { id = 1, kind = "event", name = "pickup_enter", params = [{ name = "other", type = "entity" }] },
  { id = 2, kind = "world_get", component = "editor", field = "tag", type = "str" },
  { id = 3, kind = "compare", type = "str", op = "eq", defaults = { b = "player" } },
  { id = 4, kind = "branch" },
  { id = 5, kind = "add_score", defaults = { name = "items", amount = 1 } },
  { id = 6, kind = "self" },
  { id = 7, kind = "despawn" },
]
links = [
  "1.then -> 4.in",
  "1.other -> 2.entity",
  "2.out -> 3.a",
  "3.out -> 4.cond",
  "4.true -> 5.in",
  "5.then -> 7.in",
  "6.out -> 7.entity",
]
"#;

const PLATFORM_GRAPH: &str = r#"schema_version = 1
seal_graph = "platform"
vars = [ { name = "t", type = "float", default = 0.0 } ]
nodes = [
  { id = 1, kind = "event", name = "tick", params = [{ name = "dt", type = "float" }] },
  { id = 2, kind = "frame_dt" },
  { id = 3, kind = "get", name = "t" },
  { id = 4, kind = "add", type = "float" },
  { id = 5, kind = "set", name = "t" },
  { id = 6, kind = "get", name = "t" },
  { id = 7, kind = "sin", type = "float" },
  { id = 8, kind = "mul", type = "float", defaults = { b = 2.0 } },
  { id = 9, kind = "self" },
  { id = 10, kind = "world_get", component = "transform", field = "translation", type = "vec3" },
  { id = 11, kind = "world_set", component = "transform", field = "translation", type = "vec3" },
]
links = [
  "1.then -> 5.in",
  "2.out -> 4.a",
  "3.out -> 4.b",
  "4.out -> 5.value",
  "5.then -> 11.in",
  "6.out -> 7.a",
  "7.out -> 8.a",
  "9.out -> 10.entity",
  "9.out -> 11.entity",
]
"#;

const CHECKPOINT_GRAPH: &str = r#"schema_version = 1
seal_graph = "checkpoint"
vars = []
nodes = [
  { id = 1, kind = "event", name = "save_here", params = [{ name = "other", type = "entity" }] },
  { id = 2, kind = "set_saved", key = "checkpoint", value = "1" },
  { id = 3, kind = "log", message = "Checkpoint saved" },
]
links = [
  "1.then -> 2.in",
  "2.then -> 3.in",
]
"#;

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use seal_world::PlayMode;

    static RECIPE_TEMP: AtomicU64 = AtomicU64::new(0);

    fn empty_project() -> (std::path::PathBuf, World) {
        let n = RECIPE_TEMP.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("seal-recipe-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        crate::create_project(&dir, "Recipe").unwrap();
        std::fs::write(dir.join("scene.seal"), seal_world::World::starter().to_text()).unwrap();
        let w =
            seal_world::World::from_text(&std::fs::read_to_string(dir.join("scene.seal")).unwrap())
                .unwrap();
        (dir, w)
    }

    #[test]
    fn each_recipe_adds_objects_and_scripts() {
        for id in ["door_key", "inventory_pickup", "moving_platform", "checkpoint"] {
            let (dir, mut w) = empty_project();
            w.project_dir = Some(dir.clone());
            apply_recipe_to_world(&mut w, id).unwrap();
            write_recipe_files(&dir, id).unwrap();
            assert!(dir.join("logic").read_dir().unwrap().count() >= 1, "{id}");
            assert!(w.entities.iter().any(|e| e.id.starts_with("recipe_")), "{id}");
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    #[cfg(feature = "play-preview")]
    #[test]
    fn inventory_pickup_scores_when_player_is_near() {
        let (dir, mut w) = empty_project();
        apply_recipe_to_world(&mut w, "inventory_pickup").unwrap();
        write_recipe_files(&dir, "inventory_pickup").unwrap();
        w.project_dir = Some(dir.clone());
        if let Some(p) = w.entities.iter_mut().find(|e| e.id == "hero") {
            p.id = "player".into();
            p.tag = "player".into();
            p.source_asset = "player".into();
        }
        if let Some(p) = w.entities.iter_mut().find(|e| e.tag == "player") {
            p.dynamic = true;
            p.walk_speed = 0.0;
            p.translation = [0.0, 0.5, 3.05];
        }
        w.play();
        for _ in 0..40 {
            w.step(1.0 / 60.0);
        }
        assert_eq!(w.mode, PlayMode::Playing);
        let items = w.play_game().and_then(|g| g.scores.get("items")).copied().unwrap_or(0);
        assert!(items >= 1, "pickup scored {items} log {:?}", w.play_log);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
