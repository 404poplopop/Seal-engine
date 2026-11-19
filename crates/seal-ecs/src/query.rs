//! Queries over archetype tables.

use std::any::TypeId;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

use seal_jobs::{JobPool, Priority};

use crate::column::{Column, ColumnOf, ComponentTicks};
use crate::world::{Entity, Table, World};
use crate::Component;

/// One table's columns, handed out once each to the parts of a query.
pub struct TableView<'a> {
    types: &'a [TypeId],
    slots: Vec<Option<&'a mut dyn Column>>,
    entities: &'a [Entity],
    tick: u32,
}

impl<'a> TableView<'a> {
    fn new(table: &'a mut Table, tick: u32) -> Self {
        Self {
            types: &table.types,
            slots: table.columns.iter_mut().map(|c| Some(c.as_mut())).collect(),
            entities: &table.entities,
            tick,
        }
    }

    fn take<T: Component>(&mut self) -> &'a mut ColumnOf<T> {
        let index = self.types.binary_search(&TypeId::of::<T>()).expect("query matched this table");
        let column = self.slots[index]
            .take()
            .unwrap_or_else(|| panic!("{} appears twice in one query", std::any::type_name::<T>()));
        column.as_any_mut().downcast_mut().expect("column type")
    }
}

/// A part of a query: `&T`, `&mut T`, [`Entity`], [`Ticks<T>`], or a tuple of those.
pub trait Fetch {
    type Item<'a>;
    type Slice<'a>: Send;
    type Iter<'a>: Iterator<Item = Self::Item<'a>>;

    /// Component types this part needs. The bool is true for write access.
    fn components(out: &mut Vec<(TypeId, bool)>);
    fn take<'a>(view: &mut TableView<'a>) -> Self::Slice<'a>;
    fn len(slice: &Self::Slice<'_>) -> usize;
    fn split_at<'a>(slice: Self::Slice<'a>, mid: usize) -> (Self::Slice<'a>, Self::Slice<'a>);
    fn iter<'a>(slice: Self::Slice<'a>) -> Self::Iter<'a>;
}

impl Fetch for Entity {
    type Item<'a> = Entity;
    type Slice<'a> = &'a [Entity];
    type Iter<'a> = std::iter::Copied<std::slice::Iter<'a, Entity>>;

    fn components(_: &mut Vec<(TypeId, bool)>) {}
    fn take<'a>(view: &mut TableView<'a>) -> Self::Slice<'a> {
        view.entities
    }
    fn len(slice: &Self::Slice<'_>) -> usize {
        slice.len()
    }
    fn split_at<'a>(slice: Self::Slice<'a>, mid: usize) -> (Self::Slice<'a>, Self::Slice<'a>) {
        slice.split_at(mid)
    }
    fn iter<'a>(slice: Self::Slice<'a>) -> Self::Iter<'a> {
        slice.iter().copied()
    }
}

impl<T: Component> Fetch for &T {
    type Item<'a> = &'a T;
    type Slice<'a> = &'a [T];
    type Iter<'a> = std::slice::Iter<'a, T>;

    fn components(out: &mut Vec<(TypeId, bool)>) {
        out.push((TypeId::of::<T>(), false));
    }
    fn take<'a>(view: &mut TableView<'a>) -> Self::Slice<'a> {
        &view.take::<T>().data
    }
    fn len(slice: &Self::Slice<'_>) -> usize {
        slice.len()
    }
    fn split_at<'a>(slice: Self::Slice<'a>, mid: usize) -> (Self::Slice<'a>, Self::Slice<'a>) {
        slice.split_at(mid)
    }
    fn iter<'a>(slice: Self::Slice<'a>) -> Self::Iter<'a> {
        slice.iter()
    }
}

/// Write access to one component; stamps it as changed only when written.
pub struct Mut<'a, T> {
    value: &'a mut T,
    changed: &'a mut u32,
    tick: u32,
}

impl<T> Mut<'_, T> {
    /// Read without marking the component changed.
    pub fn get(&self) -> &T {
        self.value
    }

    /// Set only if different, so unchanged values do not wake change-driven systems.
    pub fn set_if_neq(&mut self, value: T)
    where
        T: PartialEq,
    {
        if *self.value != value {
            *self.value = value;
            *self.changed = self.tick;
        }
    }
}

impl<T> Deref for Mut<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.value
    }
}

impl<T> DerefMut for Mut<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        *self.changed = self.tick;
        self.value
    }
}

pub struct MutSlice<'a, T> {
    data: &'a mut [T],
    changed: &'a mut [u32],
    tick: u32,
}

