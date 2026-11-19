//! Record the inputs a graph receives and play them back. A replay feeds the
//! same events and time steps in the same order, so the run is bit-identical,
//! and a state hash after every step proves it.

use crate::rt::{GraphError, GraphInstance, MapHost, RunState, Ty, Value};
use crate::{value_from_text, value_to_text};
use seal_sealtext::{SealMap, SealValue};

/// One input to a graph.
#[derive(Clone, Debug, PartialEq)]
pub enum Input {
    Fire { event: String, args: Vec<Value> },
    Advance(f64),
}

/// The inputs of a run, in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Recording {
    pub graph: String,
    pub inputs: Vec<Input>,
    /// The entity each player controls, in player order. Playback hands
    /// these to the host, so a run that asks for a player sees the same one.
    pub players: Vec<u64>,
}

const SCHEMA: i64 = 1;

impl Recording {
    pub fn new(graph: &str) -> Self {
        Self { graph: graph.into(), inputs: Vec::new(), players: Vec::new() }
    }

    /// Record which entities the players control. Call before the first
    /// input; the host gets the same list during playback.
    pub fn with_players(mut self, players: &[u64]) -> Self {
        self.players = players.to_vec();
        self
    }

    /// Forward an event to the graph and record it.
    pub fn fire(
        &mut self,
        g: &mut dyn GraphInstance,
        event: &str,
        args: &[Value],
        host: &mut MapHost,
    ) -> Result<RunState, GraphError> {
        self.inputs.push(Input::Fire { event: event.into(), args: args.to_vec() });
        g.fire(event, args, host)
    }

    /// Advance the graph and record the time step.
    pub fn advance(
        &mut self,
        g: &mut dyn GraphInstance,
        dt: f64,
        host: &mut MapHost,
    ) -> Result<RunState, GraphError> {
        self.inputs.push(Input::Advance(dt));
        g.advance(dt, host)
    }

    /// SealText. Time steps are stored as exact bit patterns so playback
    /// never depends on how a float prints.
    pub fn to_text(&self) -> String {
        let inputs = self
            .inputs
            .iter()
            .map(|i| match i {
                Input::Advance(dt) => SealValue::Map(SealMap::new(vec![(
                    "advance".into(),
                    SealValue::String(format!("{:016x}", dt.to_bits())),
                )])),
                Input::Fire { event, args } => SealValue::Map(SealMap::new(vec![
                    ("fire".into(), SealValue::String(event.clone())),
                    ("args".into(), SealValue::Array(args.iter().map(arg_to_text).collect())),
                ])),
            })
            .collect();
        let mut root = vec![
            ("schema_version".into(), SealValue::Number(SCHEMA.to_string())),
            ("seal_replay".into(), SealValue::String(self.graph.clone())),
        ];
        // Left out when empty, so replays without players read as before.
        if !self.players.is_empty() {
            let ids = self.players.iter().map(|p| SealValue::String(format!("{p:016x}"))).collect();
            root.push(("players".into(), SealValue::Array(ids)));
        }
        root.push(("inputs".into(), SealValue::Array(inputs)));
        SealValue::Map(SealMap::new(root)).to_text()
    }

    pub fn from_text(text: &str) -> Result<Self, String> {
        let root = seal_sealtext::parse(text).map_err(|e| e.to_string())?;
        match root.get("schema_version") {
            Some(SealValue::Number(n)) if n.parse::<i64>().ok() == Some(SCHEMA) => {}
            Some(SealValue::Number(n)) => {
                return Err(format!("replay schema {n} is not supported"))
            }
            _ => return Err("not a replay: no schema_version".into()),
        }
        let Some(SealValue::String(graph)) = root.get("seal_replay") else {
            return Err("not a replay: no seal_replay".into());
        };
        let Some(SealValue::Array(items)) = root.get("inputs") else {
            return Err("replay has no inputs".into());
        };
        let players = match root.get("players") {
            None => Vec::new(),
            Some(SealValue::Array(a)) => a
                .iter()
                .map(|v| match v {
                    SealValue::String(s) => u64::from_str_radix(s, 16).ok(),
                    _ => None,
                })
                .collect::<Option<Vec<u64>>>()
                .ok_or("players must be entity ids")?,
            Some(_) => return Err("players must be a list".into()),
        };
        let mut inputs = Vec::new();
        for (k, item) in items.iter().enumerate() {
            let bad = |what: &str| format!("input {k}: {what}");
            if let Some(SealValue::String(bits)) = item.get("advance") {
                let bits = u64::from_str_radix(bits, 16).map_err(|_| bad("bad time step"))?;
                inputs.push(Input::Advance(f64::from_bits(bits)));
            } else if let Some(SealValue::String(event)) = item.get("fire") {
                let args = match item.get("args") {
                    Some(SealValue::Array(a)) => a
                        .iter()
                        .map(|v| arg_from_text(v).ok_or_else(|| bad("bad argument")))
                        .collect::<Result<_, _>>()?,
                    None => Vec::new(),
                    _ => return Err(bad("args must be a list")),
                };
                inputs.push(Input::Fire { event: event.clone(), args });
            } else {
                return Err(bad("expected advance or fire"));
            }
        }
        Ok(Self { graph: graph.clone(), inputs, players })
    }

