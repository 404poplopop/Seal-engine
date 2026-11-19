//! Character 2D: slopes, steps, snap, platforms, crouch, jump timing, areas, save.

use seal_world::World;

const DT: f32 = 1.0 / 60.0;

fn bare() -> World {
    let mut world = World::starter();
    world.entities.retain(|e| e.id == "root");
    world
}

fn box_at(world: &mut World, at: [f32; 2], size: [f32; 2]) -> String {
    let id = world.add("empty");
    let e = world.entity_mut(&id).unwrap();
    e.translation = [at[0], at[1], 0.0];
    e.phys2d.body = "kinematic".into();
    e.phys2d.shape = "box".into();
    e.phys2d.size = size;
    id
}

fn character(world: &mut World, at: [f32; 2]) -> String {
    let id = world.add("character_2d");
    world.entity_mut(&id).unwrap().translation = [at[0], at[1], 0.0];
    id
}

fn step_n(world: &mut World, id: &str, n: u32, wish: [f32; 2]) {
    for _ in 0..n {
        if wish != [0.0, 0.0] {
            world.character_move(id, wish);
        }
        assert!(world.step(DT));
    }
}

fn settle(world: &mut World, id: &str) {
    for _ in 0..120 {
        assert!(world.step(DT));
        if world.character_status(id).is_some_and(|v| v.on_floor) {
            break;
        }
    }
    step_n(world, id, 5, [0.0, 0.0]);
    assert!(world.character_status(id).unwrap().on_floor, "never reached a floor");
}

fn pos(world: &World, id: &str) -> [f32; 2] {
    let e = world.entity(id).unwrap();
    [e.translation[0], e.translation[1]]
}

fn slope_world(angle_deg: f32, at: [f32; 2], apron: bool) -> (World, String) {
    let mut world = bare();
    if apron {
        box_at(&mut world, [-6.0, -0.5], [12.0, 1.0]);
    }
    let id = world.add("empty");
    {
        let e = world.entity_mut(&id).unwrap();
        e.phys2d.body = "fixed".into();
        e.phys2d.shape = "polygon".into();
        let h = 8.0 * angle_deg.to_radians().tan();
        e.phys2d.points = vec![[0.0, 0.0], [8.0, 0.0], [8.0, h]];
    }
    let who = character(&mut world, at);
    world.play();
    (world, who)
}

#[test]
fn a_gentle_slope_is_walkable_and_a_steep_one_is_not() {
    let (mut world, who) = slope_world(20.0, [-1.0, 2.0], true);
    settle(&mut world, &who);
    let landed = pos(&world, &who);
    step_n(&mut world, &who, 90, [1.0, 0.0]);
    let climbed = pos(&world, &who);
    assert!(climbed[1] > landed[1] + 0.4, "20° should raise the body: {landed:?} -> {climbed:?}");
    assert!(world.character_status(&who).unwrap().on_floor);

    let (mut world, who) = slope_world(60.0, [-1.0, 2.0], true);
    settle(&mut world, &who);
    let landed = pos(&world, &who);
    let mut hit_wall = false;
    for _ in 0..90 {
        world.character_move(&who, [1.0, 0.0]);
        world.step(DT);
        if world.character_status(&who).unwrap().on_wall {
            hit_wall = true;
        }
    }
    let stuck = pos(&world, &who);
    assert!(hit_wall, "a 60° face is a wall");
    assert!(stuck[0] < landed[0] + 1.0, "should not walk up 60°: {landed:?} -> {stuck:?}");
    assert!(stuck[1] < landed[1] + 0.25, "should not gain height on 60°: {landed:?} -> {stuck:?}");
}

#[test]
fn a_low_step_is_climbed_and_a_tall_one_blocks() {
    let mut world = bare();
    box_at(&mut world, [0.0, -0.5], [20.0, 1.0]);
    box_at(&mut world, [1.6, 0.15], [1.0, 0.3]);
    let who = character(&mut world, [0.0, 2.0]);
    world.play();
    settle(&mut world, &who);
    step_n(&mut world, &who, 50, [1.0, 0.0]);
    let at = pos(&world, &who);
    assert!(at[0] > 2.2, "should be past the 0.3 step, at {at:?}");

    let mut world = bare();
    box_at(&mut world, [0.0, -0.5], [20.0, 1.0]);
    box_at(&mut world, [1.6, 0.4], [1.0, 0.8]);
    let who = character(&mut world, [0.0, 2.0]);
    world.play();
    settle(&mut world, &who);
    let before = pos(&world, &who);
    let mut hit_wall = false;
    for _ in 0..40 {
        world.character_move(&who, [1.0, 0.0]);
        world.step(DT);
        if world.character_status(&who).unwrap().on_wall {
            hit_wall = true;
        }
    }
    let after = pos(&world, &who);
    assert!(hit_wall, "the tall step is a wall");
    assert!(after[0] < before[0] + 1.3, "should not pass a 0.8 step: {before:?} -> {after:?}");
}

