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

// (KS, (VS, _)) x Delta(KD, VD) -> Delta(KD, (VS, VD))
// (KS, (VS, _)) x (KD, VD) -> (KD, (VD, (KS, (VS, _))))
// (KS, (VS, _)) x Delta(KD, VD) -> Delta(KD, (VD, (KS, (VS, _))))
/*
pub fn join_delta<'a, T, IS, ID, R>(
    left: IS,
    right: ID,
    _rel: R,
) -> impl Iterator<Item = Delta<R::Dst, (&'a <R::Dst as Entity>::Row, IS::Item)>> + 'a
where
    R: Rel,
    T: Clone,
    IS: Iterator<Item = (R::Src, (&'a <R::Src as Entity>::Row, T))>,
    ID: Iterator<Item = Delta<R::Dst, &'a <R::Dst as Entity>::Row>>,
{
    left.flat_map(move |(left, (left_row, rest))| {
        right.filter_map(move |d| match d {
            Delta::Insert(right, right_row) if R::Inverse::contains(right_row, left) => {
                Some(Delta::Insert(right, (right_row, (left, (left_row, rest)))))
            }
            Delta::Remove(right, right_row) if R::Inverse::contains(right_row, left) => {
                Some(Delta::Remove(right, (right_row, (left, (left_row, rest)))))
            }
            Delta::Update { id, old, new } => {
                let in_old = R::Inverse::contains(old, left);
                let in_new = R::Inverse::contains(new, left);
                if in_old && in_new {
                    Some(Delta::Update {
                        id: right,
                        old: (old, (left, (left_row, rest.clone()))),
                        new: (new, (left, (left_row, rest.clone()))),
                    })
                } else if in_old {
                    Some(Delta::Remove(right, (old, (left, (left_row, rest)))))
                } else if in_new {
                    Some(Delta::Insert(right, (new, (left, (left_row, rest)))))
                } else {
                    None
                }
            }
            _ => None,
        })
    })

    /*#[derive(Clone)]
    struct Join<A, R> {
        query: A,
        _r: PhantomData<R>,
    }

    impl<DB, T, Q, R, FI> Query<DB> for Join<Q, R>
    where
        R: Rel,
        DB: HasStore<<R::Src as Entity>::Store> + ?Sized,
        Q: Query<DB> + Clone + 'static,
        T: Clone + 'static,
    {
        type Key = R::Dst;
        type Value<'a> = (Q::Value<'a>, <R::Dst as Entity>::Row);


        fn iter<'a>(self, db: &'a DB) -> impl Iterator<Item = (T, R::Dst)> + 'a {
            self.query.iter(db).flat_map(move |(left, left_val)| {
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
    }*/
}*/

