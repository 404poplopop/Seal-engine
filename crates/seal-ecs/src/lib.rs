//! Seal's entity-component core.
//!
//! * Entities are 64-bit generational ids: a 32-bit slot index and a 32-bit
//!   generation, so a stale id never reaches a reused slot.
//! * Components live in archetype tables: one table per exact set of
//!   component types, one dense column per type. Queries walk whole columns.
//! * Every component carries an added tick and a changed tick. Writes through
//!   a query's [`Mut`] or [`World::get_mut`] stamp the world's current tick,
//!   so systems can skip what has not changed since they last ran.
//! * Required components: registering `A requires B` adds a default `B`
//!   whenever `A` is added and `B` is missing.
//! * Queries can run in parallel over row chunks on a [`seal_jobs::JobPool`];
//!   ordered folds return chunk results in table and row order, never in
//!   completion order.
#![forbid(unsafe_code)]

mod column;
pub mod core;
mod hierarchy;
mod merge;
mod observe;
mod prefab;
mod query;
mod registry;
mod relation;
mod schedule;
mod snapshot;
pub mod sprite_draw;
mod text;
mod vocab;
mod world;

pub use column::ComponentTicks;
pub use hierarchy::{ChildOf, Children, HierarchyError};
pub use merge::{merge_scenes, MergeOutcome};
pub use observe::ObserverId;
pub use prefab::{
    diff_patch, merge_patch, Prefab, PrefabChild, PrefabInstance, PrefabLibrary, PrefabSlot,
    Resolved,
};
pub use query::{Fetch, Mut, Query, Ticks};
pub use relation::{Relation, RelationView, Sources, Targets};
pub use schedule::{Phase, Schedule, SystemConfig, Time};
pub use seal_sealtext::{parse as parse_seal_text, SealMap, SealValue};
pub use snapshot::{History, Snapshot, SnapshotError};
pub use text::{LoadCtx, SaveCtx, SavedScene, SceneId, SealField, TextComponent, SCENE_VERSION};
pub use vocab::{detect as detect_vocabulary, to_classic, to_scene_graph, Vocabulary};
pub use world::{Entity, World};

/// Anything that can be stored as a component.
pub trait Component: Send + Sync + 'static {}
impl<T: Send + Sync + 'static> Component for T {}

/// A set of components spawned or inserted together, written as a tuple.
pub trait Bundle: Send + Sync + 'static {
    #[doc(hidden)]
    fn describe(out: &mut Vec<column::ColumnInfo>);
    #[doc(hidden)]
    fn push_into(self, table: &mut world::Table, tick: u32);
}

macro_rules! tuple_bundle {
    ($($name:ident $index:tt),+) => {
        impl<$($name: Component),+> Bundle for ($($name,)+) {
            fn describe(out: &mut Vec<column::ColumnInfo>) {
                $(out.push(column::ColumnInfo::of::<$name>());)+
            }
            fn push_into(self, table: &mut world::Table, tick: u32) {
                $(table.column_of_mut::<$name>().expect("bundle column").push(self.$index, tick);)+
            }
        }
    };
}

impl Bundle for () {
    fn describe(_: &mut Vec<column::ColumnInfo>) {}
    fn push_into(self, _: &mut world::Table, _: u32) {}
}

tuple_bundle!(A 0);
tuple_bundle!(A 0, B 1);
tuple_bundle!(A 0, B 1, C 2);
tuple_bundle!(A 0, B 1, C 2, D 3);
tuple_bundle!(A 0, B 1, C 2, D 3, E 4);
tuple_bundle!(A 0, B 1, C 2, D 3, E 4, F 5);
tuple_bundle!(A 0, B 1, C 2, D 3, E 4, F 5, G 6);
tuple_bundle!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7);
