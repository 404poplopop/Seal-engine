//! Components as SealText, and whole worlds as scene files.
//!
//! A scene file lists entities with file-local ids (0, 1, 2 …) in a stable
//! order, and each entity's components by their registered names, sorted.
//! References to other entities are written as those ids and remapped on
//! load, so a scene can be loaded into any world, any number of times.
//!
//! ```text
//! seal_scene = 1
//! entities = [
//!   { id = 0, components = { name = "Lamp", transform = { … } } },
//!   { id = 1, components = { child_of = 0, … } },
//! ]
//! ```

use std::collections::{BTreeMap, HashMap};

use seal_sealtext::{parse, SealMap, SealValue};

use crate::hierarchy::ChildOf;
use crate::world::{Entity, World};
use crate::Component;

/// Turns entity references into file-local ids while saving.
pub struct SaveCtx<'a> {
    /// `None` writes live entity ids as `"5v1"` strings (for tools and agents working on a running world).
    ids: Option<&'a HashMap<Entity, u64>>,
}

impl SaveCtx<'static> {
    /// Live entity ids, as tools and resources use.
    pub fn plain() -> Self {
        Self { ids: None }
    }
}

impl SaveCtx<'_> {
    /// An entity reference; references to entities outside the saved set become `null`.
    pub fn entity(&self, entity: Entity) -> SealValue {
        match self.ids {
            Some(ids) => {
                ids.get(&entity).map_or(SealValue::Null, |id| SealValue::Number(id.to_string()))
            }
            None => SealValue::String(format!("{entity:?}")),
        }
    }
}

/// Turns file-local ids back into live entities while loading.
pub struct LoadCtx<'a> {
    /// `None` reads live entity ids.
    map: Option<&'a HashMap<u64, Entity>>,
}

impl LoadCtx<'static> {
    /// Live entity ids, as tools and resources use.
    pub fn plain() -> Self {
        Self { map: None }
    }
}

impl LoadCtx<'_> {
    pub fn entity(&self, value: &SealValue) -> Result<Option<Entity>, String> {
        match value {
            SealValue::Null => Ok(None),
            SealValue::String(text) if self.map.is_none() => Entity::parse(text)
                .map(Some)
                .ok_or_else(|| format!("bad entity id {text:?}, expected like \"5v1\"")),
            SealValue::Number(text) => {
                let id: u64 = text.parse().map_err(|_| format!("bad entity id {text}"))?;
                match self.map {
                    Some(map) => map
                        .get(&id)
                        .copied()
                        .map(Some)
                        .ok_or_else(|| format!("entity id {id} is not in this scene")),
                    None => Err(format!("entity ids are written like \"5v1\", not {id}")),
                }
            }
            other => Err(format!("expected an entity id, found {other:?}")),
        }
    }
}

/// A value that can be one field of a text component.
pub trait SealField: Sized {
    fn save(&self, ctx: &SaveCtx) -> SealValue;
    fn load(value: &SealValue, ctx: &LoadCtx) -> Result<Self, String>;
}

fn number<T: std::str::FromStr>(value: &SealValue) -> Result<T, String> {
    match value {
        SealValue::Number(text) => text.parse().map_err(|_| format!("{text} is out of range")),
        other => Err(format!("expected a number, found {other:?}")),
    }
}

macro_rules! number_field {
    ($($ty:ty),*) => {$(
        impl SealField for $ty {
            fn save(&self, _: &SaveCtx) -> SealValue {
                SealValue::Number(self.to_string())
            }
            fn load(value: &SealValue, _: &LoadCtx) -> Result<Self, String> {
                number(value)
            }
        }
    )*};
}

// Rust prints floats with the shortest text that reads back to the same bits.
number_field!(f32, f64, i8, i16, i32, i64, u8, u16, u32, u64);