pub fn join<'a, Q, R, F, DB>(
    query: Q,
    _rel: R,
    get_rel: F,
) -> impl Query<'a, DB, Item = (Q::Item, (R::Dst, &'a <R::Dst as Entity>::Row))>
where
    Q: Query<'a, DB>,
    Q::Item: Clone,
    R: Rel,
    F: Fn(Q::Item) -> (R::Src, &'a <R::Src as Entity>::Row) + 'static,
    DB: HasStore<<R::Src as Entity>::Store> + ?Sized,
{
    #[derive(Clone)]
    struct Join<Q, R, F> {
        query: Q,
        get_rel: F,
        _r: PhantomData<R>,
    }

    impl<'a, Q, R, F, DB> Query<'a, DB> for Join<Q, R, F>
    where
        Q: Query<'a, DB>,
        Q::Item: Clone,
        R: Rel,
        F: Fn(Q::Item) -> (R::Src, &'a <R::Src as Entity>::Row) + 'static,
        DB: HasStore<<R::Src as Entity>::Store> + ?Sized,
    {
        type Item = (Q::Item, (R::Dst, &'a <R::Dst as Entity>::Row));

        fn iter(self, db: &'a DB) -> impl Iterator<Item = Self::Item> + 'a {
            self.query.iter(db).flat_map(move |item| {
                let (_, left_row) = (self.get_rel)(item.clone());
                R::targets(left_row).map(move |v| (item.clone(), (v, v.fetch(db))))
            })
        }

        fn delta(self, db: &'a DB, prev: &'a DB) -> impl Iterator<Item = Delta<Self::Item>> + 'a {
            self.query.iter(db).flat_map(move |item| {
                let (left, left_row) = (self.get_rel)(item.clone());
                // Might be good to store the delta in a vec instead of traversing the store multiple times
                R::Dst::query_all()
                    .delta(db, prev)
                    .filter_map(move |d| match d {
                        Delta::Insert(inserted) if R::Inverse::contains(inserted.1, left) => {
                            Some(Delta::Insert((item.clone(), inserted)))
                        }
                        Delta::Remove(removed) if R::Inverse::contains(removed.1, left) => {
                            Some(Delta::Remove((item.clone(), removed)))
                        }
                        Delta::Update { old, new } => {
                            let in_old = R::Inverse::contains(old.1, left);
                            let in_new = R::Inverse::contains(new.1, left);
                            if in_old && in_new {
                                Some(Delta::Update {
                                    old: (item.clone(), old),
                                    new: (item.clone(), new),
                                })
                            } else if in_old {
                                Some(Delta::Remove((item.clone(), old)))
                            } else if in_new {
                                Some(Delta::Insert((item.clone(), new)))
                            } else {
                                None
                            }
                        }
                        _ => None,
                    })
            })
        }
    }

    Join {
        query,
        get_rel,
        _r: PhantomData::<R>,
    }
}

pub trait Query<'a, DB: ?Sized> {
    type Item: 'a;

    /// Returns an iterator over all items produced by the query.
    fn iter(self, db: &'a DB) -> impl Iterator<Item = Self::Item> + 'a;

    /// Returns an iterator over all changes to this query since a previous snapshot.
    fn delta(self, db: &'a DB, prev: &'a DB) -> impl Iterator<Item = Delta<Self::Item>> + 'a;

    /*fn map<U>(self, f: impl Fn(Self::Key, &Self::Value, &DB) -> U + 'static) -> impl Query<DB, Item = U>
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
    }*/

    /*/// Join on the specified relation.
    fn join<R>(self, rel: R) -> impl Query<'a, DB, Item = (
        R::Src,
        &'a <R::Src as Entity>::Row,
        R::Dst,
        &'a <R::Dst as Entity>::Row,
    )>
    where
        R: Rel,
        DB: HasStore<<R::Src as Entity>::Store>,
        Self::Item = (R::Src, &'a <R::Src as Entity>::Row),
    {
        join(self, rel)
    }*/
}

// Query = impl Iterator<(K,V)>, V: 'a
// join combinator: Iterator<(KS,V)>, Iterator<(KD,V)>

/*
impl<DB, E> Query<DB> for E
where
    E: Entity,
    DB: HasStore<E::Store>,
{
    type Key = E;
    type Value<'a> = &'a E::Row;

    fn iter<'a>(self, db: &'a DB) -> impl Iterator<Item = (E, &'a E::Row)> + 'a {
        std::iter::once((self, self.fetch(db)))
    }

    fn delta<'a>(
        self,
        db: &'a DB,
        prev: &'a DB,
    ) -> impl Iterator<Item = Delta<E, &'a E::Row>> + 'a {
        db.store().delta(&prev.store()).filter(move |e| match e {
            Delta::Insert(id, _) => self == *id,
            Delta::Remove(id, _) => self == *id,
            Delta::Update { id, .. } => self == *id,
        })
    }
}*/

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

    fn query_all<'a, DB>() -> impl Query<'a, DB, Item = (Self, &'a Self::Row)> + Copy
    where
        DB: ?Sized + HasStore<Self::Store>,
    {
        #[derive(Copy, Clone)]
        struct Q<T>(PhantomData<fn() -> T>);
        impl<'a, DB, T> Query<'a, DB> for Q<T>
        where
            T: Entity,
            DB: ?Sized + HasStore<T::Store>,
        {
            type Item = (T, &'a T::Row);

            fn iter(self, db: &'a DB) -> impl Iterator<Item = Self::Item> + 'a {
                db.store().iter()
            }

            fn delta(
                self,
                db: &'a DB,
                prev: &'a DB,
            ) -> impl Iterator<Item = Delta<Self::Item>> + 'a {
                db.store().delta(prev.store())
            }
        }
        Q(PhantomData)
    }

    /// Returns an iterator over all entity rows in the store.
    fn fetch_all<DB>(db: &DB) -> impl Iterator<Item = (Self, (&Self::Row, ()))> + '_
    where
        DB: ?Sized + HasStore<Self::Store>,
    {
        db.store().iter().map(|(id, row)| (id, (row, ())))
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

    fn delta<'a, DB>(
        db: &'a DB,
        prev: &'a DB,
    ) -> impl Iterator<Item = Delta<(Self, &'a Self::Row)>> + 'a
    where
        DB: ?Sized + HasStore<Self::Store>,
    {
        db.store().delta(prev.store())
    }
}

/// Operations for a specific entity type on a store.
pub trait EntityStore<T: Entity>: ops::Index<T, Output = T::Row> + 'static {
    fn insert(&mut self, data: T::Row) -> Result<T, Error>;
    fn check_remove(&self, index: T) -> Result<(), Error>;
    fn remove(&mut self, index: T) -> Result<T::Row, Error>;
    fn remove_unchecked(&mut self, index: T) -> T::Row;
    fn delta<'a>(&'a self, other: &'a Self) -> impl Iterator<Item = Delta<(T, &'a T::Row)>> + 'a;
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
