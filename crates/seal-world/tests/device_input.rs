//! Light bar and motion nodes call the gamepad backend.

use seal_graph::rt::{Host, V2, V3};
use seal_graph::{Graph, GraphError, GraphInstance, Interp, Node, Program, RunState, Ty, Value};
use seal_input::{deliver_light, PadBackend, PadMotion};
use seal_sealtext::{SealMap, SealValue};

fn text(s: &str) -> SealValue {
    SealValue::String(s.into())
}

fn number(s: &str) -> SealValue {
    SealValue::Number(s.into())
}

fn defaults(pairs: &[(&str, SealValue)]) -> SealValue {
    SealValue::Map(SealMap::new(pairs.iter().map(|(k, v)| ((*k).into(), v.clone())).collect()))
}

struct Mock {
    light: Option<[f32; 3]>,
    gyro: [f32; 3],
    accel: [f32; 3],
    fail: bool,
}

impl PadBackend for Mock {
    fn set_light_bar(&mut self, rgb: [f32; 3]) -> Result<(), String> {
        if self.fail {
            return Err("the gamepad light bar is not supported by this gamepad backend".into());
        }
        self.light = Some(rgb);
        Ok(())
    }
    fn motion(&self) -> PadMotion {
        if self.fail {
            PadMotion::unsupported("motion sensors are not supported by this gamepad backend")
        } else {
            PadMotion::ready(self.gyro, self.accel)
        }
    }
    fn light_note(&self) -> String {
        if self.fail {
            "the gamepad light bar is not supported by this gamepad backend".into()
        } else {
            String::new()
        }
    }
}

struct Rec {
    pad: Mock,
    log: Vec<String>,
}

impl Host for Rec {
    fn get(&mut self, _: u64, _: &str, _: &str, _: Ty) -> Result<Value, String> {
        Err("no object".into())
    }
    fn set(&mut self, _: u64, _: &str, _: &str, _: Value) -> Result<(), String> {
        Ok(())
    }
    fn log(&mut self, message: &str) {
        self.log.push(message.to_string());
    }
    fn set_pad_light(&mut self, red: f64, green: f64, blue: f64) -> Result<(), String> {
        deliver_light(&mut self.pad, [red as f32, green as f32, blue as f32])
    }
    fn pad_gyro(&mut self) -> V3 {
        let motion = self.pad.motion();
        V3 {
            x: f64::from(motion.gyro[0]),
            y: f64::from(motion.gyro[1]),
            z: f64::from(motion.gyro[2]),
        }
    }
    fn pad_accel(&mut self) -> V3 {
        let motion = self.pad.motion();
        V3 {
            x: f64::from(motion.accel[0]),
            y: f64::from(motion.accel[1]),
            z: f64::from(motion.accel[2]),
        }
    }
    fn pad_motion_note(&mut self) -> String {
        self.pad.motion().unsupported
    }
    fn raw_mouse_delta(&mut self) -> V2 {
        V2 { x: 4.0, y: -2.0 }
    }
    fn mouse_delta(&mut self) -> V2 {
        V2 { x: 1.0, y: 1.0 }
    }
    fn mouse_look(&mut self) -> V2 {
        V2 { x: 4.0, y: -2.0 }
    }
}

fn run(graph: &Graph, host: &mut Rec) -> Result<RunState, GraphError> {
    let program = Program::build(graph).unwrap_or_else(|e| panic!("{e:?}"));
    let mut interp = Interp::new(program, 0);
    interp.fire("start", &[], host)
}

#[test]
fn light_and_sensor_nodes_reach_the_backend() {
    let mut host = Rec {
        pad: Mock { light: None, gyro: [0.0, 2.0, 0.0], accel: [0.0, 1.0, 0.0], fail: false },
        log: Vec::new(),
    };
    let light = Graph::new("light")
        .node(Node::new(1, "event").with("name", text("start")))
        .node(Node::new(2, "set_pad_light").with(
            "defaults",
            defaults(&[("red", number("0.25")), ("green", number("0.5")), ("blue", number("1"))]),
        ))
        .link(1, "then", 2, "in");
    run(&light, &mut host).unwrap();
    let colour = host.pad.light.unwrap();
    assert!((colour[0] - 0.25).abs() < 1e-4);
    assert!((colour[1] - 0.5).abs() < 1e-4);
    assert!((colour[2] - 1.0).abs() < 1e-4);

    let sensors = Graph::new("sensors")
        .node(Node::new(1, "event").with("name", text("start")))
        .node(Node::new(2, "pad_gyro"))
        .node(Node::new(3, "vec_text").with("type", text("vec3")))
        .node(Node::new(4, "log"))
        .node(Node::new(5, "pad_motion_note"))
        .node(Node::new(6, "log"))
        .link(1, "then", 4, "in")
        .link(2, "out", 3, "a")
        .link(3, "out", 4, "message")
        .link(4, "then", 6, "in")
        .link(5, "out", 6, "message");
    run(&sensors, &mut host).unwrap();
    assert_eq!(host.log[0], "X=0.000 Y=2.000 Z=0.000");
    assert!(host.log[1].is_empty());

    host.pad.fail = true;
    host.log.clear();
    let err = run(&light, &mut host).unwrap_err();
    assert!(err.to_string().contains("not supported"), "{err}");
    run(&sensors, &mut host).unwrap();
    assert!(host.log.last().unwrap().contains("not supported"));
}

#[test]
fn raw_mouse_node_is_not_the_cursor() {
    let mut host = Rec {
        pad: Mock { light: None, gyro: [0.0; 3], accel: [0.0; 3], fail: false },
        log: Vec::new(),
    };
    let graph = Graph::new("look")
        .node(Node::new(1, "event").with("name", text("start")))
        .node(Node::new(2, "raw_mouse_delta"))
        .node(Node::new(3, "vec_text").with("type", text("vec2")))
        .node(Node::new(4, "log"))
        .node(Node::new(5, "mouse_delta"))
        .node(Node::new(6, "vec_text").with("type", text("vec2")))
        .node(Node::new(7, "log"))
        .link(1, "then", 4, "in")
        .link(2, "out", 3, "a")
        .link(3, "out", 4, "message")
        .link(4, "then", 7, "in")
        .link(5, "out", 6, "a")
        .link(6, "out", 7, "message");
    run(&graph, &mut host).unwrap();
    assert_ne!(host.log[0], host.log[1]);
    assert!(host.log[0].contains("4.000"));
    assert!(host.log[1].contains("1.000"));
}