impl SealField for bool {
    fn save(&self, _: &SaveCtx) -> SealValue {
        SealValue::Bool(*self)
    }
    fn load(value: &SealValue, _: &LoadCtx) -> Result<Self, String> {
        match value {
            SealValue::Bool(b) => Ok(*b),
            other => Err(format!("expected true or false, found {other:?}")),
        }
    }
}

impl SealField for String {
    fn save(&self, _: &SaveCtx) -> SealValue {
        SealValue::String(self.clone())
    }
    fn load(value: &SealValue, _: &LoadCtx) -> Result<Self, String> {
        match value {
            SealValue::String(s) => Ok(s.clone()),
            other => Err(format!("expected a string, found {other:?}")),
        }
    }
}

impl SealField for Entity {
    fn save(&self, ctx: &SaveCtx) -> SealValue {
        ctx.entity(*self)
    }
    fn load(value: &SealValue, ctx: &LoadCtx) -> Result<Self, String> {
        ctx.entity(value)?.ok_or_else(|| "missing entity".to_string())
    }
}

impl<T: SealField> SealField for Option<T> {
    fn save(&self, ctx: &SaveCtx) -> SealValue {
        self.as_ref().map_or(SealValue::Null, |v| v.save(ctx))
    }
    fn load(value: &SealValue, ctx: &LoadCtx) -> Result<Self, String> {
        match value {
            SealValue::Null => Ok(None),
            other => T::load(other, ctx).map(Some),
        }
    }
}

impl<T: SealField> SealField for Vec<T> {
    fn save(&self, ctx: &SaveCtx) -> SealValue {
        SealValue::Array(self.iter().map(|v| v.save(ctx)).collect())
    }
    fn load(value: &SealValue, ctx: &LoadCtx) -> Result<Self, String> {
        match value {
            SealValue::Array(items) => items.iter().map(|v| T::load(v, ctx)).collect(),
            other => Err(format!("expected a list, found {other:?}")),
        }
    }
}

impl<T: SealField, const N: usize> SealField for [T; N] {
    fn save(&self, ctx: &SaveCtx) -> SealValue {
        SealValue::Array(self.iter().map(|v| v.save(ctx)).collect())
    }
    fn load(value: &SealValue, ctx: &LoadCtx) -> Result<Self, String> {
        let items: Vec<T> = Vec::load(value, ctx)?;
        let len = items.len();
        items.try_into().map_err(|_| format!("expected {N} values, found {len}"))
    }
}

/// A component that can be written to and read from scene files.
/// Implement it with [`text_component!`](crate::text_component).
pub trait TextComponent: Component + Clone {
    /// The stable name used in scene files. Renaming the Rust type does not change it.
    const NAME: &'static str;
    /// Bump when the saved shape changes, and teach `migrate` the old shapes.
    const VERSION: u32 = 1;
    fn save(&self, ctx: &SaveCtx) -> SealValue;
    fn load(value: &SealValue, ctx: &LoadCtx) -> Result<Self, String>;
    /// Turn a value saved by version `from` (older than `VERSION`) into the current shape.
    fn migrate(from: u32, value: SealValue) -> Result<SealValue, String> {
        let _ = value;
        Err(format!("{} cannot read version {from} data", Self::NAME))
    }
    /// The field names, for editors and agents. Empty for newtypes and markers.
    fn fields() -> &'static [&'static str] {
        &[]
    }
}