#[test]
fn floor_snap_keeps_the_feet_on_a_downward_slope() {
    let (mut world, who) = slope_world(20.0, [5.0, 6.0], false);
    settle(&mut world, &who);
    let slope = 20.0f32.to_radians().tan();
    for _ in 0..40 {
        world.character_move(&who, [-1.0, 0.0]);
        world.step(DT);
        let view = world.character_status(&who).unwrap();
        let at = pos(&world, &who);
        let feet = at[1] - 0.8;
        let surface = at[0] * slope;
        assert!(view.on_floor, "left the slope at {at:?}");
        assert!((feet - surface).abs() < 0.35, "feet {feet} vs slope {surface} at {at:?}");
    }
    assert!(pos(&world, &who)[0] < 5.0, "should have walked downhill");
}

#[test]
fn a_moving_platform_carries_the_body() {
    let mut world = bare();
    let floor = box_at(&mut world, [0.0, -0.5], [6.0, 1.0]);
    let who = character(&mut world, [0.0, 2.0]);
    world.play();
    settle(&mut world, &who);
    let x0 = pos(&world, &who)[0];
    for _ in 0..10 {
        world.entity_mut(&floor).unwrap().translation[0] += 0.2;
        world.step(DT);
    }
    let at = pos(&world, &who);
    assert!((at[0] - x0 - 2.0).abs() < 0.25, "carry {x0} -> {}", at[0]);
    assert!(world.character_status(&who).unwrap().on_floor);
}

#[test]
fn crouch_is_refused_when_the_ceiling_is_too_low() {
    let mut world = bare();
    box_at(&mut world, [0.0, -0.5], [8.0, 1.0]);
    let ceiling = box_at(&mut world, [0.0, 30.0], [6.0, 1.0]);
    let who = character(&mut world, [0.0, 2.0]);
    world.play();
    settle(&mut world, &who);
    let stand_y = pos(&world, &who)[1];
    world.entity_mut(&who).unwrap().character2d.crouch = true;
    step_n(&mut world, &who, 3, [0.0, 0.0]);
    let crouched = world.character_status(&who).unwrap();
    assert!(crouched.crouched);
    assert!(!crouched.crouch_blocked);
    assert!(pos(&world, &who)[1] < stand_y - 0.3);
    world.entity_mut(&ceiling).unwrap().translation = [pos(&world, &who)[0], 1.5, 0.0];
    world.entity_mut(&who).unwrap().character2d.crouch = false;
    step_n(&mut world, &who, 3, [0.0, 0.0]);
    let blocked = world.character_status(&who).unwrap();
    assert!(blocked.crouch_blocked, "standing up should be refused");
    assert!(blocked.crouched, "the body stays crouched");
}

#[test]
fn a_jump_into_a_ceiling_reports_the_ceiling() {
    let mut world = bare();
    box_at(&mut world, [0.0, -0.5], [8.0, 1.0]);
    box_at(&mut world, [0.0, 2.2], [4.0, 0.4]);
    let who = character(&mut world, [0.0, 2.0]);
    world.play();
    settle(&mut world, &who);
    world.character_jump(&who);
    let mut hit = false;
    for _ in 0..40 {
        world.step(DT);
        if world.character_status(&who).unwrap().on_ceiling {
            hit = true;
        }
    }
    assert!(hit, "the jump should meet the ceiling");
}

#[test]
fn coyote_time_allows_a_late_jump_and_then_expires() {
    let jump_after = |air_frames: u32| -> f32 {
        let mut world = bare();
        box_at(&mut world, [0.0, -0.5], [4.0, 1.0]);
        let who = character(&mut world, [0.0, 2.0]);
        world.entity_mut(&who).unwrap().character2d.coyote_time = 0.2;
        world.play();
        settle(&mut world, &who);
        loop {
            world.character_move(&who, [1.0, 0.0]);
            world.step(DT);
            if !world.character_status(&who).unwrap().on_floor {
                break;
            }
        }
        for _ in 0..air_frames {
            world.character_move(&who, [1.0, 0.0]);
            world.step(DT);
        }
        world.character_move(&who, [1.0, 0.0]);
        world.character_jump(&who);
        world.step(DT);
        world.character_status(&who).unwrap().velocity[1]
    };
    assert!(jump_after(6) > 1.0, "a jump 6 frames after leaving should still count");
    assert!(jump_after(20) < 0.0, "a jump 20 frames later should have missed coyote time");
}

