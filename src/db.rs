use crate::{Index, Table};
use crate::{Delta, Error};
use std::marker::PhantomData;
use std::{fmt, mem, ops};
use std::collections::Bound;
use std::ops::RangeBounds;
use im::OrdMap;

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

pub trait Query<'a, DB: ?Sized> {
    type Item: 'a;

    /// Returns an iterator over all items produced by the query.
    fn iter(self, db: &'a DB) -> impl Iterator<Item = Self::Item> + 'a;

    /// Returns an iterator over all changes to this query since a previous snapshot.
    fn delta(self, db: &'a DB, prev: &'a DB) -> impl Iterator<Item = Delta<Self::Item>> + 'a;
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

/*
/// Helper trait for relations.
pub trait Rel {
    type Store;
    type Src: EntityId;
    type Dst: EntityId;
    type Inverse: Rel<Src = Self::Dst, Dst = Self::Src, Inverse = Self>;

    /// Returns whether a new relation targeting the given destination can be inserted.
    fn may_insert(store: &Self::Store, dst: Self::Dst) -> Result<(), Error>;

    /// Tries to insert a new relation.
    fn try_insert(
        store: &mut Self::Store,
        src: Self::Src,
        dst: Self::Dst,
    ) -> Result<(), Error>;

    /// Tries to remove a relation.
    fn remove(
        store: &mut Self::Store,
        src: Self::Src,
        dst: Self::Dst,
    ) -> Result<(), Error>;
}*/

#[macro_export]
macro_rules! impl_rel_N_to_1 {
    ($rel:ident, $store:ident, $src:ident, $src_id:ident, $fk:ident, $dst:ident, $dst_id:ident, $inv_rel:ident, $index:ident) => {
        struct $rel;
        struct $inv_rel;

        impl $rel {
            fn may_insert(store: &$store, dst: $dst_id) -> Result<(), $crate::Error> {
                Ok(())
            }

            fn try_insert(
                store: &mut $store,
                src: $src_id,
                dst: $dst_id,
            ) -> Result<(), $crate::Error> {
                let prev_dst = ::std::mem::replace(&mut store.$src[src].$fk, dst);
                store.$index.remove((prev_dst, src), ());
                store.$index.insert((dst, src), ());
                Ok(())
            }

            fn remove(
                _store: &mut <$src as $crate::Entity>::Store,
                _src: $src,
                _dst: $dst,
            ) -> Result<(), $crate::Error> {
                Err($crate::Error::RelationshipDeniedDelete)
            }
        }

        impl $crate::Rel for $inv_rel {
            type Src = $dst;
            type Dst = $src;
            type Inverse = $rel;

            fn targets(
                store: &<$src as $crate::Entity>::Store,
                src: $dst,
            ) -> impl Iterator<Item = $src> + '_ {
                store
                    .$index
                    .range((src, $src::MIN)..(src, $src::MAX))
                    .map(|(_, v)| v)
            }

            fn may_insert(
                store: &<$src as $crate::Entity>::Store,
                dst: $src,
            ) -> Result<(), $crate::Error> {
                Err($crate::Error::RelationshipDeniedDelete)
            }

            fn try_insert(
                store: &mut <$src as $crate::Entity>::Store,
                src: $dst,
                dst: $src,
            ) -> Result<(), $crate::Error> {
                $rel::try_insert(store, dst, src)
            }

            fn remove(
                _store: &mut <$src as $crate::Entity>::Store,
                _src: $dst,
                _dst: $src,
            ) -> Result<(), $crate::Error> {
                Err($crate::Error::RelationshipDeniedDelete)
            }
        }
    };
}

#[macro_export]
macro_rules! index_01_to_1 {
    ($rel:ident, $src:ident,  $fk:ident, $dst:ident, $inv_rel:ident, $index:ident) => {
        struct $rel;
        struct $inv_rel;

        impl $crate::Rel for $rel {
            type Src = $src;
            type Dst = $dst;
            type Inverse = $inv_rel;

            fn targets(
                store: &<$src as $crate::Entity>::Store,
                src: $src,
            ) -> impl Iterator<Item = $dst> + '_ {
                ::std::iter::once(store[src].$fk)
            }

            fn may_insert(
                store: &<$src as $crate::Entity>::Store,
                dst: $dst,
            ) -> Result<(), $crate::Error> {
                if store.$index.contains_key(dst) {
                    return Err($crate::Error::RelationshipTooManyTargets);
                }
                Ok(())
            }

            fn try_insert(
                store: &mut <$src as $crate::Entity>::Store,
                src: $src,
                dst: $dst,
            ) -> Result<(), $crate::Error> {
                if store.$index.contains_key(dst) {
                    return Err($crate::Error::RelationshipTooManyTargets);
                }
                let prev_dst = ::std::mem::replace(&mut store[src].$fk, dst);
                store.$index.remove(prev_dst);
                store.$index.insert(dst, src);
                Ok(())
            }

            fn remove(
                _store: &mut <$src as $crate::Entity>::Store,
                _src: $src,
                _dst: $dst,
            ) -> Result<(), $crate::Error> {
                Err($crate::Error::RelationshipDeniedDelete)
            }
        }

        impl $crate::Rel for $inv_rel {
            type Src = $dst;
            type Dst = $src;
            type Inverse = $rel;

            fn targets(
                store: &<$src as $crate::Entity>::Store,
                src: $dst,
            ) -> impl Iterator<Item = $src> + '_ {
                store.$index.get(&src)
            }

            fn may_insert(
                store: &<$src as $crate::Entity>::Store,
                dst: $src,
            ) -> Result<(), $crate::Error> {
                Err($crate::Error::RelationshipDeniedDelete)
            }

            fn try_insert(
                store: &mut <$src as $crate::Entity>::Store,
                src: $dst,
                dst: $src,
            ) -> Result<(), $crate::Error> {
                $rel::try_insert(store, dst, src)
            }

            fn remove(
                _store: &mut <$src as $crate::Entity>::Store,
                _src: $dst,
                _dst: $src,
            ) -> Result<(), $crate::Error> {
                Err($crate::Error::RelationshipDeniedDelete)
            }
        }
    };
}