pub struct MutIter<'a, T> {
    data: std::slice::IterMut<'a, T>,
    changed: std::slice::IterMut<'a, u32>,
    tick: u32,
}

impl<'a, T> Iterator for MutIter<'a, T> {
    type Item = Mut<'a, T>;
    fn next(&mut self) -> Option<Self::Item> {
        Some(Mut { value: self.data.next()?, changed: self.changed.next()?, tick: self.tick })
    }
}

impl<T: Component> Fetch for &mut T {
    type Item<'a> = Mut<'a, T>;
    type Slice<'a> = MutSlice<'a, T>;
    type Iter<'a> = MutIter<'a, T>;

    fn components(out: &mut Vec<(TypeId, bool)>) {
        out.push((TypeId::of::<T>(), true));
    }
    fn take<'a>(view: &mut TableView<'a>) -> Self::Slice<'a> {
        let tick = view.tick;
        let column = view.take::<T>();
        MutSlice { data: &mut column.data, changed: &mut column.changed, tick }
    }
    fn len(slice: &Self::Slice<'_>) -> usize {
        slice.data.len()
    }
    fn split_at<'a>(slice: Self::Slice<'a>, mid: usize) -> (Self::Slice<'a>, Self::Slice<'a>) {
        let (d0, d1) = slice.data.split_at_mut(mid);
        let (c0, c1) = slice.changed.split_at_mut(mid);
        (
            MutSlice { data: d0, changed: c0, tick: slice.tick },
            MutSlice { data: d1, changed: c1, tick: slice.tick },
        )
    }
    fn iter<'a>(slice: Self::Slice<'a>) -> Self::Iter<'a> {
        MutIter { data: slice.data.iter_mut(), changed: slice.changed.iter_mut(), tick: slice.tick }
    }
}

/// Read a component's added and changed ticks without borrowing its value.
pub struct Ticks<T>(PhantomData<T>);

pub struct TicksIter<'a> {
    added: std::slice::Iter<'a, u32>,
    changed: std::slice::Iter<'a, u32>,
}

impl Iterator for TicksIter<'_> {
    type Item = ComponentTicks;
    fn next(&mut self) -> Option<ComponentTicks> {
        Some(ComponentTicks { added: *self.added.next()?, changed: *self.changed.next()? })
    }
}

impl<T: Component> Fetch for Ticks<T> {
    type Item<'a> = ComponentTicks;
    type Slice<'a> = (&'a [u32], &'a [u32]);
    type Iter<'a> = TicksIter<'a>;

    fn components(out: &mut Vec<(TypeId, bool)>) {
        out.push((TypeId::of::<T>(), false));
    }
    fn take<'a>(view: &mut TableView<'a>) -> Self::Slice<'a> {
        let column = view.take::<T>();
        (&column.added, &column.changed)
    }
    fn len(slice: &Self::Slice<'_>) -> usize {
        slice.0.len()
    }
    fn split_at<'a>(slice: Self::Slice<'a>, mid: usize) -> (Self::Slice<'a>, Self::Slice<'a>) {
        let (a0, a1) = slice.0.split_at(mid);
        let (c0, c1) = slice.1.split_at(mid);
        ((a0, c0), (a1, c1))
    }
    fn iter<'a>(slice: Self::Slice<'a>) -> Self::Iter<'a> {
        TicksIter { added: slice.0.iter(), changed: slice.1.iter() }
    }
}

macro_rules! tuple_fetch {
    ($iter:ident; $($name:ident $index:tt),+) => {
        pub struct $iter<$($name),+>($($name,)+);

        impl<$($name: Iterator),+> Iterator for $iter<$($name),+> {
            type Item = ($($name::Item,)+);
            fn next(&mut self) -> Option<Self::Item> {
                Some(($(self.$index.next()?,)+))
            }
        }

        impl<$($name: Fetch),+> Fetch for ($($name,)+) {
            type Item<'a> = ($($name::Item<'a>,)+);
            type Slice<'a> = ($($name::Slice<'a>,)+);
            type Iter<'a> = $iter<$($name::Iter<'a>),+>;

            fn components(out: &mut Vec<(TypeId, bool)>) {
                $($name::components(out);)+
            }
            fn take<'a>(view: &mut TableView<'a>) -> Self::Slice<'a> {
                ($($name::take(view),)+)
            }
            fn len(slice: &Self::Slice<'_>) -> usize {
                A::len(&slice.0)
            }
            #[allow(non_snake_case)]
            fn split_at<'a>(slice: Self::Slice<'a>, mid: usize) -> (Self::Slice<'a>, Self::Slice<'a>) {
                $(let $name = $name::split_at(slice.$index, mid);)+
                (($($name.0,)+), ($($name.1,)+))
            }
            fn iter<'a>(slice: Self::Slice<'a>) -> Self::Iter<'a> {
                $iter($($name::iter(slice.$index),)+)
            }
        }
    };
}

