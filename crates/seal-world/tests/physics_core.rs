//! Multiple physics worlds, the handle server, backend swap, and 3D shapes.

use seal_graph::{Graph, GraphInstance, Host, Interp, Node, Program, Value};
use seal_sealtext::{SealMap, SealValue};
use seal_world::physics::{
    self, mesh_from_text, picture_from_bytes, BodyDesc, Geom, JointDesc, JointKind, PhysicsServer,
};
use seal_world::World;

fn num(text: &str) -> SealValue {
    SealValue::Number(text.into())
}

fn text(value: &str) -> SealValue {
    SealValue::String(value.into())
}

fn defaults(pairs: Vec<(String, SealValue)>) -> SealValue {
    SealValue::Map(SealMap::new(pairs))
}

struct PhysHost {
    server: PhysicsServer,
}

impl Host for PhysHost {
    fn get(&mut self, _: u64, _: &str, _: &str, _: seal_graph::Ty) -> Result<Value, String> {
        Err("no fields".into())
    }
    fn set(&mut self, _: u64, _: &str, _: &str, _: Value) -> Result<(), String> {
        Ok(())
    }
    fn log(&mut self, _: &str) {}
    fn physics(&mut self, op: &str, args: &[Value]) -> Result<Value, String> {
        physics::apply_call(&mut self.server, None, op, args)
    }
}

#[test]
fn two_worlds_step_independently() {
    let mut server = PhysicsServer::new();
    server.create_world("low", [0.0, -10.0, 0.0], "builtin", true).unwrap();
    server.create_world("still", [0.0, 0.0, 0.0], "builtin", true).unwrap();
    let falling = server
        .create_body("low", BodyDesc { position: [0.0, 5.0, 0.0], ..BodyDesc::default() })
        .unwrap();
    let resting = server
        .create_body("still", BodyDesc { position: [0.0, 5.0, 0.0], ..BodyDesc::default() })
        .unwrap();
    server.step_world("low", 0.5).unwrap();
    assert!(server.translation("low", falling).unwrap()[1] < 5.0);
    assert_eq!(server.translation("still", resting).unwrap()[1], 5.0);
    server.step_world("still", 0.5).unwrap();
    assert_eq!(server.translation("still", resting).unwrap()[1], 5.0);
}

#[test]
fn server_body_falls_under_gravity() {
    let mut server = PhysicsServer::new();
    server.create_world("main", [0.0, -9.81, 0.0], "builtin", false).unwrap();
    let body = server
        .create_body("main", BodyDesc { position: [0.0, 8.0, 0.0], ..BodyDesc::default() })
        .unwrap();
    server.step_world("main", 1.0).unwrap();
    let y = server.translation("main", body).unwrap()[1];
    assert!(y < 4.0, "y was {y}");
}

#[test]
fn backend_swap_changes_the_step() {
    let mut builtin = physics::open_backend("builtin").unwrap();
    builtin.set_gravity([0.0, -10.0, 0.0]);
    let body = builtin.create_body(BodyDesc { position: [0.0, 3.0, 0.0], ..BodyDesc::default() });
    builtin.step(0.5);
    assert!(builtin.translation(body).unwrap()[1] < 3.0);

    let mut server = PhysicsServer::new();
    server.create_world("side", [0.0, -10.0, 0.0], "builtin", false).unwrap();
    server.set_backend("side", "drift").unwrap();
    let slid = server
        .create_body("side", BodyDesc { position: [0.0, 3.0, 0.0], ..BodyDesc::default() })
        .unwrap();
    server.step_world("side", 1.0).unwrap();
    let at = server.translation("side", slid).unwrap();
    assert!(at[0] > 1.0, "x was {}", at[0]);
    assert!((at[1] - 3.0).abs() < 1e-3, "drift should not fall, y was {}", at[1]);
}

