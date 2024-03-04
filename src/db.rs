use crate::Index;
use crate::{Delta, Error};
use std::marker::PhantomData;
use std::{fmt, mem, ops};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(transparent)]
pub struct RevIndex(pub(crate) u32);

impl Default for RevIndex {
    fn default() -> Self {
        Self(0)
    }
}

impl RevIndex {
    pub const fn new(revision: u32) -> Self {
        Self(revision)
    }
}


pub fn join<DB, T, Q, R, FI>(query: Q, _rel: R, in_fn: FI) -> impl Query<DB, Item = (T, R::Dst)>
    where
        R: Rel,
        DB: HasStore<<R::Src as Entity>::Store> + ?Sized,
        Q: Query<DB, Item = T> + Clone + 'static,
        T: Clone + 'static,
        FI: Fn(T) -> R::Src + Clone + 'static,
{
    #[derive(Clone)]
    struct Join<A, FI, R> {
        query: A,
        in_fn: FI,
        _r: PhantomData<R>,
    }

    impl<DB, T, A, R, FI> Query<DB> for Join<A, FI, R>
        where
            R: Rel,
            DB: HasStore<<R::Src as Entity>::Store> + ?Sized,
            A: Query<DB, Item = T> + Clone + 'static,
            T: Clone + 'static,
            FI: Fn(T) -> R::Src + Clone + 'static,
    {
        type Item = (T, R::Dst);

        fn iter<'a>(self, db: &'a DB) -> impl Iterator<Item = (T, R::Dst)> + 'a {
            self.query.iter(db).flat_map(move |left_val| {
                let left = (self.in_fn)(left_val.clone());
                R::targets(left.fetch(db)).map(move |v| (left_val.clone(), v))
            })
        }

        fn delta<'a>(
            self,
            db: &'a DB,
            prev: &'a DB,
        ) -> impl Iterator<Item = Delta<(T, R::Dst)>> + 'a {
            self.query.iter(db).flat_map(move |left_val| {
                let left = (self.in_fn)(left_val.clone());
                // Might be good to store the delta in a vec instead of traversing the store multiple times
                R::Dst::query_all()
                    .delta(db, prev)
                    .filter_map(move |d| match d {
                        Delta::Insert(right) if R::Inverse::contains(right.fetch(db), left) => {
                            Some(Delta::Insert(right))
                        }
                        Delta::Remove(right) if R::Inverse::contains(right.fetch(prev), left) => {
                            Some(Delta::Remove(right))
                        }
                        Delta::Update(right) => {
                            let old_data = right.fetch(prev);
                            let new_data = right.fetch(db);
                            let in_old = R::Inverse::contains(old_data, left);
                            let in_new = R::Inverse::contains(new_data, left);
                            if in_old && in_new {
                                Some(Delta::Update(right))
                            } else if in_old {
                                Some(Delta::Remove(right))
                            } else if in_new {
                                Some(Delta::Insert(right))
                            } else {
                                None
                            }
                        }
                        _ => None,
                    })
                    .map(move |d| match d {
                        Delta::Insert(r) => Delta::Insert((left_val.clone(), r)),
                        Delta::Remove(r) => Delta::Remove((left_val.clone(), r)),
                        Delta::Update(r) => Delta::Update((left_val.clone(), r)),
                    })
            })
        }
    }

    Join {
        query,
        in_fn,
        _r: PhantomData::<R>,
    }
}

pub trait Query<DB: ?Sized> {
    type Item;