tuple_fetch!(Iter1; A 0);
tuple_fetch!(Iter2; A 0, B 1);
tuple_fetch!(Iter3; A 0, B 1, C 2);
tuple_fetch!(Iter4; A 0, B 1, C 2, D 3);
tuple_fetch!(Iter5; A 0, B 1, C 2, D 3, E 4);
tuple_fetch!(Iter6; A 0, B 1, C 2, D 3, E 4, F 5);

/// A query over every entity that has all of `Q`'s components.
pub struct Query<'w, Q: Fetch> {
    world: &'w mut World,
    with: Vec<TypeId>,
    without: Vec<TypeId>,
    _q: PhantomData<fn() -> Q>,
}

impl World {
    pub fn query<Q: Fetch>(&mut self) -> Query<'_, Q> {
        Query { world: self, with: Vec::new(), without: Vec::new(), _q: PhantomData }
    }
}

impl<Q: Fetch> Query<'_, Q> {
    /// Only entities that also have `T`.
    pub fn with<T: Component>(mut self) -> Self {
        self.with.push(TypeId::of::<T>());
        self
    }

    /// Skip entities that have `T`.
    pub fn without<T: Component>(mut self) -> Self {
        self.without.push(TypeId::of::<T>());
        self
    }

    fn matching(&self) -> Vec<usize> {
        let mut needed = Vec::new();
        Q::components(&mut needed);
        self.world
            .tables
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                !t.is_empty()
                    && needed.iter().all(|(id, _)| t.has(*id))
                    && self.with.iter().all(|id| t.has(*id))
                    && !self.without.iter().any(|id| t.has(*id))
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Slices for each matching table, in table order.
    fn slices(&mut self) -> Vec<Q::Slice<'_>> {
        let matching = self.matching();
        let tick = self.world.tick();
        let mut out = Vec::with_capacity(matching.len());
        let mut wanted = matching.into_iter().peekable();
        for (index, table) in self.world.tables.iter_mut().enumerate() {
            if wanted.peek() == Some(&index) {
                wanted.next();
                let mut view = TableView::new(table, tick);
                out.push(Q::take(&mut view));
            }
        }
        out
    }

    pub fn count(&self) -> usize {
        self.matching().iter().map(|&i| self.world.tables[i].len()).sum()
    }

    pub fn for_each(mut self, mut f: impl FnMut(Q::Item<'_>)) {
        for slice in self.slices() {
            Q::iter(slice).for_each(&mut f);
        }
    }

    /// Collect items in table and row order.
    pub fn collect<R>(mut self, mut f: impl FnMut(Q::Item<'_>) -> R) -> Vec<R> {
        let mut out = Vec::new();
        for slice in self.slices() {
            out.extend(Q::iter(slice).map(&mut f));
        }
        out
    }

    fn chunks(&mut self, chunk: usize) -> Vec<Q::Slice<'_>> {
        let chunk = chunk.max(1);
        let mut parts = Vec::new();
        for mut slice in self.slices() {
            while Q::len(&slice) > chunk {
                let (head, tail) = Q::split_at(slice, chunk);
                parts.push(head);
                slice = tail;
            }
            if Q::len(&slice) > 0 {
                parts.push(slice);
            }
        }
        parts
    }

    /// Run `f` on every item, `chunk` rows per job.
    pub fn par_for_each<F>(mut self, pool: &JobPool, chunk: usize, f: F)
    where
        F: for<'a> Fn(Q::Item<'a>) + Sync,
    {
        let parts = self.chunks(chunk);
        let f = &f;
        pool.scope(|scope| {
            for part in parts {
                scope.spawn(Priority::Normal, move || Q::iter(part).for_each(f));
            }
        });
    }

    /// Run `map` over each chunk in parallel and return the results in table
    /// and row order, so folding them gives the same answer on any core count.
    pub fn par_map_chunks<R, F>(mut self, pool: &JobPool, chunk: usize, map: F) -> Vec<R>
    where
        R: Send,
        F: for<'a> Fn(Q::Iter<'a>) -> R + Sync,
    {
        let parts = self.chunks(chunk);
        let mut results: Vec<Option<R>> = (0..parts.len()).map(|_| None).collect();
        let map = &map;
        pool.scope(|scope| {
            for (part, slot) in parts.into_iter().zip(results.iter_mut()) {
                scope.spawn(Priority::Normal, move || *slot = Some(map(Q::iter(part))));
            }
        });
        results.into_iter().map(|r| r.expect("every chunk ran")).collect()
    }
}
