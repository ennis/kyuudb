use std::{fmt, mem};
use crate::Error;
use crate::Index;

/*
#[doc(hidden)]
#[macro_export]
macro_rules! __try {
    ($($b:stmt)*) => {
        (|| -> Option<_> {
            Some({$($b)*})
        })()
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __validate {
    ($( $e:expr => $err:expr,)*) => {
        {
            $(
                match $crate::__try!($e) {
                    Some(false) => return Err($err),
                    _ => {},
                }
            )*
        }
    };
}*/

pub trait Entity: Copy + Eq + fmt::Debug {
    type Row;
}

pub trait EntityStore<T: Entity> {
    fn insert(&mut self, data: T::Row) -> Result<T, Error>;
    fn check_remove(&self, index: T) -> Result<(), Error>;
    fn remove(&mut self, index: T) -> Result<T::Row, Error>;
    fn remove_unchecked(&mut self, index: T) -> T::Row;
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
}


impl<T: Copy + Eq + fmt::Debug> RelOps for Vec<T>
{
    type Index = T;

    fn is_full(&self) -> bool {
        false
    }

    fn is_empty(&self) -> bool {
        self.is_empty()
    }

    fn insert(&mut self, index: Self::Index) {
        if !self.contains(&index) {
            self.push(index);
        }
    }

    fn remove(&mut self, index: Self::Index) {
        if let Some(pos) = self.iter().position(|x| *x == index) {
            self.swap_remove(pos);
        }
    }
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