    fn iter<'a>(self, db: &'a DB) -> impl Iterator<Item = Self::Item> + 'a;

    fn delta<'a>(self, db: &'a DB, prev: &'a DB) -> impl Iterator<Item = Delta<Self::Item>> + 'a;

    fn map<U>(self, f: impl Fn(Self::Item, &DB) -> U + 'static) -> impl Query<DB, Item = U>
    where
        Self: Sized,
    {
        struct Map<Q, F>(Q, F);

        impl<Q, U, F, DB> Query<DB> for Map<Q, F>
        where
            Q: Query<DB>,
            F: Fn(Q::Item, &DB) -> U + 'static,
            DB: ?Sized,
        {
            type Item = U;

            fn iter<'a>(self, db: &'a DB) -> impl Iterator<Item = U> + 'a {
                self.0.iter(db).map(move |x| (self.1)(x, db))
            }

            fn delta<'a>(self, db: &'a DB, prev: &'a DB) -> impl Iterator<Item = Delta<U>> + 'a {
                self.0.delta(db, prev).map(move |x| match x {
                    Delta::Insert(x) => Delta::Insert((self.1)(x, db)),
                    Delta::Remove(x) => Delta::Remove((self.1)(x, prev)),
                    Delta::Update(x) => Delta::Update((self.1)(x, db)),
                })
            }
        }
        Map(self, f)
    }

    /*/// Join on the specified relation.
    fn join<R, T>(self, rel: R) -> impl Query<DB, Item = (Self::Item, R::Dst)>
    where
        R: Rel,
        DB: HasStore<<R::Src as Entity>::Store> + ?Sized,
        Self: Sized,
    {
        join(self, rel, in_fn)
    }*/
}

impl<DB, E> Query<DB> for E
where
    E: Entity,
    DB: HasStore<E::Store>,
{
    type Item = E;

    fn iter<'a>(self, db: &'a DB) -> impl Iterator<Item = E> + 'a {
        std::iter::once(self)
    }

    fn delta<'a>(self, db: &'a DB, prev: &'a DB) -> impl Iterator<Item = Delta<E>> + 'a {
        db.store().delta(&prev.store()).filter(move |e| match e {
            Delta::Insert(e) => self == *e,
            Delta::Remove(e) => self == *e,
            Delta::Update(e) => self == *e,
        })
    }
}

pub trait Rel {
    type Src: Entity;
    type Dst: Entity<Store = <Self::Src as Entity>::Store>;
    type Inverse: Rel<Src = Self::Dst, Dst = Self::Src, Inverse = Self>;
    fn targets(src: &<Self::Src as Entity>::Row) -> impl Iterator<Item = Self::Dst> + '_;
    fn contains(src: &<Self::Src as Entity>::Row, dst: Self::Dst) -> bool {
        Self::targets(src).any(|t| t == dst)
    }
}

/// Represents an entity ID.
///
/// Usually it's implemented as a newtype for a `u32` index.
pub trait Entity: Copy + Eq + fmt::Debug + 'static {
    /// Data type associated with the entity.
    type Row: Clone + 'static;
    /// Store type associated with the entity.
    type Store: EntityStore<Self>;

    fn to_u32(self) -> u32;
    fn from_u32(id: u32) -> Self;

    fn query_all<DB>() -> impl Query<DB, Item = Self> + Copy
    where
        DB: ?Sized + HasStore<Self::Store>,
    {
        #[derive(Copy, Clone)]
        struct Q<T>(PhantomData<fn() -> T>);
        impl<DB, T> Query<DB> for Q<T>
        where
            T: Entity,
            DB: ?Sized + HasStore<T::Store>,
        {
            type Item = T;

            fn iter<'a>(self, db: &'a DB) -> impl Iterator<Item = T> + 'a {
                db.store().keys()
            }

            fn delta<'a>(self, db: &'a DB, prev: &'a DB) -> impl Iterator<Item = Delta<T>> + 'a {
                db.store().delta(prev.store())
            }
        }
        Q(PhantomData)
    }

    /// Returns an iterator over all entity rows in the store.
    fn fetch_all<DB>(db: &DB) -> impl Iterator<Item = (Self, &Self::Row)> + '_
    where
        DB: ?Sized + HasStore<Self::Store>,
    {
        db.store().iter()
    }

    /// Returns an iterator over all entity IDs in the store.
    fn all<DB>(db: &DB) -> impl Iterator<Item = Self> + '_
    where
        DB: ?Sized + HasStore<Self::Store>,
    {
        db.store().keys()
    }

    /// Fetches the entity row from the store.
    fn fetch<DB>(self, db: &DB) -> &Self::Row
    where
        DB: ?Sized + HasStore<Self::Store>,
    {
        &db.store()[self]
    }
}


