//! AI orders: objects walk around walls to a point, patrol, chase, and
//! scripts can order them and ask whether they arrived.

use seal_world::ai::{grid_for, Ai, Goal};
use seal_world::{Entity, World};

/// An empty scene with a floor, a 1-unit walker at the origin and a wall
/// from x = 1.5 to 2.5 spanning z = -3..3, standing on the floor.
fn scene() -> (World, Vec<Entity>) {
    let mut w = World::starter();
    w.entities.retain(|e| e.shape.is_none() && e.tag != "player");
    let floor = w.add("cube");
    let walker = w.add("cube");
    let wall = w.add("cube");
    for e in &mut w.entities {
        e.dynamic = false;
        e.tag.clear();
        if e.id == floor {
            e.translation = [0.0, -0.5, 0.0];
            e.scale = [40.0, 1.0, 40.0];
        } else if e.id == walker {
            e.translation = [0.0, 0.5, 0.0];
            e.scale = [0.5, 1.0, 0.5];
            e.name = "Walker".into();
        } else if e.id == wall {
            e.translation = [2.0, 1.0, 0.0];
            e.scale = [1.0, 2.0, 6.0];
        }
    }
    let ents = w.entities.clone();
    (w, ents)
}

fn pos(ents: &[Entity], id: &str) -> [f32; 3] {
    ents.iter().find(|e| e.id == id).unwrap().translation
}

#[test]
fn the_floor_is_ground_and_the_wall_blocks() {
    let (_, ents) = scene();
    let walker = ents.iter().find(|e| e.name == "Walker").unwrap();
    let g = grid_for(&ents, walker, &[]);
    let (wx, wz) = (g.cell_of((2.0, 0.0)).unwrap(), g.cell_of((-3.0, 0.0)).unwrap());
    assert!(!g.is_walkable(wx.0 as i64, wx.1 as i64), "wall cell is blocked");
    assert!(g.is_walkable(wz.0 as i64, wz.1 as i64), "open floor is walkable");
}

#[test]
fn move_to_walks_around_the_wall_and_arrives() {
    let (_, mut ents) = scene();
    let id = ents.iter().find(|e| e.name == "Walker").unwrap().id.clone();
    let mut ai = Ai::default();
    ai.order(&id, Goal::MoveTo { target: [5.0, 0.5, 0.0], speed: 4.0 });
    let mut log = Vec::new();
    let mut went_around = false;
    for _ in 0..200 {
        ai.step(0.05, &mut ents, &mut log);
        let p = pos(&ents, &id);
        assert!(
            !(p[0] > 1.25 && p[0] < 2.75 && p[2].abs() < 3.25),
            "walked through the wall at {p:?}"
        );
        went_around |= p[2].abs() >= 3.0;
    }
    let p = pos(&ents, &id);
    assert!((p[0] - 5.0).abs() < 1e-4 && p[2].abs() < 1e-4, "ended at {p:?}, log {log:?}");
    assert_eq!(p[1], 0.5, "height is kept");
    assert!(went_around);
    assert!(ai.arrived.contains(&id) && ai.agents.is_empty());
}

#[test]
fn speed_limits_each_step_and_an_unreachable_point_is_logged() {
    let (_, mut ents) = scene();
    let id = ents.iter().find(|e| e.name == "Walker").unwrap().id.clone();
    let mut ai = Ai::default();
    ai.order(&id, Goal::MoveTo { target: [-5.0, 0.5, 0.0], speed: 2.0 });
    let mut log = Vec::new();
    ai.step(0.25, &mut ents, &mut log);
    let p = pos(&ents, &id);
    let went = (p[0] * p[0] + p[2] * p[2]).sqrt();
    assert!(p[0] < -0.3 && went <= 0.5 + 1e-4, "2 units/s for 0.25 s: {p:?} {log:?}");
    ai.order(&id, Goal::MoveTo { target: [2.0, 0.5, 0.0], speed: 2.0 });
    ai.step(0.25, &mut ents, &mut log);
    assert_eq!(log.len(), 1);
    assert!(log[0].contains("Walker has no path"), "{log:?}");
    assert!(ai.agents.is_empty() && !ai.arrived.contains(&id));
}

