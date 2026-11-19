//! User-defined relationships between entities, such as "owns", "attached
//! to" or "targets". A relationship kind is a marker type implementing
//! [`Relation`]. The source holds [`Targets<R>`] and each target holds
//! [`Sources<R>`]; the world keeps both sides in step, including when
//! either end is despawned.

use std::marker::PhantomData;

use crate::world::{Entity, World};

/// A kind of relationship between two entities.
pub trait Relation: Send + Sync + 'static {
    /// Despawn the targets when their source is despawned (ownership).
    const DESPAWN_TARGETS_WITH_SOURCE: bool = false;
    /// A source may have at most one target of this kind; relating again replaces it.
    const EXCLUSIVE: bool = false;
}

/// The entities this one points at through `R`, in the order they were added.
pub struct Targets<R: Relation>(Vec<Entity>, PhantomData<R>);
/// The entities pointing at this one through `R`.
pub struct Sources<R: Relation>(Vec<Entity>, PhantomData<R>);

impl<R: Relation> Clone for Targets<R> {
    fn clone(&self) -> Self {
        Self(self.0.clone(), PhantomData)
    }
}
impl<R: Relation> Clone for Sources<R> {
    fn clone(&self) -> Self {
        Self(self.0.clone(), PhantomData)
    }
}
impl<R: Relation> Targets<R> {
    pub fn as_slice(&self) -> &[Entity] {
        &self.0
    }
}
impl<R: Relation> Sources<R> {
    pub fn as_slice(&self) -> &[Entity] {
        &self.0
    }
}

impl World {
    /// Make `R` usable. Safe to call more than once.
    pub fn register_relation<R: Relation>(&mut self) {
        if self.registered_relation::<R>() {
            return;
        }
        self.register_clone::<Targets<R>>();
        self.register_clone::<Sources<R>>();
        self.insert_resource(RelationRegistered::<R>(PhantomData));
        // A dying source leaves its targets' Sources lists (or takes the targets with it).
        self.on_remove::<Targets<R>>(|world, source| {
            let targets = world.get::<Targets<R>>(source).map(|t| t.0.clone()).unwrap_or_default();
            for target in targets {
                if R::DESPAWN_TARGETS_WITH_SOURCE {
                    world.despawn(target);
                } else {
                    world.drop_from::<Sources<R>>(target, source, |s| &mut s.0);
                }
            }
        });
        // A dying target leaves its sources' Targets lists.
        self.on_remove::<Sources<R>>(|world, target| {
            let sources = world.get::<Sources<R>>(target).map(|s| s.0.clone()).unwrap_or_default();
            for source in sources {
                world.drop_from::<Targets<R>>(source, target, |t| &mut t.0);
            }
        });
    }

    fn registered_relation<R: Relation>(&self) -> bool {
        self.resource::<RelationRegistered<R>>().is_some()
    }

    fn drop_from<C: crate::Component>(
        &mut self,
        holder: Entity,
        gone: Entity,
        list: fn(&mut C) -> &mut Vec<Entity>,
    ) {
        let empty = match self.get_mut::<C>(holder) {
            Some(c) => {
                let l = list(c);
                l.retain(|&e| e != gone);
                l.is_empty()
            }
            None => return,
        };
        if empty {
            self.remove::<C>(holder);
        }
    }

    /// Point `source` at `target` through `R`. Returns false if either id is stale.
    pub fn relate<R: Relation>(&mut self, source: Entity, target: Entity) -> bool {
        assert!(
            self.registered_relation::<R>(),
            "call register_relation::<{}>() first",
            std::any::type_name::<R>()
        );
        if !self.contains(source) || !self.contains(target) {
            return false;
        }
        if self.targets::<R>().of(self, source).contains(&target) {
            return true;
        }
        if R::EXCLUSIVE {
            for old in self.targets::<R>().of(self, source).to_vec() {
                self.unrelate::<R>(source, old);
            }
        }
        match self.get_mut::<Targets<R>>(source) {
            Some(t) => t.0.push(target),
            None => {
                self.insert(source, Targets::<R>(vec![target], PhantomData));
            }
        }
        match self.get_mut::<Sources<R>>(target) {
            Some(s) => s.0.push(source),
            None => {
                self.insert(target, Sources::<R>(vec![source], PhantomData));
            }
        }
        true
    }

    /// Remove one link. Returns false if it did not exist.
    pub fn unrelate<R: Relation>(&mut self, source: Entity, target: Entity) -> bool {
        if !self.targets::<R>().of(self, source).contains(&target) {
            return false;
        }
        // Each list shrinks before its component is removed, so the removal hooks find nothing left to undo.
        self.drop_from::<Targets<R>>(source, target, |t| &mut t.0);
        self.drop_from::<Sources<R>>(target, source, |s| &mut s.0);
        true
    }

    pub fn targets<R: Relation>(&self) -> RelationView<R> {
        RelationView(PhantomData)
    }
}

/// Read a relationship: `world.targets::<Owns>().of(&world, e)`.
pub struct RelationView<R>(PhantomData<R>);

impl<R: Relation> RelationView<R> {
    pub fn of<'w>(&self, world: &'w World, source: Entity) -> &'w [Entity] {
        world.get::<Targets<R>>(source).map_or(&[], |t| &t.0)
    }
    pub fn sources_of<'w>(&self, world: &'w World, target: Entity) -> &'w [Entity] {
        world.get::<Sources<R>>(target).map_or(&[], |s| &s.0)
    }
}

struct RelationRegistered<R>(PhantomData<R>);
