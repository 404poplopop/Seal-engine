//! Prefabs: reusable entity templates stored as SealText patches.
//!
//! A prefab names an optional base prefab and stores only what it changes:
//! component values (merged field by field over the base) and child slots.
//! The chain from the root base to the prefab is its class chain in the
//! class vocabulary; a prefab that extends another is a child class or,
//! in scene-graph mode, a variant. Both are the same data.
//!
//! Instances carry [`PrefabInstance`]; the children a prefab creates carry
//! [`PrefabSlot`] so later edits to the prefab can find them. A compact scene
//! stores only each instance's overrides, and loading it picks up any change
//! made to the prefab since.
//!
//! Patch rules: two maps merge key by key; anything else replaces. In
//! instance overrides, a component set to null was removed from the instance.

use std::collections::{BTreeMap, HashMap};

use seal_sealtext::{parse, SealMap, SealValue};

use crate::vocab::{detect, to_classic, to_scene_graph, Vocabulary};
use crate::world::{Entity, World};

/// This entity is an instance of the named prefab (an instance of that class).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PrefabInstance(pub String);
crate::text_component!(PrefabInstance as "prefab" newtype);

/// This entity was created by its parent's prefab, in the named child slot.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PrefabSlot(pub String);
crate::text_component!(PrefabSlot as "prefab_slot" newtype);

/// Components managed by the engine, never part of a prefab or its overrides.
const BOOKKEEPING: &[&str] = &["prefab", "prefab_slot", "child_of", "scene_id"];

/// A child slot of a prefab: an instance of another prefab or inline components.
#[derive(Clone, Debug, PartialEq)]
pub struct PrefabChild {
    pub slot: String,
    pub prefab: Option<String>,
    pub components: Vec<(String, SealValue)>,
}

/// One prefab (class) as stored in its file.
#[derive(Clone, Debug, PartialEq)]
pub struct Prefab {
    pub name: String,
    pub base: Option<String>,
    pub components: Vec<(String, SealValue)>,
    pub children: Vec<PrefabChild>,
}

/// A prefab with its whole base chain applied.
#[derive(Clone, Debug, PartialEq)]
pub struct Resolved {
    /// Root base first, the prefab itself last. Empty for inline children.
    pub chain: Vec<String>,
    pub components: Vec<(String, SealValue)>,
    pub children: Vec<(String, Resolved)>,
}

fn map_entries(value: Option<&SealValue>, what: &str) -> Result<Vec<(String, SealValue)>, String> {
    match value {
        None => Ok(Vec::new()),
        Some(SealValue::Map(map)) => Ok(map.entries.clone()),
        Some(_) => Err(format!("{what} must be a map")),
    }
}

