//! Run graphs against a `seal_ecs::World`: `world_get` and `world_set` read
//! and write text components by name.

use crate::rt::{ops, Host, Plane, Quat, Ty, Value, V2, V3};
use seal_ecs::{Entity, PrefabInstance, PrefabLibrary, World};
use seal_sealtext::{SealMap, SealValue};

/// A host over a world. Log lines are collected in `logs`.
pub struct EcsHost<'w> {
    pub world: &'w mut World,
    pub logs: Vec<String>,
    /// Prefabs `spawn` nodes can create; without them spawning fails.
    pub prefabs: Option<&'w PrefabLibrary>,
    /// The entity each local player controls, first player first.
    pub players: Vec<Entity>,
}

impl<'w> EcsHost<'w> {
    pub fn new(world: &'w mut World) -> Self {
        Self { world, logs: Vec::new(), prefabs: None, players: Vec::new() }
    }

    pub fn with_players(mut self, players: &[Entity]) -> Self {
        self.players = players.to_vec();
        self
    }

    pub fn with_prefabs(mut self, prefabs: &'w PrefabLibrary) -> Self {
        self.prefabs = Some(prefabs);
        self
    }
}

fn number(v: &SealValue) -> Option<f64> {
    if let SealValue::Number(n) = v {
        n.parse().ok()
    } else {
        None
    }
}

/// Convert a stored field to the type the graph expects.
pub fn from_seal(v: &SealValue, ty: Ty) -> Option<Value> {
    Some(match ty {
        // Components hold plain fields; a list is never stored in one.
        Ty::List(_) => return None,
        Ty::Bool => match v {
            SealValue::Bool(b) => Value::Bool(*b),
            _ => return None,
        },
        Ty::Int => match v {
            SealValue::Number(n) => Value::Int(n.parse().ok()?),
            _ => return None,
        },
        Ty::Float => Value::Float(number(v)?),
        Ty::Vec2 => match v {
            SealValue::Array(a) if a.len() == 2 => {
                Value::Vec2(V2 { x: number(&a[0])?, y: number(&a[1])? })
            }
            SealValue::Map(_) => {
                Value::Vec2(V2 { x: number(v.get("x")?)?, y: number(v.get("y")?)? })
            }
            _ => return None,
        },
        Ty::Vec3 => match v {
            SealValue::Array(a) if a.len() == 3 => {
                Value::Vec3(V3 { x: number(&a[0])?, y: number(&a[1])?, z: number(&a[2])? })
            }
            SealValue::Map(_) => Value::Vec3(V3 {
                x: number(v.get("x")?)?,
                y: number(v.get("y")?)?,
                z: number(v.get("z")?)?,
            }),
            _ => return None,
        },
        Ty::Str => match v {
            SealValue::String(s) => Value::Str(s.clone()),
            _ => return None,
        },
        Ty::Entity => match v {
            SealValue::String(s) => Value::Entity(Entity::parse(s)?.to_bits()),
            _ => return None,
        },
        Ty::Quat => match v {
            SealValue::Array(a) if a.len() == 4 => Value::Quat(Quat {
                x: number(&a[0])?,
                y: number(&a[1])?,
                z: number(&a[2])?,
                w: number(&a[3])?,
            }),
            _ => return None,
        },
        Ty::Plane => match v {
            SealValue::Array(a) if a.len() == 4 => Value::Plane(Plane {
                nx: number(&a[0])?,
                ny: number(&a[1])?,
                nz: number(&a[2])?,
                d: number(&a[3])?,
            }),
            _ => return None,
        },
    })
}

/// Convert a value for storage, keeping the shape of what was there (a vec2
/// stored as `{ x, y }` stays a map).
pub fn to_seal(v: &Value, old: Option<&SealValue>) -> SealValue {
    match v {
        Value::Vec2(p) if matches!(old, Some(SealValue::Map(_))) => {
            SealValue::Map(SealMap::new(vec![
                ("x".into(), crate::value_to_text(&Value::Float(p.x))),
                ("y".into(), crate::value_to_text(&Value::Float(p.y))),
            ]))
        }
        Value::Vec3(p) if matches!(old, Some(SealValue::Map(_))) => {
            SealValue::Map(SealMap::new(vec![
                ("x".into(), crate::value_to_text(&Value::Float(p.x))),
                ("y".into(), crate::value_to_text(&Value::Float(p.y))),
                ("z".into(), crate::value_to_text(&Value::Float(p.z))),
            ]))
        }
        Value::Entity(e) => SealValue::String(ops::entity_str(*e)),
        other => crate::value_to_text(other),
    }
}

impl Host for EcsHost<'_> {
    fn get(&mut self, entity: u64, component: &str, field: &str, ty: Ty) -> Result<Value, String> {
        let e = Entity::from_bits(entity);
        let value = self
            .world
            .component_value(e, component)
            .ok_or_else(|| format!("{e:?} has no {component}"))?;
        let raw = value.get(field).ok_or_else(|| format!("{component} has no field {field}"))?;
        from_seal(raw, ty).ok_or_else(|| format!("{component}.{field} is not a {ty}"))
    }

    fn set(
        &mut self,
        entity: u64,
        component: &str,
        field: &str,
        value: Value,
    ) -> Result<(), String> {
        let e = Entity::from_bits(entity);
        let old = self.world.component_value(e, component).and_then(|c| c.get(field).cloned());
        self.world.set_component_field(e, component, field, to_seal(&value, old.as_ref()))
    }

    fn log(&mut self, message: &str) {
        self.logs.push(message.to_string());
    }

    fn spawn(&mut self, prefab: &str) -> Result<u64, String> {
        let lib = self.prefabs.ok_or_else(|| format!("no prefabs loaded to spawn {prefab}"))?;
        Ok(self.world.instantiate(lib, prefab)?.to_bits())
    }

    fn despawn(&mut self, entity: u64) -> Result<(), String> {
        let e = Entity::from_bits(entity);
        if self.world.contains(e) {
            self.world.despawn_recursive(e);
        }
        Ok(())
    }

    fn exists(&mut self, entity: u64) -> bool {
        self.world.contains(Entity::from_bits(entity))
    }

    fn is_a(&mut self, entity: u64, prefab: &str) -> bool {
        let e = Entity::from_bits(entity);
        let Some(made) = self.world.get::<PrefabInstance>(e) else { return false };
        match self.prefabs {
            Some(lib) => lib.is_a(&made.0, prefab),
            None => made.0 == prefab,
        }
    }

    fn player(&mut self, index: i64) -> u64 {
        let e = usize::try_from(index).ok().and_then(|i| self.players.get(i)).copied();
        // A player whose entity is gone controls nothing.
        e.filter(|e| self.world.contains(*e)).map_or(crate::rt::NO_ENTITY, Entity::to_bits)
    }

    fn find_all(&mut self, prefab: &str) -> Vec<u64> {
        let all = self.world.entities();
        all.into_iter().map(Entity::to_bits).filter(|e| self.is_a(*e, prefab)).collect()
    }
}
