use seal_world::World;

#[test]
fn unscaled_time_advances_while_scaled_motion_is_frozen() {
    let mut w = World::demo();
    w.time_scale = 0.0;
    w.play();
    w.step(0.1);
    assert!(w.unscaled_time > 0.0);
    assert_eq!(w.sim_tick, 1);
    w.pause();
    w.step(0.05);
    assert!(w.unscaled_time > 0.1);
    assert_eq!(w.sim_tick, 2);
}
