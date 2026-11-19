//! What the world knows about each component type beyond storing it:
//! how to clone a column (snapshots, undo) and how to read and write it as
//! SealText (scene files).

use std::any::TypeId;
use std::collections::HashMap;

use seal_sealtext::SealValue;

use crate::column::{Column, ColumnOf};
use crate::text::{LoadCtx, SaveCtx, TextComponent};
use crate::world::{Entity, World};
use crate::Component;

pub(crate) type CloneColumn = fn(&dyn Column) -> Box<dyn Column>;
pub(crate) type SaveRow = fn(&dyn Column, usize, &SaveCtx) -> SealValue;
pub(crate) type LoadInsert = fn(&mut World, Entity, &SealValue, &LoadCtx) -> Result<(), String>;

#[derive(Clone, Copy)]
pub(crate) struct TextOps {
    pub name: &'static str,
    pub version: u32,
    pub save: SaveRow,
    pub load: LoadInsert,
    pub migrate: fn(u32, SealValue) -> Result<SealValue, String>,
    pub fields: fn() -> &'static [&'static str],
    pub remove: fn(&mut World, Entity) -> bool,
    /// Read a (possibly partial) value and write it back: fills defaults and normalises spelling.
    pub canonical: fn(&SealValue) -> Result<SealValue, String>,
}

#[derive(Clone, Copy)]
pub(crate) struct Registration {
    pub clone: Option<CloneColumn>,
    pub text: Option<TextOps>,
}

#[derive(Default, Clone)]
pub(crate) struct Registry {
    pub by_type: HashMap<TypeId, Registration>,
    pub by_name: HashMap<&'static str, TypeId>,
    /// Resources saved in scene files, by name.
    pub resources: std::collections::BTreeMap<&'static str, ResourceOps>,
}

/// Save and load one resource type in a scene's `resources` block.
#[derive(Clone, Copy)]
pub(crate) struct ResourceOps {
    pub save: fn(&World) -> Option<SealValue>,
    /// Parse only, so a bad file is rejected before the world changes.
    pub check: fn(&SealValue) -> Result<(), String>,
    pub load: fn(&mut World, &SealValue) -> Result<(), String>,
}

fn clone_column<T: Component + Clone>(column: &dyn Column) -> Box<dyn Column> {
    let column = column.as_any().downcast_ref::<ColumnOf<T>>().expect("registered type");
    Box::new(ColumnOf {
        data: column.data.clone(),
        added: column.added.clone(),
        changed: column.changed.clone(),
    })
}

fn save_row<T: TextComponent>(column: &dyn Column, row: usize, ctx: &SaveCtx) -> SealValue {
    let column = column.as_any().downcast_ref::<ColumnOf<T>>().expect("registered type");
    column.data[row].save(ctx)
}

fn load_insert<T: TextComponent>(
    world: &mut World,
    entity: Entity,
    value: &SealValue,
    ctx: &LoadCtx,
) -> Result<(), String> {
    let component = T::load(value, ctx).map_err(|e| format!("{}: {e}", T::NAME))?;
    world.insert(entity, component);
    Ok(())
}

impl World {
    /// Save resource `R` with scenes, under `resources.<R::NAME>`.
    pub fn register_text_resource<R: TextComponent>(&mut self) {
        let ctx_save =
            |world: &World| world.resource::<R>().map(|r| r.save(&crate::text::SaveCtx::plain()));
        self.registry.resources.insert(
            R::NAME,
            ResourceOps {
                save: ctx_save,
                check: |value| R::load(value, &crate::text::LoadCtx::plain()).map(|_| ()),
                load: |world, value| {
                    let r = R::load(value, &crate::text::LoadCtx::plain())
                        .map_err(|e| format!("resource {}: {e}", R::NAME))?;
                    world.insert_resource(r);
                    Ok(())
                },
            },
        );
    }

    /// Allow snapshots and undo to copy `T`.
    pub fn register_clone<T: Component + Clone>(&mut self) {
        self.registry
            .by_type
            .entry(TypeId::of::<T>())
            .or_insert(Registration { clone: None, text: None })
            .clone = Some(clone_column::<T>);
    }

    /// Allow `T` in scene files under the name `T::NAME`; also allows snapshots.
    pub fn register_text<T: TextComponent>(&mut self) {
        self.register_clone::<T>();
        assert!(
            !crate::vocab::RESERVED.contains(&T::NAME),
            "{} is a reserved component name",
            T::NAME
        );
        if let Some(previous) = self.registry.by_name.insert(T::NAME, TypeId::of::<T>()) {
            assert_eq!(
                previous,
                TypeId::of::<T>(),
                "two component types share the SealText name {}",
                T::NAME
            );
        }
        self.registry.by_type.get_mut(&TypeId::of::<T>()).expect("just registered").text =
            Some(TextOps {
                name: T::NAME,
                version: T::VERSION,
                save: save_row::<T>,
                load: load_insert::<T>,
                migrate: T::migrate,
                fields: T::fields,
                remove: |world, entity| world.remove::<T>(entity).is_some(),
                canonical: crate::text::canonical::<T>,
            });
    }
}
