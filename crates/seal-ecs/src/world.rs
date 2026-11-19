//! Entities, archetype tables and the world that owns them.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::fmt;

use crate::column::{Column, ColumnInfo, ColumnOf, ComponentTicks};
use crate::{Bundle, Component};

/// A 64-bit generational entity id.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Entity {
    index: u32,
    generation: u32,
}

impl Entity {
    pub fn index(self) -> u32 {
        self.index
    }

    pub fn generation(self) -> u32 {
        self.generation
    }

    /// Generation in the high 32 bits, slot index in the low 32 bits.
    pub fn to_bits(self) -> u64 {
        (u64::from(self.generation) << 32) | u64::from(self.index)
    }

    pub fn from_bits(bits: u64) -> Self {
        Self { index: bits as u32, generation: (bits >> 32) as u32 }
    }
}

impl Entity {
    /// Read the `5v1` form that `Debug` prints.
    pub fn parse(text: &str) -> Option<Entity> {
        let (index, generation) = text.trim().split_once('v')?;
        Some(Entity { index: index.parse().ok()?, generation: generation.parse().ok()? })
    }
}

impl fmt::Debug for Entity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}v{}", self.index, self.generation)
    }
}

/// All entities with exactly one set of component types.
pub struct Table {
    /// Sorted, so a set of types always maps to the same table.
    pub(crate) types: Vec<TypeId>,
    pub(crate) infos: Vec<ColumnInfo>,
    pub(crate) columns: Vec<Box<dyn Column>>,
    pub(crate) entities: Vec<Entity>,
}

impl Table {
    fn new(mut infos: Vec<ColumnInfo>) -> Self {
        infos.sort_by_key(|i| i.type_id);
        infos.dedup_by_key(|i| i.type_id);
        Self {
            types: infos.iter().map(|i| i.type_id).collect(),
            columns: infos.iter().map(|i| (i.make)()).collect(),
            infos,
            entities: Vec::new(),
        }
    }

    pub(crate) fn column_index(&self, type_id: TypeId) -> Option<usize> {
        self.types.binary_search(&type_id).ok()
    }

    pub(crate) fn has(&self, type_id: TypeId) -> bool {
        self.column_index(type_id).is_some()
    }

    pub(crate) fn column_of<T: Component>(&self) -> Option<&ColumnOf<T>> {
        let index = self.column_index(TypeId::of::<T>())?;
        self.columns[index].as_any().downcast_ref()
    }

    pub(crate) fn column_of_mut<T: Component>(&mut self) -> Option<&mut ColumnOf<T>> {
        let index = self.column_index(TypeId::of::<T>())?;
        self.columns[index].as_any_mut().downcast_mut()
    }