/// Make a component readable and writable as SealText.
///
/// * `text_component!(Transform as "transform" { translation, rotation, scale });`
///   for a struct with named fields that implements `Default`. Missing fields
///   load as their default, and unknown fields are ignored, so older and newer
///   files both load.
/// * `text_component!(Transform as "transform" version 2 migrate upgrade { … });`
///   when the saved shape changed: `upgrade(from_version, old_value)` returns
///   the value in the current shape.
/// * `text_component!(Health as "health" newtype);` for a one-field tuple struct.
/// * `text_component!(Player as "player" marker);` for a unit struct.
#[macro_export]
macro_rules! text_component {
    ($ty:ident as $name:literal version $version:literal migrate $migrate:path { $($field:ident),* $(,)? }) => {
        $crate::text_component!(@fields $ty, $name, $version, { fn migrate(from: u32, value: $crate::SealValue) -> Result<$crate::SealValue, String> { $migrate(from, value) } }, $($field),*);
    };
    ($ty:ident as $name:literal { $($field:ident),* $(,)? }) => {
        $crate::text_component!(@fields $ty, $name, 1, {}, $($field),*);
    };
    (@fields $ty:ident, $name:literal, $version:literal, { $($extra:tt)* }, $($field:ident),*) => {
        impl $crate::TextComponent for $ty {
            const NAME: &'static str = $name;
            const VERSION: u32 = $version;
            $($extra)*
            fn fields() -> &'static [&'static str] {
                &[$(stringify!($field)),*]
            }
            fn save(&self, ctx: &$crate::SaveCtx) -> $crate::SealValue {
                $crate::SealValue::Map($crate::SealMap::new(vec![
                    $((stringify!($field).to_string(), $crate::SealField::save(&self.$field, ctx)),)*
                ]))
            }
            fn load(value: &$crate::SealValue, ctx: &$crate::LoadCtx) -> Result<Self, String> {
                if value.as_map().is_none() {
                    return Err("expected a map".into());
                }
                #[allow(unused_mut)]
                let mut out = <$ty as Default>::default();
                $(if let Some(field) = value.get(stringify!($field)) {
                    out.$field = $crate::SealField::load(field, ctx)
                        .map_err(|e| format!("{}: {e}", stringify!($field)))?;
                })*
                Ok(out)
            }
        }
    };
    ($ty:ident as $name:literal newtype) => {
        impl $crate::TextComponent for $ty {
            const NAME: &'static str = $name;
            fn save(&self, ctx: &$crate::SaveCtx) -> $crate::SealValue {
                $crate::SealField::save(&self.0, ctx)
            }
            fn load(value: &$crate::SealValue, ctx: &$crate::LoadCtx) -> Result<Self, String> {
                $crate::SealField::load(value, ctx).map($ty)
            }
        }
    };
    ($ty:ident as $name:literal marker) => {
        impl $crate::TextComponent for $ty {
            const NAME: &'static str = $name;
            fn save(&self, _: &$crate::SaveCtx) -> $crate::SealValue {
                $crate::SealValue::Bool(true)
            }
            fn load(_: &$crate::SealValue, _: &$crate::LoadCtx) -> Result<Self, String> {
                Ok($ty)
            }
        }
    };
}

pub(crate) fn canonical<T: TextComponent>(value: &SealValue) -> Result<SealValue, String> {
    let component =
        T::load(value, &LoadCtx { map: None }).map_err(|e| format!("{}: {e}", T::NAME))?;
    Ok(component.save(&SaveCtx { ids: None }))
}

/// A stable id for an entity in scene files. The editor gives one to every
/// entity it saves, so files keep the same ids and order between saves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SceneId(pub u64);

crate::text_component!(SceneId as "scene_id" newtype);

/// The result of saving: the text, and component types that were left out
/// because they are not registered for text.
#[derive(Debug)]
pub struct SavedScene {
    pub text: String,
    pub skipped: Vec<&'static str>,
}

pub const SCENE_VERSION: u64 = 1;

impl World {
    /// Write `entities` (or every entity, if `None`) as a scene file.
    pub fn save_scene(&self, entities: Option<&[Entity]>) -> SavedScene {
        let (scene, _, skipped) = self.scene_value(entities);
        SavedScene { text: scene.to_text(), skipped }
    }