fn string(value: Option<&SealValue>, what: &str) -> Result<Option<String>, String> {
    match value {
        None | Some(SealValue::Null) => Ok(None),
        Some(SealValue::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(format!("{what} must be a string")),
    }
}

/// Merge `patch` over `base`: maps merge key by key, anything else replaces.
pub fn merge_patch(base: &mut SealValue, patch: &SealValue) {
    match (base.as_map_mut(), patch.as_map()) {
        (Some(base_entries), Some(patch_entries)) => {
            for (key, value) in patch_entries {
                match base_entries.iter_mut().find(|(k, _)| k == key) {
                    Some((_, existing)) => merge_patch(existing, value),
                    None => base_entries.push((key.clone(), value.clone())),
                }
            }
        }
        _ => *base = patch.clone(),
    }
}

/// What `current` changes relative to `base`, as a patch, or None when equal.
pub fn diff_patch(base: &SealValue, current: &SealValue) -> Option<SealValue> {
    match (base.as_map(), current.as_map()) {
        (Some(b), Some(c)) => {
            let mut out = Vec::new();
            for (key, value) in c {
                match b.iter().find(|(k, _)| k == key) {
                    Some((_, old)) => {
                        if let Some(changed) = diff_patch(old, value) {
                            out.push((key.clone(), changed));
                        }
                    }
                    None => out.push((key.clone(), value.clone())),
                }
            }
            (!out.is_empty()).then(|| SealValue::Map(SealMap::new(out)))
        }
        _ => (base != current).then(|| current.clone()),
    }
}

fn patch_components(into: &mut Vec<(String, SealValue)>, patch: &[(String, SealValue)]) {
    for (name, value) in patch {
        if *value == SealValue::Null {
            into.retain(|(n, _)| n != name);
            continue;
        }
        match into.iter_mut().find(|(n, _)| n == name) {
            Some((_, existing)) => merge_patch(existing, value),
            None => into.push((name.clone(), value.clone())),
        }
    }
}

impl Prefab {
    /// Read a prefab file written in either vocabulary.
    pub fn from_text(text: &str) -> Result<Prefab, String> {
        let value = parse(text).map_err(|e| e.to_string())?;
        let value = if detect(&value) == Vocabulary::Classic {
            parse(&to_scene_graph(text)?).map_err(|e| e.to_string())?
        } else {
            value
        };
        if value.get("seal_prefab").is_none() {
            return Err("not a prefab or class file".into());
        }
        let name = string(value.get("prefab"), "prefab")?.ok_or("prefab file has no name")?;
        let mut children = Vec::new();
        match value.get("children") {
            None => {}
            Some(SealValue::Array(items)) => {
                for (i, item) in items.iter().enumerate() {
                    let slot = string(item.get("slot"), "slot")?
                        .ok_or(format!("{name}: child {i} has no slot name"))?;
                    if children.iter().any(|c: &PrefabChild| c.slot == slot) {
                        return Err(format!("{name}: slot {slot} appears twice"));
                    }
                    children.push(PrefabChild {
                        prefab: string(item.get("prefab"), "prefab")?,
                        components: map_entries(item.get("components"), "components")?,
                        slot,
                    });
                }
            }
            Some(_) => return Err("children must be a list".into()),
        }
        Ok(Prefab {
            base: string(value.get("base"), "base")?,
            components: map_entries(value.get("components"), "components")?,
            children,
            name,
        })
    }

    /// Write this prefab in the given vocabulary.
    pub fn to_text(&self, vocabulary: Vocabulary) -> String {
        let mut entries = vec![
            ("seal_prefab".to_string(), SealValue::Number("1".into())),
            ("prefab".to_string(), SealValue::String(self.name.clone())),
        ];
        if let Some(base) = &self.base {
            entries.push(("base".into(), SealValue::String(base.clone())));
        }
        entries.push(("components".into(), SealValue::Map(SealMap::new(self.components.clone()))));
        if !self.children.is_empty() {
            let children = self
                .children
                .iter()
                .map(|c| {
                    let mut e = vec![("slot".to_string(), SealValue::String(c.slot.clone()))];
                    if let Some(p) = &c.prefab {
                        e.push(("prefab".into(), SealValue::String(p.clone())));
                    }
                    e.push((
                        "components".into(),
                        SealValue::Map(SealMap::new(c.components.clone())),
                    ));
                    SealValue::Map(SealMap::new(e))
                })
                .collect();
            entries.push(("children".into(), SealValue::Array(children)));
        }
        let text = SealValue::Map(SealMap::new(entries)).to_text();
        match vocabulary {
            Vocabulary::SceneGraph => text,
            Vocabulary::Classic => to_classic(&text).expect("written in scene-graph words"),
        }
    }
}

/// Every prefab known to a project, by name.
#[derive(Clone, Debug, Default)]
pub struct PrefabLibrary {
    prefabs: BTreeMap<String, Prefab>,
}

impl PrefabLibrary {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, prefab: Prefab) -> Option<Prefab> {
        self.prefabs.insert(prefab.name.clone(), prefab)
    }

    pub fn remove(&mut self, name: &str) -> Option<Prefab> {
        self.prefabs.remove(name)
    }

    pub fn get(&self, name: &str) -> Option<&Prefab> {
        self.prefabs.get(name)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.prefabs.keys().map(String::as_str)
    }

    /// Root base first, `name` last.
    pub fn class_chain(&self, name: &str) -> Result<Vec<String>, String> {
        let mut chain = vec![name.to_string()];
        let mut current = self.get(name).ok_or(format!("unknown prefab {name}"))?;
        while let Some(base) = &current.base {
            if chain.contains(base) {
                return Err(format!("prefab {name}: base chain loops at {base}"));
            }
            chain.push(base.clone());
            current =
                self.get(base).ok_or(format!("prefab {}: unknown base {base}", current.name))?;
        }
        chain.reverse();
        Ok(chain)
    }

    /// True when `name` is `ancestor` or extends it (is-a in classic terms).
    pub fn is_a(&self, name: &str, ancestor: &str) -> bool {
        self.class_chain(name).is_ok_and(|c| c.iter().any(|n| n == ancestor))
    }

    /// The prefab with its base chain and nested prefabs applied.
    pub fn resolve(&self, name: &str) -> Result<Resolved, String> {
        self.resolve_inner(name, &mut Vec::new())
    }

    fn resolve_inner(&self, name: &str, nesting: &mut Vec<String>) -> Result<Resolved, String> {
        if nesting.iter().any(|n| n == name) {
            return Err(format!("prefab {name} contains itself"));
        }
        nesting.push(name.to_string());
        let chain = self.class_chain(name)?;
        let mut out =
            Resolved { chain: chain.clone(), components: Vec::new(), children: Vec::new() };
        for link in &chain {
            let prefab = &self.prefabs[link];
            patch_components(&mut out.components, &prefab.components);
            for child in &prefab.children {
                let existing = out.children.iter().position(|(s, _)| *s == child.slot);
                let mut node = match (&child.prefab, existing) {
                    // Naming a prefab on a slot (re)places what the slot holds.
                    (Some(p), _) => self.resolve_inner(p, nesting)?,
                    (None, Some(i)) => out.children[i].1.clone(),
                    (None, None) => {
                        Resolved { chain: Vec::new(), components: Vec::new(), children: Vec::new() }
                    }
                };
                patch_components(&mut node.components, &child.components);
                match existing {
                    Some(i) => out.children[i].1 = node,
                    None => out.children.push((child.slot.clone(), node)),
                }
            }
        }
        nesting.pop();
        Ok(out)
    }
}