#[test]
fn a_jump_pressed_just_before_landing_is_remembered() {
    let hopped = |buffer: f32| -> bool {
        let mut world = bare();
        box_at(&mut world, [0.0, -0.5], [8.0, 1.0]);
        let who = character(&mut world, [0.0, 1.15]);
        world.entity_mut(&who).unwrap().character2d.jump_buffer = buffer;
        world.play();
        world.character_jump(&who);
        world.step(DT);
        let mut seen_floor = false;
        let mut hopped = false;
        let mut low = f32::MAX;
        for _ in 0..40 {
            world.step(DT);
            let at = pos(&world, &who);
            let on_floor = world.character_status(&who).unwrap().on_floor;
            if on_floor {
                seen_floor = true;
                low = low.min(at[1]);
            } else if seen_floor && at[1] > low + 0.25 {
                hopped = true;
            }
        }
        assert!(seen_floor, "should land");
        hopped
    };
    assert!(hopped(0.3), "a press just before landing should jump");
    assert!(!hopped(0.0), "a zero buffer should forget the press");
}

#[test]
fn water_and_climb_areas_switch_the_mode_and_fly_stays_fly() {
    let mut world = bare();
    box_at(&mut world, [0.0, -0.5], [8.0, 1.0]);
    let who = character(&mut world, [0.0, 2.0]);
    world.play();
    settle(&mut world, &who);
    let water = world.add("water_area");
    let at = pos(&world, &who);
    world.entity_mut(&water).unwrap().translation = [at[0], at[1], 0.0];
    world.entity_mut(&who).unwrap().character2d.mode = "fly".into();
    world.character_move(&who, [0.0, 1.0]);
    world.step(DT);
    assert_eq!(world.character_status(&who).unwrap().mode, "fly");
    let y_fly = pos(&world, &who)[1];
    world.character_move(&who, [0.0, 1.0]);
    world.step(DT);
    assert!(pos(&world, &who)[1] > y_fly);

    world.entity_mut(&who).unwrap().character2d.mode = "walk".into();
    world.step(DT);
    assert_eq!(world.character_status(&who).unwrap().mode, "swim");
    let y0 = pos(&world, &who)[1];
    step_n(&mut world, &who, 20, [0.0, 1.0]);
    assert!(pos(&world, &who)[1] > y0 + 0.4, "swim should rise");
    world.entity_mut(&water).unwrap().translation[1] = 80.0;
    world.step(DT);
    assert_eq!(world.character_status(&who).unwrap().mode, "walk");

    let climb = world.add("empty");
    let at = pos(&world, &who);
    {
        let e = world.entity_mut(&climb).unwrap();
        e.tag = "climb".into();
        e.scale = [6.0, 6.0, 1.0];
        e.translation = [at[0], at[1], 0.0];
    }
    world.step(DT);
    assert_eq!(world.character_status(&who).unwrap().mode, "climb");
    world.entity_mut(&climb).unwrap().translation[1] = 80.0;
    world.step(DT);
    assert_eq!(world.character_status(&who).unwrap().mode, "walk");
}

#[test]
fn the_same_input_script_matches_bit_for_bit() {
    let run = || {
        let mut world = bare();
        box_at(&mut world, [0.0, -0.5], [16.0, 1.0]);
        let who = character(&mut world, [0.0, 2.0]);
        world.play();
        for frame in 0..80 {
            let wish = if frame < 40 { [1.0, 0.0] } else { [-1.0, 0.0] };
            world.character_move(&who, wish);
            if frame == 25 {
                world.character_jump(&who);
            }
            world.step(DT);
        }
        let at = pos(&world, &who);
        let vel = world.character_status(&who).unwrap().velocity;
        (at, vel)
    };
    let a = run();
    let b = run();
    assert_eq!(a.0[0].to_bits(), b.0[0].to_bits());
    assert_eq!(a.0[1].to_bits(), b.0[1].to_bits());
    assert_eq!(a.1[0].to_bits(), b.1[0].to_bits());
    assert_eq!(a.1[1].to_bits(), b.1[1].to_bits());
    assert!(a.0[0].abs() > 0.15, "the script should move the body, at {:?}", a.0);
}

