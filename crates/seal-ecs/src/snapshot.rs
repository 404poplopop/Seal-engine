//! Whole-world snapshots, transactions and undo.
//!
//! A snapshot copies every table, so taking one costs time and memory in
//! proportion to the world. That is fine for editor undo steps and for
//! transactions around tool actions; per-tick rollback for networking will
//! use change-tick deltas instead.

use std::collections::HashMap;

use crate::world::{Slot, Table, World};

/// A copy of a world's entities and components. Resources are not included.
pub struct Snapshot {
    slots: Vec<Slot>,
    free: Vec<u32>,
    tables: Vec<Table>,
    table_by_types: HashMap<Vec<std::any::TypeId>, usize>,
    tick: u32,
    live: usize,
}

impl Snapshot {
    pub fn entity_count(&self) -> usize {
        self.live
    }
}

/// Component types that cannot be copied because they were never registered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotError {
    pub unregistered: Vec<&'static str>,
}

impl std::fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "cannot snapshot components without register_clone: {}",
            self.unregistered.join(", ")
        )
    }
}

impl std::error::Error for SnapshotError {}

impl World {
    fn clone_tables(&self, tables: &[Table]) -> Result<Vec<Table>, SnapshotError> {
        let mut unregistered = Vec::new();
        for table in tables {
            for info in &table.infos {
                let ok =
                    self.registry.by_type.get(&info.type_id).is_some_and(|r| r.clone.is_some());
                if !ok && !unregistered.contains(&info.name) {
                    unregistered.push(info.name);
                }
            }
        }
        if !unregistered.is_empty() {
            return Err(SnapshotError { unregistered });
        }
        Ok(tables
            .iter()
            .map(|table| Table {
                types: table.types.clone(),
                infos: table.infos.clone(),
                columns: table
                    .infos
                    .iter()
                    .zip(&table.columns)
                    .map(|(info, column)| {
                        (self.registry.by_type[&info.type_id].clone.expect("checked"))(
                            column.as_ref(),
                        )
                    })
                    .collect(),
                entities: table.entities.clone(),
            })
            .collect())
    }

    /// Copy every entity and component.
    pub fn snapshot(&self) -> Result<Snapshot, SnapshotError> {
        Ok(Snapshot {
            slots: self.slots.clone(),
            free: self.free.clone(),
            tables: self.clone_tables(&self.tables)?,
            table_by_types: self.table_by_types.clone(),
            tick: self.tick,
            live: self.live,
        })
    }

    /// Put the world back exactly as it was when `snapshot` was taken.
    /// Entity ids taken before the snapshot keep working; ids made after it go stale.
    /// The tick keeps moving forward, so change detection still sees the restore.
    pub fn restore(&mut self, snapshot: &Snapshot) {
        self.tables =
            self.clone_tables(&snapshot.tables).expect("the snapshot was clonable when taken");
        self.table_by_types = snapshot.table_by_types.clone();
        self.free = snapshot.free.clone();
        self.live = snapshot.live;
        // Slots created after the snapshot are kept but emptied, with a newer
        // generation, so their ids stay stale and their indices are reused safely.
        let mut slots = snapshot.slots.clone();
        for (index, slot) in self.slots.iter().enumerate().skip(slots.len()) {
            slots.push(Slot { generation: slot.generation.wrapping_add(1), location: None });
            self.free.push(index as u32);
        }
        for (index, slot) in slots.iter_mut().enumerate().take(snapshot.slots.len()) {
            // A slot that was reused after the snapshot must not let the newer id resolve.
            let current = self.slots[index].generation;
            if slot.location.is_none() && current >= slot.generation {
                slot.generation = current.wrapping_add(1);
            }
        }
        self.slots = slots;
        self.tick = self.tick.max(snapshot.tick);
        self.advance_tick();
    }

    /// Run `body`; if it returns `Err`, put the world back as it was.
    pub fn transaction<R, E>(
        &mut self,
        body: impl FnOnce(&mut World) -> Result<R, E>,
    ) -> Result<Result<R, E>, SnapshotError> {
        let before = self.snapshot()?;
        let result = body(self);
        if result.is_err() {
            self.restore(&before);
        }
        Ok(result)
    }
}

/// Undo and redo by whole-world snapshots.
#[derive(Default)]
pub struct History {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    limit: usize,
}

impl History {
    /// Keep at most `limit` undo steps (0 means unlimited).
    pub fn with_limit(limit: usize) -> Self {
        Self { undo: Vec::new(), redo: Vec::new(), limit }
    }

    /// Call before an edit: remembers the world as it is now.
    pub fn record(&mut self, world: &World) -> Result<(), SnapshotError> {
        self.undo.push(world.snapshot()?);
        if self.limit > 0 && self.undo.len() > self.limit {
            self.undo.remove(0);
        }
        self.redo.clear();
        Ok(())
    }

    /// Remember a snapshot taken before an edit that has now succeeded.
    pub fn push(&mut self, before: Snapshot) {
        self.undo.push(before);
        if self.limit > 0 && self.undo.len() > self.limit {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self, world: &mut World) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        if let Ok(now) = world.snapshot() {
            self.redo.push(now);
        }
        world.restore(&previous);
        true
    }

    pub fn redo(&mut self, world: &mut World) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        if let Ok(now) = world.snapshot() {
            self.undo.push(now);
        }
        world.restore(&next);
        true
    }
}