    /// The scene as a value, with the entities in row order.
    pub(crate) fn scene_value(
        &self,
        entities: Option<&[Entity]>,
    ) -> (SealValue, Vec<Entity>, Vec<&'static str>) {
        let mut list: Vec<Entity> = match entities {
            Some(list) => list.iter().copied().filter(|e| self.contains(*e)).collect(),
            None => self.tables.iter().flat_map(|t| t.entities.iter().copied()).collect(),
        };
        list.sort_by_key(|e| e.index());
        list.dedup();
        // Entities with a SceneId keep it as their file id, so ids, references
        // and entity order stay the same from save to save (diffs and merges
        // depend on that). The rest get ids above the largest SceneId.
        let mut next =
            list.iter().filter_map(|e| self.get::<SceneId>(*e)).map(|s| s.0 + 1).max().unwrap_or(0);
        let mut ids: HashMap<Entity, u64> = HashMap::with_capacity(list.len());
        for &entity in &list {
            let id = match self.get::<SceneId>(entity) {
                Some(scene_id) if !ids.values().any(|v| *v == scene_id.0) => scene_id.0,
                _ => {
                    next += 1;
                    next - 1
                }
            };
            ids.insert(entity, id);
        }
        list.sort_by_key(|e| ids[e]);
        let ctx = SaveCtx { ids: Some(&ids) };
        let mut skipped = Vec::new();
        let mut versions = BTreeMap::new();
        let mut rows = Vec::new();
        for &entity in &list {
            let id = ids[&entity];
            let (table_index, row) = self.location(entity).expect("filtered to live entities");
            let table = &self.tables[table_index];
            let mut components = BTreeMap::new();
            for (column_index, info) in table.infos.iter().enumerate() {
                match self.registry.by_type.get(&info.type_id).and_then(|r| r.text) {
                    Some(ops) => {
                        components.insert(
                            ops.name,
                            (ops.save)(table.columns[column_index].as_ref(), row, &ctx),
                        );
                        if ops.version > 1 {
                            versions.insert(ops.name, ops.version);
                        }
                    }
                    None => {
                        // Children is rebuilt from ChildOf on load; never written.
                        if info.type_id != std::any::TypeId::of::<crate::hierarchy::Children>()
                            && !skipped.contains(&info.name)
                        {
                            skipped.push(info.name);
                        }
                    }
                }
            }
            let components = components.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
            rows.push(SealValue::Map(SealMap::new(vec![
                ("id".into(), SealValue::Number(id.to_string())),
                ("components".into(), SealValue::Map(SealMap::new(components))),
            ])));
        }
        let mut header = vec![("seal_scene".into(), SealValue::Number(SCENE_VERSION.to_string()))];
        if !versions.is_empty() {
            let versions = versions
                .into_iter()
                .map(|(k, v)| (k.to_string(), SealValue::Number(v.to_string())))
                .collect();
            header.push(("versions".into(), SealValue::Map(SealMap::new(versions))));
        }
        let resources: Vec<(String, SealValue)> = self
            .registry
            .resources
            .iter()
            .filter_map(|(name, ops)| (ops.save)(self).map(|v| (name.to_string(), v)))
            .collect();
        if !resources.is_empty() {
            header.push(("resources".into(), SealValue::Map(SealMap::new(resources))));
        }
        header.push(("entities".into(), SealValue::Array(rows)));
        (SealValue::Map(SealMap::new(header)), list, skipped)
    }