    pub fn len(&self) -> usize {
        self.entities.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Slot {
    pub(crate) generation: u32,
    pub(crate) location: Option<(u32, u32)>,
}

type RequiredHook = fn(&mut World, Entity);

/// Owns entities, their components and singleton resources.
pub struct World {
    pub(crate) slots: Vec<Slot>,
    pub(crate) free: Vec<u32>,
    pub(crate) tables: Vec<Table>,
    pub(crate) table_by_types: HashMap<Vec<TypeId>, usize>,
    pub(crate) tick: u32,
    resources: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
    required: HashMap<TypeId, Vec<RequiredHook>>,
    pub(crate) live: usize,
    pub(crate) registry: crate::registry::Registry,
    pub(crate) hooks: crate::observe::Hooks,
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl World {
    pub fn new() -> Self {
        let mut world = Self {
            slots: Vec::new(),
            free: Vec::new(),
            tables: Vec::new(),
            table_by_types: HashMap::new(),
            tick: 1,
            resources: HashMap::new(),
            required: HashMap::new(),
            live: 0,
            registry: crate::registry::Registry::default(),
            hooks: crate::observe::Hooks::default(),
        };
        // Table 0 holds entities with no components.
        world.table_for(Vec::new());
        world.register_text::<crate::hierarchy::ChildOf>();
        world.register_text::<crate::text::SceneId>();
        world.register_text::<crate::prefab::PrefabInstance>();
        world.register_text::<crate::prefab::PrefabSlot>();
        world.register_clone::<crate::hierarchy::Children>();
        world
    }

    /// The tick that writes are stamped with now.
    pub fn tick(&self) -> u32 {
        self.tick
    }

    /// Move to the next tick and return it. Call once per system run or frame.
    pub fn advance_tick(&mut self) -> u32 {
        self.tick = self.tick.wrapping_add(1).max(1);
        self.tick
    }

    pub fn len(&self) -> usize {
        self.live
    }

    pub fn is_empty(&self) -> bool {
        self.live == 0
    }

    /// How many archetype tables exist (including the empty one).
    pub fn table_count(&self) -> usize {
        self.tables.len()
    }

    pub(crate) fn table_for(&mut self, infos: Vec<ColumnInfo>) -> usize {
        let mut key: Vec<TypeId> = infos.iter().map(|i| i.type_id).collect();
        key.sort();
        key.dedup();
        if let Some(&index) = self.table_by_types.get(&key) {
            return index;
        }
        let index = self.tables.len();
        self.tables.push(Table::new(infos));
        self.table_by_types.insert(key, index);
        index
    }

    pub(crate) fn alloc_entity(&mut self) -> Entity {
        self.live += 1;
        match self.free.pop() {
            Some(index) => Entity { index, generation: self.slots[index as usize].generation },
            None => {
                let index = u32::try_from(self.slots.len()).expect("fewer than 2^32 entity slots");
                self.slots.push(Slot { generation: 0, location: None });
                Entity { index, generation: 0 }
            }
        }
    }

    pub(crate) fn location(&self, entity: Entity) -> Option<(usize, usize)> {
        let slot = self.slots.get(entity.index as usize)?;
        if slot.generation != entity.generation {
            return None;
        }
        slot.location.map(|(t, r)| (t as usize, r as usize))
    }

    pub(crate) fn set_location(&mut self, entity: Entity, table: usize, row: usize) {
        self.slots[entity.index as usize].location = Some((table as u32, row as u32));
    }

    pub fn contains(&self, entity: Entity) -> bool {
        self.location(entity).is_some()
    }

    /// Every live entity, ordered by index.
    pub fn entities(&self) -> Vec<Entity> {
        let mut all: Vec<Entity> =
            self.tables.iter().flat_map(|t| t.entities.iter().copied()).collect();
        all.sort_by_key(|e| e.index);
        all
    }

    /// Spawn an entity with the components in `bundle`.
    pub fn spawn<B: Bundle>(&mut self, bundle: B) -> Entity {
        let mut infos = Vec::new();
        B::describe(&mut infos);
        let table_index = self.table_for(infos.clone());
        let entity = self.alloc_entity();
        let table = &mut self.tables[table_index];
        bundle.push_into(table, self.tick);
        table.entities.push(entity);
        let row = table.entities.len() - 1;
        self.set_location(entity, table_index, row);
        for info in infos {
            self.apply_required(entity, info.type_id);
            self.fire_add(entity, info.type_id);
        }
        entity
    }

    /// Spawn many entities with the same component set in one table move.
    pub fn spawn_batch<B: Bundle>(&mut self, bundles: impl IntoIterator<Item = B>) -> Vec<Entity> {
        let mut infos = Vec::new();
        B::describe(&mut infos);
        let table_index = self.table_for(infos.clone());
        let mut spawned = Vec::new();
        for bundle in bundles {
            let entity = self.alloc_entity();
            let table = &mut self.tables[table_index];
            bundle.push_into(table, self.tick);
            table.entities.push(entity);
            let row = table.entities.len() - 1;
            self.set_location(entity, table_index, row);
            spawned.push(entity);
        }
        if infos
            .iter()
            .any(|i| self.required.contains_key(&i.type_id) || self.hooks.has_add(i.type_id))
        {
            for &entity in &spawned {
                for info in &infos {
                    self.apply_required(entity, info.type_id);
                    self.fire_add(entity, info.type_id);
                }
            }
        }
        spawned
    }

    /// Remove an entity and drop its components. Returns false for a stale id.
    pub fn despawn(&mut self, entity: Entity) -> bool {
        if !self.contains(entity) {
            return false;
        }
        self.fire_remove_all(entity);
        if !self.contains(entity) {
            return false; // a remove hook despawned it already
        }
        self.detach_for_despawn(entity);
        let Some((table_index, row)) = self.location(entity) else {
            return false;
        };
        let table = &mut self.tables[table_index];
        for column in &mut table.columns {
            column.swap_remove_drop(row);
        }
        table.entities.swap_remove(row);
        if let Some(&moved) = table.entities.get(row) {
            self.set_location(moved, table_index, row);
        }
        let slot = &mut self.slots[entity.index as usize];
        slot.location = None;
        slot.generation = slot.generation.wrapping_add(1);
        self.free.push(entity.index);
        self.live -= 1;
        true
    }

    pub fn get<T: Component>(&self, entity: Entity) -> Option<&T> {
        let (table, row) = self.location(entity)?;
        self.tables[table].column_of::<T>().map(|c| &c.data[row])
    }

    /// Mutable access; stamps the component as changed at the current tick.
    pub fn get_mut<T: Component>(&mut self, entity: Entity) -> Option<&mut T> {
        let (table, row) = self.location(entity)?;
        let tick = self.tick;
        let column = self.tables[table].column_of_mut::<T>()?;
        column.changed[row] = tick;
        Some(&mut column.data[row])
    }

    pub fn ticks<T: Component>(&self, entity: Entity) -> Option<ComponentTicks> {
        let (table, row) = self.location(entity)?;
        let index = self.tables[table].column_index(TypeId::of::<T>())?;
        Some(self.tables[table].columns[index].ticks(row))
    }

    pub fn has<T: Component>(&self, entity: Entity) -> bool {
        self.location(entity).is_some_and(|(t, _)| self.tables[t].has(TypeId::of::<T>()))
    }

    /// Move an entity's row to `dst_table`. Columns missing in the destination are dropped.
    fn move_entity(
        &mut self,
        entity: Entity,
        src_index: usize,
        row: usize,
        dst_index: usize,
    ) -> usize {
        debug_assert_ne!(src_index, dst_index);
        let (src, dst) = if src_index < dst_index {
            let (low, high) = self.tables.split_at_mut(dst_index);
            (&mut low[src_index], &mut high[0])
        } else {
            let (low, high) = self.tables.split_at_mut(src_index);
            (&mut high[0], &mut low[dst_index])
        };
        for (column_index, type_id) in src.types.iter().enumerate() {
            match dst.column_index(*type_id) {
                Some(dst_column) => {
                    src.columns[column_index].move_row(row, dst.columns[dst_column].as_mut())
                }
                None => src.columns[column_index].swap_remove_drop(row),
            }
        }
        src.entities.swap_remove(row);
        let moved = src.entities.get(row).copied();
        dst.entities.push(entity);
        let new_row = dst.entities.len() - 1;
        if let Some(moved) = moved {
            self.set_location(moved, src_index, row);
        }
        self.set_location(entity, dst_index, new_row);
        new_row
    }

    /// Add or replace a component. Returns false for a stale id.
    pub fn insert<T: Component>(&mut self, entity: Entity, value: T) -> bool {
        let Some((table_index, row)) = self.location(entity) else {
            return false;
        };
        let tick = self.tick;
        if let Some(column) = self.tables[table_index].column_of_mut::<T>() {
            column.data[row] = value;
            column.changed[row] = tick;
            return true;
        }
        let mut infos = self.tables[table_index].infos.clone();
        infos.push(ColumnInfo::of::<T>());
        let dst_index = self.table_for(infos);
        self.move_entity(entity, table_index, row, dst_index);
        self.tables[dst_index].column_of_mut::<T>().expect("new column").push(value, tick);
        self.apply_required(entity, TypeId::of::<T>());
        self.fire_add(entity, TypeId::of::<T>());
        true
    }

    /// Remove a component and return it.
    pub fn remove<T: Component>(&mut self, entity: Entity) -> Option<T> {
        let (table_index, _) = self.location(entity)?;
        let type_id = TypeId::of::<T>();
        if !self.tables[table_index].has(type_id) {
            return None;
        }
        if self.hooks.has_remove(type_id) {
            self.fire_remove(entity, type_id);
            // The hook may have changed the entity; start again from its current state.
            if !self.has::<T>(entity) {
                return None;
            }
            return self.remove_now::<T>(entity);
        }
        self.remove_now::<T>(entity)
    }

    fn remove_now<T: Component>(&mut self, entity: Entity) -> Option<T> {
        let (table_index, row) = self.location(entity)?;
        let type_id = TypeId::of::<T>();
        let infos: Vec<ColumnInfo> = self.tables[table_index]
            .infos
            .iter()
            .copied()
            .filter(|i| i.type_id != type_id)
            .collect();
        let dst_index = self.table_for(infos);
        // Take T out with a swap-remove; every other column then swap-removes
        // the same row while moving it, so all columns stay aligned.
        let (value, _) =
            self.tables[table_index].column_of_mut::<T>().expect("checked above").swap_remove(row);
        let dst = dst_index;
        let (src, dst_table) = if table_index < dst {
            let (low, high) = self.tables.split_at_mut(dst);
            (&mut low[table_index], &mut high[0])
        } else {
            let (low, high) = self.tables.split_at_mut(table_index);
            (&mut high[0], &mut low[dst])
        };
        for (column_index, tid) in src.types.iter().enumerate() {
            if *tid == type_id {
                continue;
            }
            let dst_column = dst_table.column_index(*tid).expect("all other types are kept");
            src.columns[column_index].move_row(row, dst_table.columns[dst_column].as_mut());
        }
        src.entities.swap_remove(row);
        let moved = src.entities.get(row).copied();
        dst_table.entities.push(entity);
        let new_row = dst_table.entities.len() - 1;
        if let Some(moved) = moved {
            self.set_location(moved, table_index, row);
        }
        self.set_location(entity, dst_index, new_row);
        Some(value)
    }

    /// Whenever `A` is added to an entity without `B`, add `B::default()` too.
    pub fn require<A: Component, B: Component + Default>(&mut self) {
        fn add_default<B: Component + Default>(world: &mut World, entity: Entity) {
            if !world.has::<B>(entity) {
                world.insert(entity, B::default());
            }
        }
        self.required.entry(TypeId::of::<A>()).or_default().push(add_default::<B>);
    }

    fn apply_required(&mut self, entity: Entity, type_id: TypeId) {
        if let Some(hooks) = self.required.get(&type_id).cloned() {
            for hook in hooks {
                hook(self, entity);
            }
        }
    }

    pub fn insert_resource<R: Component>(&mut self, value: R) {
        self.resources.insert(TypeId::of::<R>(), Box::new(value));
    }

    pub fn resource<R: Component>(&self) -> Option<&R> {
        self.resources.get(&TypeId::of::<R>())?.downcast_ref()
    }

    pub fn resource_mut<R: Component>(&mut self) -> Option<&mut R> {
        self.resources.get_mut(&TypeId::of::<R>())?.downcast_mut()
    }

    pub fn remove_resource<R: Component>(&mut self) -> Option<R> {
        self.resources.remove(&TypeId::of::<R>())?.downcast().ok().map(|b| *b)
    }

    /// Entities whose `T` changed after tick `since`, in table and row order.
    pub fn changed_since<T: Component>(&self, since: u32) -> Vec<Entity> {
        let mut out = Vec::new();
        for table in &self.tables {
            if let Some(column) = table.column_of::<T>() {
                for (row, &changed) in column.changed.iter().enumerate() {
                    if changed > since {
                        out.push(table.entities[row]);
                    }
                }
            }
        }
        out
    }

    /// Names of the component types on an entity, for tools and debugging.
    pub fn component_names(&self, entity: Entity) -> Vec<&'static str> {
        self.location(entity)
            .map(|(t, _)| self.tables[t].infos.iter().map(|i| i.name).collect())
            .unwrap_or_default()
    }

    /// Check that every table's columns and entity list agree and every
    /// live entity points at its row. Used by tests and fuzzing.
    pub fn check_integrity(&self) -> Result<(), String> {
        let mut live = 0;
        for (t, table) in self.tables.iter().enumerate() {
            for column in &table.columns {
                if column.len() != table.entities.len() {
                    return Err(format!(
                        "table {t}: column has {} rows, table {}",
                        column.len(),
                        table.entities.len()
                    ));
                }
            }
            for (row, &entity) in table.entities.iter().enumerate() {
                if self.location(entity) != Some((t, row)) {
                    return Err(format!(
                        "{entity:?} listed in table {t} row {row} but located elsewhere"
                    ));
                }
                live += 1;
            }
        }
        if live != self.live {
            return Err(format!("{live} entities in tables, {} counted live", self.live));
        }
        Ok(())
    }
}
