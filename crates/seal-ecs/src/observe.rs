//! Reacting to changes: component hooks and event observers.
//!
//! * `on_add::<T>` runs right after `T` is added to an entity (spawn or insert,
//!   not when an existing value is replaced).
//! * `on_remove::<T>` runs right before `T` leaves an entity (remove or
//!   despawn), while the value can still be read.
//! * `observe::<E>` runs whenever `trigger(E)` is called.
//!
//! Callbacks run immediately, in registration order, and may change the
//! world freely, including adding more callbacks. Snapshot restores and
//! scene merges do not run them: they replace the world wholesale.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

use crate::world::{Entity, World};
use crate::Component;

type EntityCallback = Arc<dyn Fn(&mut World, Entity) + Send + Sync>;
type EventCallback = Arc<dyn Fn(&mut World, &dyn Any) + Send + Sync>;

/// Returned when registering a callback; pass it to [`World::unobserve`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ObserverId(u64);

#[derive(Default)]
pub(crate) struct Hooks {
    add: HashMap<TypeId, Vec<(ObserverId, EntityCallback)>>,
    remove: HashMap<TypeId, Vec<(ObserverId, EntityCallback)>>,
    events: HashMap<TypeId, Vec<(ObserverId, EventCallback)>>,
    next: u64,
}

impl Hooks {
    fn id(&mut self) -> ObserverId {
        self.next += 1;
        ObserverId(self.next)
    }

    pub fn has_add(&self, type_id: TypeId) -> bool {
        self.add.get(&type_id).is_some_and(|v| !v.is_empty())
    }

    pub fn has_remove(&self, type_id: TypeId) -> bool {
        self.remove.get(&type_id).is_some_and(|v| !v.is_empty())
    }
}

impl World {
    pub fn on_add<T: Component>(
        &mut self,
        f: impl Fn(&mut World, Entity) + Send + Sync + 'static,
    ) -> ObserverId {
        let id = self.hooks.id();
        self.hooks.add.entry(TypeId::of::<T>()).or_default().push((id, Arc::new(f)));
        id
    }

    pub fn on_remove<T: Component>(
        &mut self,
        f: impl Fn(&mut World, Entity) + Send + Sync + 'static,
    ) -> ObserverId {
        let id = self.hooks.id();
        self.hooks.remove.entry(TypeId::of::<T>()).or_default().push((id, Arc::new(f)));
        id
    }

    /// Run `f` for every `trigger` of an `E`.
    pub fn observe<E: 'static>(
        &mut self,
        f: impl Fn(&mut World, &E) + Send + Sync + 'static,
    ) -> ObserverId {
        let id = self.hooks.id();
        let erased: EventCallback = Arc::new(move |world, event| {
            f(world, event.downcast_ref::<E>().expect("keyed by type"))
        });
        self.hooks.events.entry(TypeId::of::<E>()).or_default().push((id, erased));
        id
    }

    /// Deliver `event` to every observer of its type. Returns how many ran.
    pub fn trigger<E: 'static>(&mut self, event: E) -> usize {
        let callbacks: Vec<EventCallback> = self
            .hooks
            .events
            .get(&TypeId::of::<E>())
            .map_or(vec![], |v| v.iter().map(|(_, f)| f.clone()).collect());
        for f in &callbacks {
            f(self, &event);
        }
        callbacks.len()
    }

    /// Stop a hook or observer. Returns false if it was already gone.
    pub fn unobserve(&mut self, id: ObserverId) -> bool {
        let mut found = false;
        for list in self.hooks.add.values_mut().chain(self.hooks.remove.values_mut()) {
            let before = list.len();
            list.retain(|(i, _)| *i != id);
            found |= list.len() != before;
        }
        for list in self.hooks.events.values_mut() {
            let before = list.len();
            list.retain(|(i, _)| *i != id);
            found |= list.len() != before;
        }
        found
    }

    pub(crate) fn fire_add(&mut self, entity: Entity, type_id: TypeId) {
        if !self.hooks.has_add(type_id) {
            return;
        }
        let callbacks: Vec<EntityCallback> =
            self.hooks.add[&type_id].iter().map(|(_, f)| f.clone()).collect();
        for f in callbacks {
            if !self.contains(entity) {
                break;
            }
            f(self, entity);
        }
    }

    pub(crate) fn fire_remove(&mut self, entity: Entity, type_id: TypeId) {
        if !self.hooks.has_remove(type_id) {
            return;
        }
        let callbacks: Vec<EntityCallback> =
            self.hooks.remove[&type_id].iter().map(|(_, f)| f.clone()).collect();
        for f in callbacks {
            if !self.contains(entity) {
                break;
            }
            f(self, entity);
        }
    }

    /// Before a despawn: run remove hooks for every component the entity has.
    pub(crate) fn fire_remove_all(&mut self, entity: Entity) {
        if self.hooks.remove.is_empty() {
            return;
        }
        let Some((table, _)) = self.location(entity) else {
            return;
        };
        let types: Vec<TypeId> = self.tables[table]
            .types
            .iter()
            .copied()
            .filter(|t| self.hooks.has_remove(*t))
            .collect();
        for type_id in types {
            if !self.contains(entity) {
                return;
            }
            let still_has = self.location(entity).is_some_and(|(t, _)| self.tables[t].has(type_id));
            if still_has {
                self.fire_remove(entity, type_id);
            }
        }
    }
}