/*
// E.g. FkIndex<Track, Album>

/// (Internal) Index for a non-unique foreign key relation.
struct RelIndex<A, B>(OrdMap<(A,B),()>);

/// (Internal) Index for a unique foreign key relation.
struct RelIndexUnique<K,V>(OrdMap<K,V>);

impl<K, V> RelIndex<K, V> where K: Ord, V: Ord {
    fn insert(&mut self, k: K, v: V) {
        self.0.insert((k,v), ());
    }
    fn remove(&mut self, k: K, v: V) {
        self.0.remove(&(k,v));
    }
    fn contains_key(&self, k: K) -> bool {
        self.0.contains_key(&(k,..))
    }
    fn get(&self, k: K) -> impl Iterator<Item = V> + '_ {
        self.0.range((k,..)).map(|(_,v)| v)
    }
    fn range<R>(&self, range: R) -> impl Iterator<Item = (K,V)> + '_ where R: RangeBounds<K> {
        self.0.range(range)
    }
}*/

/// Entity index.
pub trait EntityId: Copy + Eq + fmt::Debug + 'static {
    fn from_u32(id: u32) -> Self;
    fn to_u32(self) -> u32;
}

/// Represents an entity.
///
/// Usually it's implemented as a newtype for a `u32` index.
pub trait Entity: 'static + Clone {
    type Id: EntityId;
    fn id(&self) -> Self::Id;
}

/*
/// Operations for a specific entity type on a store.
pub trait EntityStore<T: Entity>: ops::Index<T::Id, Output = T> + 'static {
    fn insert(&mut self, f: impl FnOnce(T::Id) -> T) -> Result<T::Id, Error>;
    fn remove(&mut self, index: T::Id) -> Result<T, Error>;
    fn delta<'a>(&'a self, other: &'a Self) -> impl Iterator<Item = Delta<&'a T>> + 'a;
    fn iter<'a>(&'a self) -> impl Iterator<Item = &'a T>;
}*/

/// Trait implemented by databases that hold a specific store type.
pub trait HasStore<Store> {
    fn store(&self) -> &Store;
    fn store_mut(&mut self) -> &mut Store;
}

pub trait Relation {
    type Key;
    type Value;
}

/// Represents an operation on a store when an entity of a specific type is modified.
///
/// There are triggers are in charge of:
/// - enforcing unique constraints
/// - updating indices
/// - enforcing integrity rules on deletion, like "delete cascade" (delete all related entities when an entity is deleted).
pub trait Trigger<DB: ?Sized, R: Relation> {
    /// Called before an entity is inserted.
    fn before_insert(&self, db: &DB, inserting: &R::Value) -> Result<(), Error> {
        Ok(())
    }

    /// Called after an entity is inserted.
    fn after_insert(&self, db: &mut DB, inserted: &R::Value) -> Result<(), Error> {
        Ok(())
    }

    /// Called when an entity is about to be deleted.
    fn before_delete(&self, db: &DB, deleting: &R::Value) -> Result<(), Error> {
        Ok(())
    }

    /// Called after an entity is deleted.
    fn after_delete(&self, db: &mut DB, deleted: &R::Value) -> Result<(), Error> {
        Ok(())
    }
}



/// Operations on a database type.
pub trait Database: Send + 'static {
    /// Rolls back the database to the given revision.
    fn rollback(&self, index: RevIndex);
}


/// Operations on relation indices.
pub trait RelIndex {
    /// The child entity.
    type Key;
    /// The referenced entity.
    type Value;

    fn get(&self, key: Self::Key) -> impl Iterator<Item = Self::Value> + '_;
}

// Foreign-key index for entity A referencing B: OrdMap<(B,A),()>,
// Foreign-key index for entity A referencing B, with unique constraint: OrdMap<B,A>,

/*
impl<K,V> RelIndex for OrdMap<(K,V),()> where K: Ord, V: Ord {
    type Key = K;
    type Value = V;

    fn get(&self, key: &K) -> impl Iterator<Item = &V> + '_ {
        self.range((key,..)).map(|(_,v)| v)
    }
}*/

/*
// A <- B.a
fn join2_delta_helper<A,B,DB>(db: &DB, prev: &DB) {
    let a_delta =  db.a.delta(&prev.a);
    let b_delta =  db.b.delta(&prev.b);

    let index = &db.index;

    let mut joined = im::OrdMap::<(A::Id, B::Id), Delta<(&A, &B)>>::new();

    // A x Delta(B)
    for (_,b) in b_delta {
        match b {
            Delta::Insert(b) => {
                let a = db.a.get(b.a);
                joined.insert((a.id, b.id), Delta::Insert((a, b)));
            }
            Delta::Remove(b) => {
                let a = prev.a.get(b.a);
                joined.insert((a.id, b.id), Delta::Remove((a, b)));
            }
            Delta::Update { old, new } => {
                let old_a = db.a.get(old.a);
                let new_a = if old.a != new.a {
                    db.a.get(new.a)
                } else {
                    old_a
                };
                joined.insert((old.a, old.id), Delta::Remove((a, old)));
                joined.insert((old.b, Delta::Insert((a, new)));
            }
        }
    }


}
*/
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