#[test]
fn ray_and_overlap_hit_cylinder_trimesh_and_heightfield() {
    let cylinder = Geom::Cylinder { radius: 0.5, half_height: 1.0 };
    let hit = physics::ray(&cylinder, [0.0; 3], [0.0, 0.0, -3.0], [0.0, 0.0, 1.0], 10.0);
    assert!(hit.unwrap() < 3.0);
    assert!(physics::ray(&cylinder, [0.0; 3], [4.0, 0.0, -3.0], [0.0, 0.0, 1.0], 10.0).is_none());
    assert!(physics::overlap_sphere(&cylinder, [0.0; 3], [0.0, 0.0, 0.0], 0.2));
    assert!(!physics::overlap_sphere(&cylinder, [0.0; 3], [3.0, 0.0, 0.0], 0.2));

    let mesh = mesh_from_text("v 0 0 0\nv 2 0 0\nv 0 0 2\nf 0 1 2\n").unwrap();
    let trimesh = Geom::Mesh { vertices: mesh.vertices, indices: mesh.indices };
    let hit = physics::ray(&trimesh, [0.0; 3], [0.2, 3.0, 0.2], [0.0, -1.0, 0.0], 10.0);
    assert!(hit.is_some());
    assert!(physics::overlap_sphere(&trimesh, [0.0; 3], [0.2, 0.1, 0.2], 0.3));
    assert!(!physics::overlap_sphere(&trimesh, [0.0; 3], [0.2, 4.0, 0.2], 0.2));

    let field = physics::geom_from_args("heightfield", 4.0, 2.0, 4.0, "grid 2 2\n0 0\n1 1\n", None)
        .unwrap();
    let hit = physics::ray(&field, [0.0; 3], [0.0, 5.0, 0.0], [0.0, -1.0, 0.0], 20.0);
    assert!(hit.is_some());
    assert!(physics::overlap_sphere(&field, [0.0; 3], [0.0, 1.0, 0.0], 1.2));
    assert!(!physics::overlap_sphere(&field, [0.0; 3], [0.0, 8.0, 0.0], 0.2));
}

#[test]
fn decomposition_hulls_are_convex_and_cover_the_mesh() {
    let (vertices, indices) = two_boxes();
    let hulls = physics::decompose(&vertices, &indices, 2);
    assert!(hulls.len() >= 2, "got {} hulls", hulls.len());
    for hull in &hulls {
        assert!(hull_is_convex(hull), "a hull left a point outside itself");
    }
    for vertex in &vertices {
        assert!(
            hulls.iter().any(|hull| hull.contains(*vertex)),
            "vertex {vertex:?} is outside every hull"
        );
    }
}

#[test]
fn damaged_mesh_and_picture_are_refused() {
    assert!(mesh_from_text("v 1 2\n").is_err());
    assert!(mesh_from_text("f 0 1 2\n").is_err());
    assert!(mesh_from_text("nope\n").is_err());
    assert!(picture_from_bytes(b"not a picture").is_err());
    // A 2x1 grayscale png: black then white gives heights 0 and 1.
    let mut png_bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png_bytes, 2, 1);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().unwrap().write_image_data(&[0, 255]).unwrap();
    }
    let pic = picture_from_bytes(&png_bytes).unwrap();
    assert_eq!((pic.width, pic.height), (2, 1));
    assert!(pic.luma[0] < 0.01 && pic.luma[1] > 0.99);
    assert!(picture_from_bytes(&png_bytes[..20]).is_err());
    assert!(picture_from_bytes(b"P6\n2 2\n255\n").is_err());
    assert!(picture_from_bytes(b"grid 2 2\n1\n").is_err());
    assert!(picture_from_bytes(b"BM").is_err());
}