    /// Feed every input to `g` and return the state hash after each one.
    /// An error is mixed into that step's hash, and playback goes on past
    /// it, the same way the recorded run did.
    pub fn play(&self, g: &mut dyn GraphInstance, host: &mut MapHost) -> Vec<u64> {
        if !self.players.is_empty() {
            host.players = self.players.clone();
            host.alive.extend(self.players.iter().copied());
        }
        self.inputs
            .iter()
            .map(|i| {
                let r = match i {
                    Input::Fire { event, args } => g.fire(event, args, host),
                    Input::Advance(dt) => g.advance(*dt, host),
                };
                let mut h = state_hash(g, host);
                if let Err(e) = r {
                    h = mix(h, &e.to_string().into_bytes());
                }
                h
            })
            .collect()
    }
}

/// Arguments are written as `["int", 3]` so they load without the graph.
fn arg_to_text(v: &Value) -> SealValue {
    let ty = match v {
        Value::Bool(_) => Ty::Bool,
        Value::Int(_) => Ty::Int,
        Value::Float(_) => Ty::Float,
        Value::Vec2(_) => Ty::Vec2,
        Value::Vec3(_) => Ty::Vec3,
        Value::Quat(_) => Ty::Quat,
        Value::Plane(_) => Ty::Plane,
        Value::Str(_) => Ty::Str,
        Value::List(e, items) => {
            return SealValue::Array(vec![
                SealValue::String(Ty::List(*e).name().into()),
                SealValue::Array(items.iter().map(arg_to_text).collect()),
            ])
        }
        Value::Entity(e) => {
            return SealValue::Array(vec![
                SealValue::String("entity".into()),
                SealValue::String(format!("{e:016x}")),
            ])
        }
    };
    let body = match v {
        Value::Float(f) => SealValue::String(format!("{:016x}", f.to_bits())),
        Value::Vec2(p) => SealValue::Array(vec![
            SealValue::String(format!("{:016x}", p.x.to_bits())),
            SealValue::String(format!("{:016x}", p.y.to_bits())),
        ]),
        Value::Vec3(p) => SealValue::Array(vec![
            SealValue::String(format!("{:016x}", p.x.to_bits())),
            SealValue::String(format!("{:016x}", p.y.to_bits())),
            SealValue::String(format!("{:016x}", p.z.to_bits())),
        ]),
        Value::Quat(p) => SealValue::Array(vec![
            SealValue::String(format!("{:016x}", p.x.to_bits())),
            SealValue::String(format!("{:016x}", p.y.to_bits())),
            SealValue::String(format!("{:016x}", p.z.to_bits())),
            SealValue::String(format!("{:016x}", p.w.to_bits())),
        ]),
        Value::Plane(p) => SealValue::Array(vec![
            SealValue::String(format!("{:016x}", p.nx.to_bits())),
            SealValue::String(format!("{:016x}", p.ny.to_bits())),
            SealValue::String(format!("{:016x}", p.nz.to_bits())),
            SealValue::String(format!("{:016x}", p.d.to_bits())),
        ]),
        other => value_to_text(other),
    };
    SealValue::Array(vec![SealValue::String(ty.name().into()), body])
}

fn bits(v: &SealValue) -> Option<f64> {
    match v {
        SealValue::String(s) => u64::from_str_radix(s, 16).ok().map(f64::from_bits),
        _ => None,
    }
}