    /// Spawn every entity in a scene file into this world, with references
    /// remapped. Nothing is spawned if any part of the file is invalid.
    pub fn load_scene(&mut self, text: &str) -> Result<Vec<Entity>, String> {
        let mut scene = parse(text).map_err(|e| e.to_string())?;
        if crate::vocab::detect(&scene) == crate::vocab::Vocabulary::Classic {
            scene = parse(&crate::vocab::to_scene_graph(text)?).map_err(|e| e.to_string())?;
        }
        let version: u64 = scene
            .get("seal_scene")
            .ok_or("not a Seal scene: no seal_scene version")
            .and_then(|v| number(v).map_err(|_| "bad seal_scene version"))?;
        if version > SCENE_VERSION {
            return Err(format!(
                "scene version {version} is newer than this engine ({SCENE_VERSION})"
            ));
        }
        let Some(SealValue::Array(rows)) = scene.get("entities") else {
            return Err("scene has no entities list".into());
        };
        // Component versions the file was written with (1 when not listed).
        let mut file_versions: HashMap<&str, u32> = HashMap::new();
        if let Some(map) = scene.get("versions").and_then(SealValue::as_map) {
            for (name, value) in map {
                file_versions.insert(
                    name.as_str(),
                    number(value).map_err(|e| format!("versions.{name}: {e}"))?,
                );
            }
        }
        // Scene-wide resources: checked now, inserted once the entities load.
        let resources = scene.get("resources").and_then(SealValue::as_map).unwrap_or(&[]);
        let mut resource_ops = Vec::with_capacity(resources.len());
        for (name, value) in resources {
            let Some(ops) = self.registry.resources.get(name.as_str()).copied() else {
                return Err(format!("unknown resource {name}"));
            };
            (ops.check)(value).map_err(|e| format!("resource {name}: {e}"))?;
            resource_ops.push((ops, value));
        }
        // Pass 1: validate ids, names and versions before touching the world.
        let mut parsed = Vec::with_capacity(rows.len());
        for (index, row) in rows.iter().enumerate() {
            let id: u64 = row.get("id").ok_or(format!("entity {index}: no id")).and_then(number)?;
            let components = row.get("components").and_then(SealValue::as_map).unwrap_or(&[]);
            for (name, _) in components {
                let Some(type_id) = self.registry.by_name.get(name.as_str()) else {
                    return Err(format!("entity {id}: unknown component {name}"));
                };
                let ours =
                    self.registry.by_type[type_id].text.expect("named types have text").version;
                let theirs = file_versions.get(name.as_str()).copied().unwrap_or(1);
                if theirs > ours {
                    return Err(format!("{name} was saved by a newer version ({theirs}) than this engine reads ({ours})"));
                }
            }
            parsed.push((id, components));
        }
        let mut map = HashMap::new();
        let mut spawned = Vec::new();
        for (id, _) in &parsed {
            let entity = self.spawn(());
            spawned.push(entity);
            if map.insert(*id, entity).is_some() {
                return Err(
                    self.undo_failed_load(&spawned, format!("entity id {id} appears twice"))
                );
            }
        }
        // Pass 2: components, with every id already mapped.
        let ctx = LoadCtx { map: Some(&map) };
        for ((id, components), &entity) in parsed.iter().zip(&spawned) {
            for (name, value) in components.iter() {
                let type_id = self.registry.by_name[name.as_str()];
                let ops = self.registry.by_type[&type_id].text.expect("registered for text");
                let saved_as = file_versions.get(name.as_str()).copied().unwrap_or(1);
                let migrated;
                let value = if saved_as < ops.version {
                    match (ops.migrate)(saved_as, value.clone()) {
                        Ok(v) => {
                            migrated = v;
                            &migrated
                        }
                        Err(message) => {
                            return Err(
                                self.undo_failed_load(&spawned, format!("entity {id}: {message}"))
                            )
                        }
                    }
                } else {
                    value
                };
                if let Err(message) = (ops.load)(self, entity, value, &ctx) {
                    return Err(self.undo_failed_load(&spawned, format!("entity {id}: {message}")));
                }
            }
        }
        for (ops, value) in resource_ops {
            (ops.load)(self, value).expect("checked above");
        }
        // Rebuild Children from the loaded ChildOf links.
        for &entity in &spawned {
            if let Some(parent) = self.get::<ChildOf>(entity).map(|c| c.parent()) {
                self.link_child(parent, entity);
            }
        }
        Ok(spawned)
    }