#[test]
fn patrol_goes_back_and_forth_and_chase_stops_next_to_the_target() {
    let (mut w, mut ents) = scene();
    let id = ents.iter().find(|e| e.name == "Walker").unwrap().id.clone();
    let mut ai = Ai::default();
    ai.order(&id, Goal::Patrol { a: [-2.0, 0.5, 0.0], b: [-2.0, 0.5, 4.0], speed: 4.0 });
    let mut log = Vec::new();
    let mut seen = (false, false);
    for _ in 0..120 {
        ai.step(0.05, &mut ents, &mut log);
        let p = pos(&ents, &id);
        seen.0 |= p[2].abs() < 1e-4 && (p[0] + 2.0).abs() < 1e-4;
        seen.1 |= (p[2] - 4.0).abs() < 1e-4;
        if seen.0 && seen.1 && p[2].abs() < 1e-4 {
            break;
        }
    }
    assert!(seen.0 && seen.1, "patrol reached both ends");
    assert_eq!(ai.agents.len(), 1, "patrol keeps going");

    let target = w.add("sphere");
    let mut t = w.entities.iter().find(|e| e.id == target).unwrap().clone();
    t.translation = [6.0, 0.5, 2.0];
    t.scale = [0.5, 0.5, 0.5];
    t.dynamic = true; // moving things are not walls
    ents.push(t);
    ai.order(&id, Goal::Chase { target: target.clone(), speed: 5.0 });
    for _ in 0..200 {
        ai.step(0.05, &mut ents, &mut log);
    }
    let (p, q) = (pos(&ents, &id), pos(&ents, &target));
    let d = ((p[0] - q[0]).powi(2) + (p[2] - q[2]).powi(2)).sqrt();
    assert!(d <= 1.5 + 1e-3, "chaser ends next to the target: {d}");
    assert!(ai.arrived.contains(&id) && ai.agents.len() == 1, "chase stays active");
    // The target moves away: the chase follows again.
    ents.iter_mut().find(|e| e.id == target).unwrap().translation = [-6.0, 0.5, -5.0];
    for _ in 0..200 {
        ai.step(0.05, &mut ents, &mut log);
    }
    let p = pos(&ents, &id);
    assert!(((p[0] + 6.0).powi(2) + (p[2] + 5.0).powi(2)).sqrt() <= 1.5 + 1e-3, "{p:?}");
    ai.order(&id, Goal::Stop);
    assert!(ai.agents.is_empty());
}

const GUARD: &str = r#"
schema_version = 1
seal_graph = "guard"
vars = []
nodes = [
  { id = 1, kind = "event", name = "start" },
  { id = 2, kind = "self" },
  { id = 3, kind = "ai_move_to", defaults = { target = [5.0, 0.5, 0.0], speed = 4.0 } },
  { id = 4, kind = "event", name = "tick" },
  { id = 5, kind = "ai_arrived" },
  { id = 6, kind = "branch" },
  { id = 7, kind = "log", defaults = { message = "arrived" } },
  { id = 8, kind = "ai_stop" },
]
links = [
  "1.then -> 3.in", "2.out -> 3.entity", "4.then -> 6.in", "2.out -> 5.entity", "5.out -> 6.cond",
  "6.true -> 7.in", "7.then -> 8.in", "2.out -> 8.entity",
]
"#;

#[test]
fn a_script_orders_its_object_and_hears_when_it_arrived() {
    let dir = std::env::temp_dir().join(format!("seal-world-ai-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("logic")).unwrap();
    std::fs::write(dir.join("logic/guard.graph"), GUARD).unwrap();
    let (mut w, _) = scene();
    let i = w.entities.iter().position(|e| e.name == "Walker").unwrap();
    let start = w.entities[i].translation;
    w.entities[i].script = "logic/guard.graph".into();
    w.project_dir = Some(dir.clone());
    w.play();
    for _ in 0..200 {
        w.step(0.05);
    }
    let p = w.entities[i].translation;
    assert!((p[0] - 5.0).abs() < 1e-4 && p[2].abs() < 1e-4, "{p:?} {:?}", w.play_log);
    assert!(w.play_log.iter().any(|l| l == "arrived"), "{:?}", w.play_log);
    w.stop();
    assert_eq!(w.entities[i].translation, start);
    assert!(w.ai.agents.is_empty() && w.ai.arrived.is_empty());
    std::fs::remove_dir_all(dir).ok();
}