fn check_component(world: &World, name: &str) -> Result<(), String> {
    if BOOKKEEPING.contains(&name) {
        return Err(format!("{name} is managed by the engine and cannot be set by a prefab"));
    }
    if world.text_ops(name).is_none() {
        return Err(format!("unknown component {name}"));
    }
    Ok(())
}

impl World {
    /// Spawn an instance of a prefab (an instance of that class) with its children.
    /// Nothing is left behind if the prefab is invalid.
    pub fn instantiate(&mut self, library: &PrefabLibrary, name: &str) -> Result<Entity, String> {
        let node = library.resolve(name)?;
        let mut spawned = Vec::new();
        match self.spawn_node(&node, None, None, &mut spawned) {
            Ok(root) => Ok(root),
            Err(message) => {
                for e in spawned.into_iter().rev() {
                    self.despawn(e);
                }
                Err(message)
            }
        }
    }

    fn spawn_node(
        &mut self,
        node: &Resolved,
        slot: Option<&str>,
        parent: Option<Entity>,
        spawned: &mut Vec<Entity>,
    ) -> Result<Entity, String> {
        for (name, _) in &node.components {
            check_component(self, name)?;
        }
        let entity = self.spawn(());
        spawned.push(entity);
        for (name, value) in &node.components {
            self.set_component_value(entity, name, value).map_err(|e| format!("{name}: {e}"))?;
        }
        if let Some(prefab) = node.chain.last() {
            self.insert(entity, PrefabInstance(prefab.clone()));
        }
        if let Some(slot) = slot {
            self.insert(entity, PrefabSlot(slot.to_string()));
        }
        if let Some(parent) = parent {
            self.set_parent(entity, Some(parent)).map_err(|e| format!("{e:?}"))?;
        }
        for (child_slot, child) in &node.children {
            self.spawn_node(child, Some(child_slot), Some(entity), spawned)?;
        }
        Ok(entity)
    }