/// Operations for a specific entity type on a store.
pub trait EntityStore<T: Entity>: ops::Index<T, Output = T::Row> + 'static {
    fn insert(&mut self, data: T::Row) -> Result<T, Error>;
    fn check_remove(&self, index: T) -> Result<(), Error>;
    fn remove(&mut self, index: T) -> Result<T::Row, Error>;
    fn remove_unchecked(&mut self, index: T) -> T::Row;
    fn delta<'a>(&'a self, other: &'a Self) -> impl Iterator<Item = Delta<T>> + 'a;
    fn iter(&self) -> impl Iterator<Item = (T, &T::Row)>;
    fn keys<'a>(&'a self) -> impl Iterator<Item = T> + 'a;
}

/// Trait implemented by databases that hold a specific store type.
pub trait HasStore<Store> {
    fn store(&self) -> &Store;
    fn store_mut(&mut self) -> &mut Store;
}

/// Helper trait for relation attributes (`Option<T>` for optional to-one relations, `Vec<T>` for to-many relations).
pub trait RelOps {
    type Index: Copy + Eq + fmt::Debug;
    fn is_full(&self) -> bool;
    fn is_empty(&self) -> bool;
    fn insert(&mut self, index: Self::Index);
    fn remove(&mut self, index: Self::Index);
    fn contains(&self, index: Self::Index) -> bool;
}

impl<T: Copy + Eq + fmt::Debug> RelOps for Option<T> {
    type Index = T;

    fn is_full(&self) -> bool {
        self.is_some()
    }

    fn is_empty(&self) -> bool {
        self.is_none()
    }

    fn insert(&mut self, index: Self::Index) {
        *self = Some(index);
    }

    fn remove(&mut self, index: Self::Index) {
        if let Some(prev) = *self {
            assert_eq!(prev, index, "inconsistent index");
        }
        *self = None;
    }

    fn contains(&self, index: Self::Index) -> bool {
        self == &Some(index)
    }
}

impl<T: Copy + Eq + fmt::Debug> RelOps for Vec<T> {
    type Index = T;

    fn is_full(&self) -> bool {
        false
    }

    fn is_empty(&self) -> bool {
        self.is_empty()
    }

    fn insert(&mut self, index: Self::Index) {
        if !<[_]>::contains(self, &index) {
            self.push(index);
        }
    }

    fn remove(&mut self, index: Self::Index) {
        if let Some(pos) = self.iter().position(|x| *x == index) {
            self.swap_remove(pos);
        }
    }

    fn contains(&self, index: Self::Index) -> bool {
        <[_]>::contains(self, &index)
    }
}

/// Operations on a database type.
pub trait Database: Send + 'static {
    /// Creates a snapshot of the database.
    ///
    /// This supposed to be cheap so you can use it to implement an undo/redo system.

    /// Rolls back the database to the given revision.
    fn rollback(&self, index: RevIndex);
}

/*
store! {
    store ExtendedTrackDb : TrackDb;

    // Extensions to Album
    // Only contains optional relations or defaultable attributes
    AlbumExt[Album] (
        rel studio: RecordingStudio?.albums
    );

    // syntax alternatives:
    // - AlbumExt[Album](rel studio: RecordingStudio?.albums)
    // - extend AlbumExt for Album (rel studio: RecordingStudio?.albums)

    RecordingStudio(
        name: String,
        rel albums: Album*
    );
}
*/