#[test]
fn graph_nodes_drop_a_body_under_gravity() {
    let graph = Graph::new("phys")
        .node(Node::new(1, "event").with("name", text("start")))
        .node(Node::new(2, "physics_body").with(
            "defaults",
            defaults(vec![
                ("world".into(), text("server")),
                ("x".into(), num("0")),
                ("y".into(), num("8")),
                ("z".into(), num("0")),
                ("dynamic".into(), SealValue::Bool(true)),
            ]),
        ))
        .node(Node::new(3, "physics_step").with(
            "defaults",
            defaults(vec![("world".into(), text("server")), ("dt".into(), num("1"))]),
        ))
        .node(
            Node::new(4, "physics_position")
                .with("defaults", defaults(vec![("world".into(), text("server"))])),
        )
        .link(1, "then", 2, "in")
        .link(2, "then", 3, "in")
        .link(2, "body", 4, "body");
    let program = Program::build(&graph).unwrap_or_else(|err| panic!("{err:?}"));
    let mut host = PhysHost { server: PhysicsServer::new() };
    let mut interp = Interp::new(program, 1);
    interp.fire("start", &[], &mut host).unwrap();
    let index = interp
        .program()
        .nodes
        .iter()
        .position(|n| matches!(n.op, seal_graph::Op::Server(seal_graph::ServerCall::Position)))
        .unwrap();
    let at = interp.eval_pure(index as u32, &mut host).unwrap().vec3();
    assert!(at.y < 4.0, "y was {}", at.y);
}

#[test]
fn a_fixed_joint_keeps_the_offset() {
    let mut server = PhysicsServer::new();
    server.create_world("main", [0.0, -10.0, 0.0], "builtin", false).unwrap();
    let anchor = server
        .create_body(
            "main",
            BodyDesc { position: [0.0, 0.0, 0.0], dynamic: false, ..BodyDesc::default() },
        )
        .unwrap();
    let hung = server
        .create_body("main", BodyDesc { position: [0.0, 2.0, 0.0], ..BodyDesc::default() })
        .unwrap();
    server
        .create_joint(
            "main",
            JointDesc { kind: JointKind::Fixed, body_a: anchor, body_b: hung, length: 0.0 },
        )
        .unwrap();
    server.step_world("main", 0.5).unwrap();
    let at = server.translation("main", hung).unwrap();
    assert!((at[1] - 2.0).abs() < 0.05, "y was {}", at[1]);
}

#[test]
fn scene_worlds_round_trip_and_step_apart() {
    assert!(!World::demo().to_text().contains("physics_scene"));
    let mut world = World::demo();
    let high = world.add("cube");
    let held = world.add("cube");
    world.entity_mut(&high).unwrap().dynamic = true;
    world.entity_mut(&high).unwrap().translation = [0.0, 6.0, 0.0];
    world.entity_mut(&held).unwrap().dynamic = true;
    world.entity_mut(&held).unwrap().translation = [3.0, 6.0, 0.0];
    world.physics.scene.add_world(0.0);
    world.physics.scene.set_member(&high, "main");
    world.physics.scene.set_member(&held, "preview");
    let loaded = World::from_text(&world.to_text()).unwrap();
    assert_eq!(loaded.physics.scene.member(&high).unwrap(), "main");
    assert_eq!(loaded.physics.scene.member(&held).unwrap(), "preview");
    let mut world = loaded;
    world.play();
    world.step(0.5);
    assert!(world.entity(&high).unwrap().translation[1] < 6.0);
    assert!((world.entity(&held).unwrap().translation[1] - 6.0).abs() < 1e-2);
}

fn hull_is_convex(hull: &physics::Hull) -> bool {
    if hull.flat {
        return true;
    }
    hull.points.iter().all(|point| hull.contains(*point))
}

fn two_boxes() -> (Vec<[f32; 3]>, Vec<[u32; 3]>) {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for origin in [[0.0, 0.0, 0.0], [6.0, 0.0, 0.0]] {
        let base = vertices.len() as u32;
        for z in [0.0, 1.0] {
            for y in [0.0, 1.0] {
                for x in [0.0, 1.0] {
                    vertices.push([origin[0] + x, origin[1] + y, origin[2] + z]);
                }
            }
        }
        let corner = [
            [0, 1, 3],
            [0, 3, 2],
            [4, 6, 7],
            [4, 7, 5],
            [0, 4, 5],
            [0, 5, 1],
            [2, 3, 7],
            [2, 7, 6],
            [0, 2, 6],
            [0, 6, 4],
            [1, 5, 7],
            [1, 7, 3],
        ];
        for tri in corner {
            indices.push([base + tri[0], base + tri[1], base + tri[2]]);
        }
    }
    (vertices, indices)
}