    /// What this entity should look like according to its prefabs, or None if
    /// it does not come from one.
    pub fn prefab_expectation(
        &self,
        library: &PrefabLibrary,
        entity: Entity,
    ) -> Result<Option<Resolved>, String> {
        if let (Some(slot), Some(parent)) = (self.get::<PrefabSlot>(entity), self.parent(entity)) {
            if let Some(parent_node) = self.prefab_expectation(library, parent)? {
                if let Some((_, node)) =
                    parent_node.children.into_iter().find(|(s, _)| *s == slot.0)
                {
                    return Ok(Some(node));
                }
            }
        }
        match self.get::<PrefabInstance>(entity) {
            Some(instance) => library.resolve(&instance.0).map(Some),
            None => Ok(None),
        }
    }

    /// This entity's changes relative to its prefab, as a component patch
    /// (null marks a removed component). None if it does not come from a prefab.
    pub fn instance_overrides(
        &self,
        library: &PrefabLibrary,
        entity: Entity,
    ) -> Result<Option<SealValue>, String> {
        let Some(node) = self.prefab_expectation(library, entity)? else {
            return Ok(None);
        };
        let values = self.entity_values(entity).ok_or("no such entity")?;
        Ok(Some(overrides_against(
            self,
            &node,
            values.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
        )?))
    }

    /// Put an instance's component back to what its prefab says.
    pub fn revert_to_prefab(
        &mut self,
        library: &PrefabLibrary,
        entity: Entity,
        component: &str,
    ) -> Result<(), String> {
        let node = self.prefab_expectation(library, entity)?.ok_or("not a prefab instance")?;
        match node.components.iter().find(|(n, _)| n == component) {
            Some((_, value)) => self.set_component_value(entity, component, value),
            None => self.remove_component_named(entity, component).map(|_| ()),
        }
    }

    /// Save a scene where prefab instances store only their overrides.
    pub fn save_scene_compact(
        &self,
        library: &PrefabLibrary,
        vocabulary: Vocabulary,
    ) -> Result<String, String> {
        let (mut scene, list, _) = self.scene_value(None);
        let entries = scene.as_map_mut().expect("scene is a map");
        let rows = entries
            .iter_mut()
            .find(|(k, _)| k == "entities")
            .map(|(_, v)| v)
            .expect("scene has entities");
        let SealValue::Array(rows) = rows else { unreachable!() };
        for (row, &entity) in rows.iter_mut().zip(&list) {
            let Some(node) = self.prefab_expectation(library, entity)? else {
                continue;
            };
            let row_entries = row.as_map_mut().expect("row is a map");
            let (_, components) = row_entries
                .iter_mut()
                .find(|(k, _)| k == "components")
                .expect("row has components");
            let full = components.as_map().expect("map").to_vec();
            let mut compact: Vec<(String, SealValue)> =
                full.iter().filter(|(k, _)| BOOKKEEPING.contains(&k.as_str())).cloned().collect();
            let rest =
                full.into_iter().filter(|(k, _)| !BOOKKEEPING.contains(&k.as_str())).collect();
            if let Some(map) = overrides_against(self, &node, rest)?.as_map() {
                compact.extend(map.iter().cloned());
            }
            compact.sort_by(|a, b| a.0.cmp(&b.0));
            *components = SealValue::Map(SealMap::new(compact));
        }
        let text = scene.to_text();
        Ok(match vocabulary {
            Vocabulary::SceneGraph => text,
            Vocabulary::Classic => to_classic(&text)?,
        })
    }

