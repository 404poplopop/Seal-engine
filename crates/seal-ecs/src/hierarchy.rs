//! Parent and child links between entities.
//!
//! A child carries [`ChildOf`]; its parent carries [`Children`] in the order
//! they were attached. Both are kept in step by [`World::set_parent`] and by
//! despawning, so they never disagree. Cycles are refused.

use crate::world::{Entity, World};

/// This entity's parent. Change it with [`World::set_parent`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChildOf(Entity);

impl ChildOf {
    pub fn parent(self) -> Entity {
        self.0
    }
}

impl Default for ChildOf {
    fn default() -> Self {
        ChildOf(Entity::from_bits(u64::MAX))
    }
}

crate::text_component!(ChildOf as "child_of" newtype);

/// This entity's children, in attach order. Maintained by the world.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Children(Vec<Entity>);

impl Children {
    pub fn as_slice(&self) -> &[Entity] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HierarchyError {
    /// The child or the parent does not exist.
    NoSuchEntity(Entity),
    /// The parent is the child itself or one of its descendants.
    WouldCycle,
}

impl World {
    pub fn parent(&self, child: Entity) -> Option<Entity> {
        self.get::<ChildOf>(child).map(|c| c.0)
    }

    pub fn children(&self, parent: Entity) -> &[Entity] {
        self.get::<Children>(parent).map_or(&[], |c| &c.0)
    }

    /// Parent, grandparent, … up to the root.
    pub fn ancestors(&self, entity: Entity) -> impl Iterator<Item = Entity> + '_ {
        std::iter::successors(self.parent(entity), move |&e| self.parent(e))
    }

    /// Every descendant, depth first, in child order.
    pub fn descendants(&self, entity: Entity) -> Vec<Entity> {
        let mut out = Vec::new();
        let mut stack: Vec<Entity> = self.children(entity).iter().rev().copied().collect();
        while let Some(next) = stack.pop() {
            out.push(next);
            stack.extend(self.children(next).iter().rev().copied());
        }
        out
    }

    /// Attach `child` under `parent`, or detach it with `None`.
    pub fn set_parent(
        &mut self,
        child: Entity,
        parent: Option<Entity>,
    ) -> Result<(), HierarchyError> {
        if !self.contains(child) {
            return Err(HierarchyError::NoSuchEntity(child));
        }
        if let Some(parent) = parent {
            if !self.contains(parent) {
                return Err(HierarchyError::NoSuchEntity(parent));
            }
            if parent == child || self.ancestors(parent).any(|a| a == child) {
                return Err(HierarchyError::WouldCycle);
            }
        }
        if self.parent(child) == parent {
            return Ok(());
        }
        if let Some(old) = self.parent(child) {
            self.unlink_child(old, child);
            self.remove::<ChildOf>(child);
        }
        if let Some(parent) = parent {
            self.insert(child, ChildOf(parent));
            self.link_child(parent, child);
        }
        Ok(())
    }

    pub(crate) fn link_child(&mut self, parent: Entity, child: Entity) {
        if !self.contains(parent) {
            return;
        }
        match self.get_mut::<Children>(parent) {
            Some(children) => {
                if !children.0.contains(&child) {
                    children.0.push(child);
                }
            }
            None => {
                self.insert(parent, Children(vec![child]));
            }
        }
    }

    fn unlink_child(&mut self, parent: Entity, child: Entity) {
        let empty = match self.get_mut::<Children>(parent) {
            Some(children) => {
                children.0.retain(|&c| c != child);
                children.0.is_empty()
            }
            None => false,
        };
        if empty {
            self.remove::<Children>(parent);
        }
    }

    /// Called by `despawn`: the parent forgets this entity, and its children become roots.
    pub(crate) fn detach_for_despawn(&mut self, entity: Entity) {
        if let Some(parent) = self.parent(entity) {
            self.unlink_child(parent, entity);
        }
        if let Some(children) = self.get::<Children>(entity).map(|c| c.0.clone()) {
            for child in children {
                self.remove::<ChildOf>(child);
            }
        }
    }

    /// Despawn an entity and all of its descendants. Returns how many were removed.
    pub fn despawn_recursive(&mut self, entity: Entity) -> usize {
        if !self.contains(entity) {
            return 0;
        }
        let mut doomed = self.descendants(entity);
        doomed.push(entity);
        doomed.into_iter().rev().filter(|&e| self.despawn(e)).count()
    }

    /// Check that every ChildOf has a matching Children entry and back.
    pub fn check_hierarchy(&self) -> Result<(), String> {
        for table in &self.tables {
            for &entity in &table.entities {
                if let Some(parent) = self.parent(entity) {
                    if !self.children(parent).contains(&entity) {
                        return Err(format!(
                            "{entity:?} says its parent is {parent:?}, which does not list it"
                        ));
                    }
                }
                for &child in self.children(entity) {
                    if self.parent(child) != Some(entity) {
                        return Err(format!(
                            "{entity:?} lists child {child:?}, whose parent is {:?}",
                            self.parent(child)
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}