#[test]
fn an_object_without_the_component_is_not_moved() {
    let mut world = bare();
    let id = world.add("empty");
    let before = pos(&world, &id);
    world.play();
    world.character_move(&id, [1.0, 0.0]);
    world.character_jump(&id);
    for _ in 0..10 {
        world.step(DT);
    }
    assert_eq!(pos(&world, &id), before);
    assert!(world.character_status(&id).is_none());
}

#[test]
fn the_component_round_trips_and_a_damaged_value_is_refused() {
    let demo = World::demo().to_text();
    assert!(!demo.contains("character2d"), "{demo}");

    let mut world = bare();
    let id = character(&mut world, [1.0, 2.0]);
    {
        let c = &mut world.entity_mut(&id).unwrap().character2d;
        c.speed = 7.25;
        c.mode = "fly".into();
        c.jump_buffer = 0.2;
        c.crouch = true;
    }
    let text = world.to_text();
    assert!(text.contains("character2d"), "{text}");
    let back = World::from_text(&text).unwrap();
    let c = &back.entity(&id).unwrap().character2d;
    assert!(c.on);
    assert_eq!(c.mode, "fly");
    assert_eq!(c.speed, 7.25);
    assert_eq!(c.jump_buffer, 0.2);
    assert!(c.crouch);

    let damaged = text.replacen("speed = 7.25", "speed = \"nope\"", 1);
    assert_ne!(damaged, text, "the saved speed should be a bare number:\n{text}");
    let err = match World::from_text(&damaged) {
        Err(err) => err,
        Ok(_) => panic!("a damaged speed was accepted"),
    };
    assert!(err.contains("speed"), "{err}");

    let legacy = world.to_legacy_text();
    assert!(legacy.contains("char_speed"), "{legacy}");
    let broken = legacy.replacen("char_speed = \"7.25\"", "char_speed = \"nope\"", 1);
    assert_ne!(broken, legacy, "{legacy}");
    let err = match World::from_text(&broken) {
        Err(err) => err,
        Ok(_) => panic!("a damaged legacy field was accepted"),
    };
    assert!(err.contains("damaged"), "{err}");
}

const CHARACTER_GRAPH: &str = r#"
schema_version = 1
seal_graph = "character"
vars = []
nodes = [
  { id = 1, kind = "event", name = "start" },
  { id = 2, kind = "event", name = "tick" },
  { id = 3, kind = "self" },
  { id = 4, kind = "move_character", defaults = { wish = [1.0, 0.0] } },
  { id = 5, kind = "jump" },
  { id = 6, kind = "set_movement_mode", defaults = { mode = "walk" } },
  { id = 7, kind = "is_on_floor" },
  { id = 8, kind = "is_on_wall" },
  { id = 9, kind = "get_velocity" },
  { id = 10, kind = "to_str", type = "vec2" },
  { id = 11, kind = "log" },
]
links = [
  "1.then -> 6.in",
  "6.then -> 5.in",
  "5.then -> 11.in",
  "3.out -> 6.entity",
  "3.out -> 5.entity",
  "3.out -> 4.entity",
  "3.out -> 7.entity",
  "3.out -> 8.entity",
  "3.out -> 9.entity",
  "9.out -> 10.a",
  "10.out -> 11.message",
  "2.then -> 4.in",
]
"#;

#[test]
fn a_graph_can_move_the_character() {
    let dir = std::env::temp_dir().join(format!("seal-character2d-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("logic")).unwrap();
    std::fs::write(dir.join("logic/character.graph"), CHARACTER_GRAPH).unwrap();
    let mut world = bare();
    box_at(&mut world, [0.0, -0.5], [16.0, 1.0]);
    let who = character(&mut world, [0.0, 2.0]);
    world.entity_mut(&who).unwrap().script = "logic/character.graph".into();
    world.project_dir = Some(dir.clone());
    world.play();
    assert!(
        world.play_log.iter().all(|line| !line.starts_with("script on")),
        "{:?}",
        world.play_log
    );
    let x0 = pos(&world, &who)[0];
    for _ in 0..40 {
        assert!(world.step(DT));
    }
    assert!(
        world.play_log.iter().all(|line| !line.starts_with("script on")),
        "{:?}",
        world.play_log
    );
    assert!(pos(&world, &who)[0] > x0 + 0.5, "graph move should slide the body");
    std::fs::remove_dir_all(dir).ok();
}
