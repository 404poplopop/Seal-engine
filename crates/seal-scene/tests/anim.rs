use seal_scene::anim::Anim;
const TEXT: &str = "length 2\nloop true\ntrack position.x linear\nkey 0 0\nkey 1 4\ntrack sprite_frame step\nkey 0 0\nkey 1 3\ntrack color.r cubic\nkey 0 0\nkey 2 1\n";
fn get(a: &Anim, t: f32, n: &str) -> f32 {
    a.sample(t).into_iter().find(|x| x.0 == n).unwrap().1
}
#[test]
fn linear_step_cubic_sample() {
    let a = Anim::parse(TEXT).unwrap();
    assert_eq!(get(&a, 0.5, "position.x"), 2.0);
    assert_eq!(get(&a, 0.99, "sprite_frame"), 0.0);
    assert_eq!(get(&a, 1.0, "sprite_frame"), 3.0);
    assert_eq!(get(&a, 1.0, "color.r"), 0.5);
    assert_eq!(get(&a, 9.0, "position.x"), 4.0);
}
#[test]
fn loops_and_finishes() {
    let a = Anim::parse(TEXT).unwrap();
    assert_eq!(get(&a, 2.5, "position.x"), 2.0);
    assert!(!a.finished(5.0));
    let b = Anim::parse("length 1\ntrack a linear\nkey 0 0\nkey 1 1\n").unwrap();
    assert!(b.finished(1.0));
    assert_eq!(get(&b, 7.0, "a"), 1.0);
}
#[test]
fn bad_input_names_the_line() {
    assert!(Anim::parse("key 0 1").unwrap_err().contains("line 1"));
    assert!(Anim::parse("track a linear\nkey 1 0\nkey 1 2").unwrap_err().contains("line 3"));
    assert!(Anim::parse("track a wavy").is_err());
}