fn arg_from_text(v: &SealValue) -> Option<Value> {
    let SealValue::Array(pair) = v else { return None };
    let [SealValue::String(ty), body] = pair.as_slice() else { return None };
    Some(match ty.as_str() {
        "entity" => match body {
            SealValue::String(s) => Value::Entity(u64::from_str_radix(s, 16).ok()?),
            _ => return None,
        },
        "float" => Value::Float(bits(body)?),
        "vec2" => match body {
            SealValue::Array(a) if a.len() == 2 => {
                Value::Vec2(crate::rt::V2 { x: bits(&a[0])?, y: bits(&a[1])? })
            }
            _ => return None,
        },
        "vec3" => match body {
            SealValue::Array(a) if a.len() == 3 => {
                Value::Vec3(crate::rt::V3 { x: bits(&a[0])?, y: bits(&a[1])?, z: bits(&a[2])? })
            }
            _ => return None,
        },
        "quat" => match body {
            SealValue::Array(a) if a.len() == 4 => Value::Quat(crate::rt::Quat {
                x: bits(&a[0])?,
                y: bits(&a[1])?,
                z: bits(&a[2])?,
                w: bits(&a[3])?,
            }),
            _ => return None,
        },
        "plane" => match body {
            SealValue::Array(a) if a.len() == 4 => Value::Plane(crate::rt::Plane {
                nx: bits(&a[0])?,
                ny: bits(&a[1])?,
                nz: bits(&a[2])?,
                d: bits(&a[3])?,
            }),
            _ => return None,
        },
        other => match Ty::parse(other)? {
            Ty::List(e) => match body {
                SealValue::Array(a) => {
                    let items: Vec<Value> = a.iter().map(arg_from_text).collect::<Option<_>>()?;
                    if items.iter().any(|v| v.ty() != e.ty()) {
                        return None;
                    }
                    Value::List(e, items)
                }
                _ => return None,
            },
            t => value_from_text(t, body)?,
        },
    })
}

/// FNV-1a, 64 bit: fixed, portable and independent of the standard
/// library's hasher, so hashes match across machines and releases.
fn mix(mut h: u64, bytes: &[u8]) -> u64 {
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn mix_value(h: u64, v: &Value) -> u64 {
    match v {
        Value::Bool(b) => mix(h, &[0, *b as u8]),
        Value::Int(i) => mix(mix(h, &[1]), &i.to_le_bytes()),
        Value::Float(f) => mix(mix(h, &[2]), &f.to_bits().to_le_bytes()),
        Value::Vec2(p) => {
            mix(mix(mix(h, &[3]), &p.x.to_bits().to_le_bytes()), &p.y.to_bits().to_le_bytes())
        }
        Value::Str(s) => mix(mix(mix(h, &[4]), &(s.len() as u64).to_le_bytes()), s.as_bytes()),
        Value::Entity(e) => mix(mix(h, &[5]), &e.to_le_bytes()),
        Value::Vec3(p) => {
            let h =
                mix(mix(mix(h, &[6]), &p.x.to_bits().to_le_bytes()), &p.y.to_bits().to_le_bytes());
            mix(h, &p.z.to_bits().to_le_bytes())
        }
        Value::Quat(p) => {
            let h = mix(
                mix(mix(mix(h, &[8]), &p.x.to_bits().to_le_bytes()), &p.y.to_bits().to_le_bytes()),
                &p.z.to_bits().to_le_bytes(),
            );
            mix(h, &p.w.to_bits().to_le_bytes())
        }
        Value::Plane(p) => {
            let h = mix(
                mix(
                    mix(mix(h, &[9]), &p.nx.to_bits().to_le_bytes()),
                    &p.ny.to_bits().to_le_bytes(),
                ),
                &p.nz.to_bits().to_le_bytes(),
            );
            mix(h, &p.d.to_bits().to_le_bytes())
        }
        Value::List(e, items) => {
            let mut h = mix(mix(h, &[7, *e as u8]), &(items.len() as u64).to_le_bytes());
            for v in items {
                h = mix_value(h, v);
            }
            h
        }
    }
}

/// A hash of everything a run can observe: variables, time, waiting delays,
/// world fields, logs and the write count.
pub fn state_hash(g: &dyn GraphInstance, host: &MapHost) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325;
    for (name, v) in g.vars() {
        h = mix(h, name.as_bytes());
        h = mix_value(h, &v);
    }
    h = mix(h, &g.time().to_bits().to_le_bytes());
    h = mix(h, &(g.pending_delays() as u64).to_le_bytes());
    for ((e, c, f), v) in &host.fields {
        h = mix(h, &e.to_le_bytes());
        h = mix(mix(h, c.as_bytes()), f.as_bytes());
        h = mix_value(h, v);
    }
    for line in &host.logs {
        h = mix(mix(h, &(line.len() as u64).to_le_bytes()), line.as_bytes());
    }
    mix(h, &(host.writes as u64).to_le_bytes())
}