    /// Names of every component type that can be read and written as text, sorted.
    pub fn text_component_names(&self) -> Vec<&'static str> {
        let mut names: Vec<_> = self.registry.by_name.keys().copied().collect();
        names.sort_unstable();
        names
    }

    /// The field names of a text component, empty for newtypes and markers.
    pub fn component_fields(&self, name: &str) -> Option<&'static [&'static str]> {
        let ops = self.text_ops(name)?;
        Some((ops.fields)())
    }

    pub(crate) fn text_ops(&self, name: &str) -> Option<crate::registry::TextOps> {
        self.registry.by_type.get(self.registry.by_name.get(name)?)?.text
    }

    /// One component as SealText, with entity references as live ids.
    pub fn component_value(&self, entity: Entity, name: &str) -> Option<SealValue> {
        let type_id = *self.registry.by_name.get(name)?;
        let ops = self.text_ops(name)?;
        let (table, row) = self.location(entity)?;
        let column = self.tables[table].column_index(type_id)?;
        Some((ops.save)(self.tables[table].columns[column].as_ref(), row, &SaveCtx { ids: None }))
    }

    /// Every text component on an entity, by name.
    pub fn entity_values(&self, entity: Entity) -> Option<Vec<(&'static str, SealValue)>> {
        self.location(entity)?;
        Some(
            self.text_component_names()
                .into_iter()
                .filter_map(|n| Some((n, self.component_value(entity, n)?)))
                .collect(),
        )
    }

    /// Add or replace a component from SealText (entity references as live ids).
    pub fn set_component_value(
        &mut self,
        entity: Entity,
        name: &str,
        value: &SealValue,
    ) -> Result<(), String> {
        let ops = self.text_ops(name).ok_or_else(|| format!("unknown component {name}"))?;
        if !self.contains(entity) {
            return Err(format!("no entity {entity:?}"));
        }
        (ops.load)(self, entity, value, &LoadCtx { map: None })
    }

    /// Change one field of a component in place, keeping the others.
    pub fn set_component_field(
        &mut self,
        entity: Entity,
        name: &str,
        field: &str,
        value: SealValue,
    ) -> Result<(), String> {
        let fields =
            self.component_fields(name).ok_or_else(|| format!("unknown component {name}"))?;
        if !fields.contains(&field) {
            return Err(format!("{name} has no field {field}; it has {}", fields.join(", ")));
        }
        let mut current = self
            .component_value(entity, name)
            .ok_or_else(|| format!("{entity:?} has no {name}"))?;
        current.insert(field, value);
        self.set_component_value(entity, name, &current)
    }

    /// Remove a component by its text name. Returns false if it was not there.
    pub fn remove_component_named(&mut self, entity: Entity, name: &str) -> Result<bool, String> {
        let ops = self.text_ops(name).ok_or_else(|| format!("unknown component {name}"))?;
        Ok((ops.remove)(self, entity))
    }

    /// Give a fresh, unique SceneId to every entity that has none or shares one
    /// (for example after a scene was loaded twice). Returns how many changed.
    pub fn ensure_scene_ids(&mut self) -> usize {
        let mut all: Vec<Entity> =
            self.tables.iter().flat_map(|t| t.entities.iter().copied()).collect();
        all.sort_by_key(|e| e.index());
        let mut next =
            all.iter().filter_map(|e| self.get::<SceneId>(*e)).map(|s| s.0 + 1).max().unwrap_or(0);
        let mut seen = std::collections::HashSet::new();
        let mut changed = 0;
        for entity in all {
            let keep = self.get::<SceneId>(entity).is_some_and(|s| seen.insert(s.0));
            if !keep {
                self.insert(entity, SceneId(next));
                seen.insert(next);
                next += 1;
                changed += 1;
            }
        }
        changed
    }

    /// Remove everything a failed load spawned, so the world is as it was.
    fn undo_failed_load(&mut self, spawned: &[Entity], message: String) -> String {
        for &entity in spawned {
            self.despawn(entity);
        }
        message
    }
}
