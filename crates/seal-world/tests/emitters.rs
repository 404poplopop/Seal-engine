use seal_world::particles::EmitterDef;
use seal_world::World;

#[test]
fn an_emitter_follows_its_object_during_play_and_is_cleared_on_stop() {
    let mut w = World::demo();
    let id = w.entities[1].id.clone();
    w.play();
    w.play_emitter(
        &id,
        EmitterDef { rate: 60.0, velocity: [0.0; 3], lifetime: 5.0, ..Default::default() },
    );
    w.entities[1].translation = [4.0, 0.0, 0.0];
    for _ in 0..30 {
        w.step(1.0 / 60.0);
    }
    let (_, em) = &w.emitters[0];
    assert!(em.particles.len() >= 20, "{}", em.particles.len());
    assert!(em.particles.iter().all(|p| p.pos[0] == 4.0));
    w.stop();
    assert!(w.emitters.is_empty());
}

#[test]
fn an_effect_file_parses_and_reports_bad_lines() {
    let def = EmitterDef::parse("# trail\nrate 30\nlifetime 2\nvelocity 0 3 0\nsize 0.4 0.1\ncolor 1 1 0 1  1 0 0 0\nmax 50\n").unwrap();
    assert_eq!(def.rate, 30.0);
    assert_eq!(def.velocity, [0.0, 3.0, 0.0]);
    assert_eq!(def.size, (0.4, 0.1));
    assert_eq!(def.color.1, [1.0, 0.0, 0.0, 0.0]);
    assert_eq!(def.max, 50);
    assert_eq!(EmitterDef::parse("velocity 1 2").unwrap_err(), "line 1: velocity takes 3 numbers");
    assert!(EmitterDef::parse("wobble 3").unwrap_err().contains("unknown key wobble"));
}

#[test]
fn an_object_with_an_effect_saves_it_and_emits_when_play_starts() {
    let dir = std::env::temp_dir().join(format!("seal-fx-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("fx")).unwrap();
    std::fs::write(dir.join("fx/puff.fx"), "rate 60\nlifetime 3\n").unwrap();
    let mut w = World::demo();
    w.entities[1].particles = "fx/puff.fx".into();
    let again = World::from_text(&w.to_text()).unwrap();
    assert_eq!(again.entities[1].particles, "fx/puff.fx", "the effect survives save and load");

    let mut w = again;
    w.project_dir = Some(dir.clone());
    w.play();
    for _ in 0..30 {
        w.step(1.0 / 60.0);
    }
    assert_eq!(w.emitters.len(), 1);
    assert!(w.emitters[0].1.particles.len() >= 20);
    w.stop();
    assert!(w.emitters.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_missing_effect_file_is_reported_in_the_play_log() {
    let dir = std::env::temp_dir().join(format!("seal-fx-missing-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut w = World::demo();
    w.entities[1].particles = "fx/none.fx".into();
    w.project_dir = Some(dir.clone());
    w.play();
    assert!(w.emitters.is_empty());
    assert!(w.play_log.iter().any(|l| l.starts_with("particles fx/none.fx:")), "{:?}", w.play_log);
    let _ = std::fs::remove_dir_all(&dir);
}