    /// Load a scene saved in full or compact form, in either vocabulary.
    /// Instances are rebuilt from their prefabs plus overrides, and slots the
    /// prefab gained since the scene was saved are spawned.
    pub fn load_scene_with(
        &mut self,
        library: &PrefabLibrary,
        text: &str,
    ) -> Result<Vec<Entity>, String> {
        let mut scene = parse(text).map_err(|e| e.to_string())?;
        if detect(&scene) == Vocabulary::Classic {
            scene = parse(&to_scene_graph(text)?).map_err(|e| e.to_string())?;
        }
        // Index rows by file id so each row can find its parent's prefab.
        let rows: Vec<SealValue> = match scene.get("entities") {
            Some(SealValue::Array(rows)) => rows.clone(),
            _ => return Err("scene has no entities list".into()),
        };
        let mut by_id: HashMap<String, usize> = HashMap::new();
        for (i, row) in rows.iter().enumerate() {
            if let Some(SealValue::Number(id)) = row.get("id") {
                by_id.insert(id.clone(), i);
            }
        }
        let mut memo: HashMap<usize, Option<Resolved>> = HashMap::new();
        let mut expanded = rows.clone();
        for i in 0..rows.len() {
            let Some(node) = row_expectation(library, &rows, &by_id, i, &mut memo, 0)? else {
                continue;
            };
            let patch =
                rows[i].get("components").and_then(SealValue::as_map).unwrap_or(&[]).to_vec();
            let mut components = node.components.clone();
            for (name, _) in &components {
                check_component(self, name)?;
            }
            let (bookkeeping, rest): (Vec<_>, Vec<_>) =
                patch.into_iter().partition(|(k, _)| BOOKKEEPING.contains(&k.as_str()));
            patch_components(&mut components, &rest);
            components.extend(bookkeeping);
            expanded[i].insert("components", SealValue::Map(SealMap::new(components)));
        }
        scene.insert("entities", SealValue::Array(expanded));
        let spawned = self.load_scene(&scene.to_text())?;
        // Spawn slots the prefab gained since this scene was saved.
        let mut added = Vec::new();
        for (i, &entity) in spawned.iter().enumerate() {
            let Some(Some(node)) = memo.get(&i) else { continue };
            let present: Vec<String> = self
                .children(entity)
                .iter()
                .filter_map(|c| self.get::<PrefabSlot>(*c).map(|s| s.0.clone()))
                .collect();
            for (slot, child) in &node.children {
                if !present.contains(slot) {
                    if let Err(message) =
                        self.spawn_node(child, Some(slot), Some(entity), &mut added)
                    {
                        for e in added.iter().chain(spawned.iter()).rev() {
                            self.despawn(*e);
                        }
                        return Err(message);
                    }
                }
            }
        }
        Ok(spawned.into_iter().chain(added).collect())
    }
}

fn overrides_against(
    world: &World,
    node: &Resolved,
    current: Vec<(String, SealValue)>,
) -> Result<SealValue, String> {
    let mut out = Vec::new();
    for (name, value) in &current {
        if BOOKKEEPING.contains(&name.as_str()) {
            continue;
        }
        match node.components.iter().find(|(n, _)| n == name) {
            Some((_, expected)) => {
                let ops = world.text_ops(name).ok_or(format!("unknown component {name}"))?;
                let expected = (ops.canonical)(expected)?;
                if let Some(changed) = diff_patch(&expected, value) {
                    out.push((name.clone(), changed));
                }
            }
            None => out.push((name.clone(), value.clone())),
        }
    }
    for (name, _) in &node.components {
        if !current.iter().any(|(n, _)| n == name) {
            out.push((name.clone(), SealValue::Null));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(SealValue::Map(SealMap::new(out)))
}

fn row_expectation(
    library: &PrefabLibrary,
    rows: &[SealValue],
    by_id: &HashMap<String, usize>,
    i: usize,
    memo: &mut HashMap<usize, Option<Resolved>>,
    depth: usize,
) -> Result<Option<Resolved>, String> {
    if let Some(done) = memo.get(&i) {
        return Ok(done.clone());
    }
    if depth > 1024 {
        return Err("child_of links loop".into());
    }
    let components = rows[i].get("components");
    let get = |key: &str| components.and_then(|c| c.get(key));
    let mut result = None;
    if let (Some(SealValue::String(slot)), Some(SealValue::Number(parent))) =
        (get("prefab_slot"), get("child_of"))
    {
        if let Some(&p) = by_id.get(parent) {
            if let Some(parent_node) = row_expectation(library, rows, by_id, p, memo, depth + 1)? {
                result = parent_node.children.into_iter().find(|(s, _)| s == slot).map(|(_, n)| n);
            }
        }
    }
    if result.is_none() {
        if let Some(SealValue::String(prefab)) = get("prefab") {
            result = Some(library.resolve(prefab)?);
        }
    }
    memo.insert(i, result.clone());
    Ok(result)
}
