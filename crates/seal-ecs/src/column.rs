//! Type-erased component columns.

use std::any::{Any, TypeId};

/// When a component was added and last changed, in world ticks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComponentTicks {
    pub added: u32,
    pub changed: u32,
}

impl ComponentTicks {
    /// Changed after `since` (a tick a system saw last time it ran).
    pub fn changed_since(self, since: u32) -> bool {
        self.changed > since
    }

    pub fn added_since(self, since: u32) -> bool {
        self.added > since
    }
}

/// How to make an empty column for a component type.
#[derive(Clone, Copy)]
pub struct ColumnInfo {
    pub type_id: TypeId,
    pub name: &'static str,
    pub make: fn() -> Box<dyn Column>,
}

impl ColumnInfo {
    pub fn of<T: crate::Component>() -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            name: std::any::type_name::<T>(),
            make: || Box::new(ColumnOf::<T>::default()),
        }
    }
}

pub trait Column: Any + Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn len(&self) -> usize;
    fn ticks(&self, row: usize) -> ComponentTicks;
    /// Remove `row` (the last row moves into it) and drop the value.
    fn swap_remove_drop(&mut self, row: usize);
    /// Remove `row` (the last row moves into it) and push it, with its ticks, onto `dst`.
    fn move_row(&mut self, row: usize, dst: &mut dyn Column);
}

pub struct ColumnOf<T> {
    pub data: Vec<T>,
    pub added: Vec<u32>,
    pub changed: Vec<u32>,
}

impl<T> Default for ColumnOf<T> {
    fn default() -> Self {
        Self { data: Vec::new(), added: Vec::new(), changed: Vec::new() }
    }
}

impl<T: crate::Component> ColumnOf<T> {
    pub fn push(&mut self, value: T, tick: u32) {
        self.data.push(value);
        self.added.push(tick);
        self.changed.push(tick);
    }

    pub fn swap_remove(&mut self, row: usize) -> (T, ComponentTicks) {
        let ticks = ComponentTicks {
            added: self.added.swap_remove(row),
            changed: self.changed.swap_remove(row),
        };
        (self.data.swap_remove(row), ticks)
    }
}

impl<T: crate::Component> Column for ColumnOf<T> {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn len(&self) -> usize {
        self.data.len()
    }
    fn ticks(&self, row: usize) -> ComponentTicks {
        ComponentTicks { added: self.added[row], changed: self.changed[row] }
    }
    fn swap_remove_drop(&mut self, row: usize) {
        self.swap_remove(row);
    }
    fn move_row(&mut self, row: usize, dst: &mut dyn Column) {
        let (value, ticks) = self.swap_remove(row);
        let dst = dst.as_any_mut().downcast_mut::<ColumnOf<T>>().expect("columns of the same type");
        dst.data.push(value);
        dst.added.push(ticks.added);
        dst.changed.push(ticks.changed);
    }
}
