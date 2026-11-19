use seal_world::World;

#[test]
fn copies_number_after_the_highest_name() {
    let mut world = World::demo();
    let a = world.add("cube");
    world.entity_mut(&a).unwrap().name = "Gem".into();
    assert_eq!(world.free_name("Gem"), "Gem 2");
    let b = world.add("cube");
    world.entity_mut(&b).unwrap().name = "Gem 3".into();
    assert_eq!(world.free_name("Gem 3"), "Gem 4");
    assert_eq!(world.free_name("Gem"), "Gem 4");
    assert_eq!(world.free_name("Ruby 2"), "Ruby 2");
}
