#![feature(macro_metavar_expr)]

use kyuudb::db::Trigger;
use kyuudb::im::{HashMap, OrdMap, OrdSet};
use kyuudb::{Delta, Error, HasStore};
use kyuudb_macros::store;
use paste::paste;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::ffi::c_void;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::iter::once;
use std::marker::PhantomData;
use std::num::NonZeroU32;
use std::ops::{Bound, RangeBounds};

/*
store2! {
    /// Database schema for a music library.
    pub store TrackDb;

    /// Represents an album.
    Album {
        #[key]
        id: AlbumId,
        name: String,
        year: u32
    }

    /// Represents a track of an album.
    Track {
        #[key]
        id: TrackId,
        name: String,
        rel album: AlbumId,
        rel artist: ArtistId
    }

    /// Represents a playlist.
    Playlist {
        #[key]
        id: PlaylistId,
        name: String,
        //ref tracks: TrackId* (unique)
    }

    Artist {
        #[key]
        id: ArtistId,
        name: String
    }

}*/


pub trait Idx: Copy + Ord + Hash + fmt::Debug + Default {
    const MIN: Self;
    const MAX: Self;

    fn to_u32(self) -> u32;
    fn from_u32(id: u32) -> Self;
    fn dummy() -> Self;
    fn next(self) -> Self;
}

macro_rules! make_id {
    ($name:ident) => {
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
        #[repr(transparent)]
        pub struct $name(pub(crate) NonZeroU32);

        impl Idx for $name {
            const MIN: $name = $name(NonZeroU32::MIN);
            const MAX: $name = $name(NonZeroU32::MAX);

            fn to_u32(self) -> u32 {
                self.0.get() - 1
            }

            fn from_u32(id: u32) -> $name {
                $name(unsafe { NonZeroU32::new_unchecked(id + 1) })
            }

            fn dummy() -> $name {
                $name(unsafe { NonZeroU32::new_unchecked(u32::MAX) })
            }

            fn next(self) -> $name {
                $name::from_u32(self.to_u32() + 1)
            }
        }

        impl Default for $name {
            fn default() -> Self {
                $name::from_u32(0)
            }
        }
    };
}


make_id!(AlbumId);
make_id!(TrackId);
make_id!(PlaylistId);
make_id!(ArtistId);

trait Entity {
    type Key: Idx;
    fn key(&self) -> Self::Key;
}

#[derive(Clone)]
struct Album {
    id: AlbumId,
    name: String,
    year: u32,
    album_artist: Option<ArtistId>,
}

impl Entity for Album {
    type Key = AlbumId;
    fn key(&self) -> AlbumId {
        self.id
    }
}

#[derive(Clone)]
struct Track {
    id: TrackId,
    name: String,
    album: AlbumId,
    artist: ArtistId,
}

impl Entity for Track {
    type Key = TrackId;
    fn key(&self) -> TrackId {
        self.id
    }
}

#[derive(Clone)]
struct Playlist {
    id: PlaylistId,
    name: String,
}

impl Entity for Playlist {
    type Key = PlaylistId;
    fn key(&self) -> PlaylistId {
        self.id
    }
}

#[derive(Clone)]
struct Artist {
    id: ArtistId,
    name: String,
}

impl Entity for Artist {
    type Key = ArtistId;
    fn key(&self) -> ArtistId {
        self.id
    }
}

/*
#[derive(Clone)]
enum ChangeKind<V> {
    Inserted(V),
    Removed(V),
}*/

#[derive(Debug, Clone)]
struct Change {
    timestamp: u64,
    kind: ChangeKind,
}

#[derive(Default)]
struct ChangeSet {
    changes: Vec<Change>,
}

impl ChangeSet {
    fn since(&self, timestamp: u64) -> impl Iterator<Item = &ChangeKind> {
        let last = self.changes.iter().rposition(|change| change.timestamp < timestamp).map(|i| i + 1).unwrap_or(0);
        self.changes[last..].iter().map(|change| &change.kind)
    }

    fn push(&mut self, timestamp: u64, change: ChangeKind) {
        self.changes.push(Change { timestamp, kind: change });
    }
}

#[derive(Debug, Clone)]
enum ChangeKind {
    Album_Inserted(AlbumId),
    Album_Removed(AlbumId),
    Album_name_Inserted(AlbumId, String),
    Album_name_Removed(AlbumId, String),
    Album_year_Inserted(AlbumId, u32),
    Album_year_Removed(AlbumId, u32),
    Album_album_artist_Inserted(AlbumId, ArtistId),
    Album_album_artist_Removed(AlbumId, ArtistId),
    Track_Inserted(TrackId),
    Track_Removed(TrackId),
    Track_name_Inserted(TrackId, String),
    Track_name_Removed(TrackId, String),
    Track_album_Inserted(TrackId, AlbumId),
    Track_album_Removed(TrackId, AlbumId),
    Track_artist_Inserted(TrackId, ArtistId),
    Track_artist_Removed(TrackId, ArtistId),
    Artist_Inserted(ArtistId),
    Artist_Removed(ArtistId),
    Artist_name_Inserted(ArtistId, String),
    Artist_name_Removed(ArtistId, String),
}

#[derive(Default)]
struct DbStore4 {
    timestamp: u64,

    changes: ChangeSet,

    Album_next_id: AlbumId,
    Track_next_id: TrackId,
    Playlist_next_id: PlaylistId,
    Artist_next_id: ArtistId,

    clustered_Track: BTreeMap<(AlbumId, TrackId), Track>,
    pk_Track: BTreeMap<TrackId, (AlbumId, TrackId)>,
    pk_Album: BTreeMap<AlbumId, Album>,
    pk_Artist: BTreeMap<ArtistId, Artist>,

    pk_Playlist: BTreeMap<PlaylistId, Playlist>,
    fk_Track_album: BTreeMap<(AlbumId, TrackId), ()>,
    fk_Track_artist: BTreeMap<(ArtistId, TrackId), ()>,
    fk_Album_album_artist: BTreeMap<(ArtistId, AlbumId), ()>,

    multi_Playlist_tracks: BTreeMap<(PlaylistId, TrackId), ()>, // 16 bytes per entry
    multi_Playlist_tracks_inv: BTreeMap<(TrackId, PlaylistId), ()>, // 16 bytes per entry
}

impl DbStore4 {
    fn next(&mut self) -> u64 {
        self.timestamp += 1;
        self.timestamp
    }
}

macro_rules! __ignore {
    ($($tts:tt)*) => {};
}

fn range_helper<A: Idx, B: Idx>(
    a: impl RangeBounds<A>,
) -> (Bound<(A, B)>, Bound<(A, B)>) {
    let start = match a.start_bound() {
        Bound::Included(x) => Bound::Included((*x, B::MIN)),
        Bound::Excluded(x) => Bound::Excluded((*x, B::MAX)),
        Bound::Unbounded => Bound::Unbounded,
    };
    let end = match a.end_bound() {
        Bound::Included(x) => Bound::Included((*x, B::MAX)),
        Bound::Excluded(x) => Bound::Excluded((*x, B::MIN)),
        Bound::Unbounded => Bound::Unbounded,
    };
    (start, end)
}

macro_rules! impl_rel {
    (
        $r:ident
        primary key ($pk:ident: $pkty:ty)
        attributes ($($attr:ident : $attr_ty:ty),*)
        foreign keys ($($fk:ident : $fk_ref:ident),*)
        nullable foreign keys ($($nullfk:ident : $nullfk_ref:ident),*)
        $(cluster ($($cluster_attr:ident),*))?
        delete cascade ($($cascade:ident . $cascade_fk:ident),*)
        delete nullify ($($nullify:ident . $nullify_fk:ident),*)
        delete deny ($($deny:ident . $deny_fk:ident),*)
    ) => {
        // $r: Relation (e.g. Album, Track)
        // $fk: Foreign key (e.g. album, artist)
        // $fk_ref: Referenced entity (e.g. Album, Artist)
        // $cascade.$cascade_fk: foreign-key references to $r with cascade delete (e.g. Track.album)
        // $nullify.$nullify_fk: foreign-key references to $r with nullify delete
        // $deny.$deny_fk: foreign-key references to $r with deny delete

        paste! {
            impl $r {
                fn before_insert(db: &DbStore4, inserting: &$r) -> Result<(), Error> {
                    // check validity of primary key
                    if db.[<pk_ $r>].contains_key(&inserting.$pk) {
                        return Err(Error::EntityNotFound);
                    }
                    // check that foreign keys are valid
                    $( if !db.[< pk_ $fk_ref >].contains_key(&inserting.$fk) { return Err(Error::ForeignKeyViolation);} )*
                    $( if let Some(fk) = inserting.$nullfk { if !db.[< pk_ $nullfk_ref >].contains_key(&fk) { return Err(Error::ForeignKeyViolation); }} )*
                    Ok(())
                }

                fn before_delete(db: &DbStore4, deleted: &$r) -> Result<(), Error> {
                    // check that cascading deletes are valid
                    $(
                        for ((_, v),_) in db.[< fk_ $cascade _ $cascade_fk >].range(range_helper(deleted.$pk..=deleted.$pk)) {
                            $cascade::before_delete(db, $cascade::fetch(db, *v).expect("foreign key integrity error"))?;
                        }
                    )*
                    // deny delete if there's any referencing entity
                    $(
                        if db.[< fk_ $deny _ $deny_fk >].contains_key(&deleted.$pk) {
                            return Err(Error::RelationshipDeniedDelete);
                        }
                    )*
                    Ok(())
                }


                fn fetch(db: &DbStore4, key: $pkty) -> Result<&$r, Error> {
                    let v = db.[<pk_ $r>].get(&key).ok_or(Error::EntityNotFound)?;
                    $(
                        __ignore!($($cluster_attr)*);
                        let v = db.[<clustered_ $r>].get(&v).ok_or(Error::EntityNotFound)?;
                    )?
                    Ok(v)
                }

                fn fetch_mut(db: &mut DbStore4, key: $pkty) -> Result<&mut $r, Error> {
                    let v = db.[<pk_ $r>].get_mut(&key).ok_or(Error::EntityNotFound)?;
                    $(
                        __ignore!($($cluster_attr)*);
                        let v = db.[<clustered_ $r>].get_mut(&v).ok_or(Error::EntityNotFound)?;
                    )?
                    Ok(v)
                }

                fn all(db: &DbStore4) -> impl Iterator<Item = &$r> {
                    let iter = db.[<pk_ $r>].values();
                    $(
                        __ignore!($($cluster_attr)*);
                        let iter = db.[<clustered_ $r>].values();
                    )?
                    iter
                }

                fn delete(db: &mut DbStore4, key: $pkty) -> Result<$r, Error> {
                    let v = Self::fetch(db, key)?;
                    Self::before_delete(db, v)?;
                    let deleted = Self::delete_inner(db, key)?;
                    Ok(deleted)
                }

                fn delete_inner(db: &mut DbStore4, key: $pkty) -> Result<$r, Error> {
                    let timestamp = db.timestamp;
                    let deleted = db.[<pk_ $r>].remove(&key).unwrap();
                    $(
                        __ignore!($($cluster_attr)*);
                        let deleted = db.[<clustered_ $r>].remove(&deleted).unwrap();
                    )?

                    // record the change
                    let timestamp = db.timestamp;
                    $(db.changes.push(timestamp, ChangeKind::[< $r _ $attr _Removed >](deleted.$pk, deleted.$attr.clone()));)*
                    $(db.changes.push(timestamp, ChangeKind::[< $r _ $fk _Removed >](deleted.$pk, deleted.$fk));)*
                    $(if let Some(fk) = deleted.$nullfk { db.changes.push(timestamp, ChangeKind::[< $r _ $nullfk _Removed >](deleted.$pk, fk)); })*
                    db.changes.push(timestamp, ChangeKind::[< $r _Removed >](deleted.$pk));

                     // update foreign key indices
                    $( db.[< fk_ $r _ $fk >].remove(&(deleted.$fk, deleted.id));)*
                    $( if let Some(fk) = deleted.$nullfk { db.[< fk_ $r _ $nullfk >].remove(&(fk, deleted.id)); })*

                    // delete cascade
                    $(
                        let to_delete = db.[< fk_ $cascade _ $cascade_fk >].range(range_helper(deleted.$pk..=deleted.$pk)).map(|((_, v),_)| *v).collect::<Vec<_>>();
                        for v in to_delete {
                            // skip before_delete since it's already been done
                            $cascade::delete_inner(db, v)?;
                        }
                    )*
                    // nullify
                    $(
                        let to_nullify = db.[< fk_ $nullify _ $nullify_fk >].range(range_helper(deleted.$pk..=deleted.$pk)).map(|((_, v),_)| *v).collect::<Vec<_>>();
                        for v in to_nullify {
                            $nullify::fetch_mut(db, v).unwrap().$nullify_fk = None;
                            // TODO update index
                        }
                    )*

                    Ok(deleted)
                }

                fn insert(db: &mut DbStore4, f: impl FnOnce($pkty) -> $r) -> Result<$pkty, Error> {
                    let id = db.[<$r _next_id>].next();
                    let val = f(id);

                    Self::before_insert(db, &val)?;

                    // first, update foreign key indices
                    $( db.[< fk_ $r _ $fk >].insert((val.$fk, val.$pk), ()); )*
                    $( if let Some(fk) = val.$nullfk { db.[< fk_ $r _ $nullfk >].insert((fk, val.$pk), ()); } )*

                    // record the change
                    let timestamp = db.timestamp;
                    db.changes.push(timestamp, ChangeKind::[< $r _Inserted >](val.$pk));
                    $(db.changes.push(timestamp, ChangeKind::[< $r _ $attr _Inserted >](val.$pk, val.$attr.clone()));)*
                    $(db.changes.push(timestamp, ChangeKind::[< $r _ $fk _Inserted >](val.$pk, val.$fk));)*
                    $(if let Some(fk) = val.$nullfk { db.changes.push(timestamp, ChangeKind::[< $r _ $nullfk _Inserted >](val.$pk, fk)); })*

                    // insert
                    let pk = val.$pk;
                    $(
                        // insert into custom clustered index
                        let key = ($(val.$cluster_attr,)*);
                        db.[<clustered_ $r>].insert(key, val);
                        let val = key;
                    )?

                    // insert into pk index
                    db.[<pk_ $r>].insert(pk, val);
                    db.[<$r _next_id>] = id;
                    Ok(id)
                }


                // foreign key navigation
                $(
                    fn $fk(self, db: &DbStore4) -> Result<&$fk_ref, Error> {
                        $fk_ref::fetch(db, self.$fk)
                    }
                )*

                fn update(db: &mut DbStore4, key: $pkty, f: impl FnOnce(&mut $r)) -> Result<(), Error> {
                    // update is delete + insert
                    let mut val = $r::delete(db, key)?;
                    f(&mut val);
                    $r::insert(db, |_| val)?;
                    Ok(())
                }
            }

            impl $pkty {
                fn delete(self, db: &mut DbStore4) -> Result<$r, Error> {
                    $r::delete(db, self)
                }

                fn fetch(self, db: &DbStore4) -> Result<&$r,Error> {
                    $r::fetch(db, self)
                }

                // foreign key setters
                $(
                    fn [<set_ $fk>](self, db: &mut DbStore4, $fk: <$fk_ref as Entity>::Key) -> Result<(), Error> {
                        let timestamp = db.timestamp;
                        let val = $r::fetch_mut(db, self)?;
                        let old_fk = std::mem::replace(&mut val.$fk, $fk);

                        // TODO unique constraints
                        db.[< fk_ $r _ $fk >].remove(&(old_fk, self));
                        db.changes.push(timestamp, ChangeKind::[<$r _ $fk _Removed>](self, old_fk));
                        db.[< fk_ $r _ $fk >].insert(($fk, self), ());
                        db.changes.push(timestamp, ChangeKind::[<$r _ $fk _Inserted>](self, $fk));
                        Ok(())
                    }
                )*

                $(
                    fn [<set_ $nullfk>](self, db: &mut DbStore4, $nullfk: Option<<$nullfk_ref as Entity>::Key>) -> Result<(), Error> {
                        let val = $r::fetch_mut(db, self)?;
                        let old_nullfk = std::mem::replace(&mut val.$nullfk, $nullfk);

                        if let Some(fk) = old_nullfk {
                            db.[< fk_ $r _ $nullfk >].remove(&(fk, self));
                        }
                        if let Some(fk) = $nullfk {
                            db.[< fk_ $r _ $nullfk >].insert((fk, self), ());
                        }
                        Ok(())
                    }
                )*

                // attribute setters
                $(
                    fn [<set_ $attr>](self, db: &mut DbStore4, $attr: $attr_ty) -> Result<(), Error> {
                        let val = $r::fetch_mut(db, self)?;
                        let old = std::mem::replace(&mut val.$attr, $attr.clone());
                        let timestamp = db.timestamp;
                        db.changes.push(timestamp, ChangeKind::[<$r _ $attr _Removed>](self, old));
                        db.changes.push(timestamp, ChangeKind::[<$r _ $attr _Inserted>](self, $attr));
                        Ok(())
                    }
                )*
            }
        }
    };
}

impl_rel!(Track
    primary key (id: TrackId)
    attributes (name: String)
    foreign keys (album: Album, artist: Artist)
    nullable foreign keys ()
    cluster (album,id)
    delete cascade ()
    delete nullify ()
    delete deny ()
);

impl_rel!(Album
    primary key (id: AlbumId)
    attributes (name: String, year: u32)
    foreign keys ()
    nullable foreign keys (album_artist: Artist)
    delete cascade (Track . album)
    delete nullify ()
    delete deny ()
);

impl_rel!(Artist
    primary key (id: ArtistId)
    attributes (name: String)
    foreign keys ()
    nullable foreign keys ()
    delete cascade (Track . artist)
    delete nullify (Album . album_artist)
    delete deny ()
);

/*


    for track_id in db.fk_Track_artist.keys(artist_id) {
      if let album_id = db.pk_Track[track_id].album {
        if let album_artist_id = db.pk_Album[album_id].album_artist {
          // we have every key

        }
      }
    }

    for diff in db.changes.since(t).filter_map(|c| { match c { ChangeKind::[<$r _ $attr _Inserted>](id, val) => Some(id)), _ => None }) {

    });

    for $pk in db.[<fk_ $r _ $fk>].range($fk) {
      let $r = $r::fetch(db, $pk).unwrap();
      // do something with $r
    }

*/

macro_rules! delta_loop {
    (@insert $db:expr, () $b:block) => {
        $b
    };

    (@remove $db:expr, () $b:block) => {
        $b
    };

    (@insert $db:expr, ($lhs:ident >> $p:pat, $($rest:tt)*) $b:block) => {
        paste!{
            let Ok($p) = $lhs.fetch($db) else { continue };
            delta_loop!(@insert $db, ($($rest)*) $b);
        }
    };

    (@remove $db:expr, ($lhs:ident >> $p:pat, $($rest:tt)*) $b:block) => {
        paste!{
            let Ok($p) = $lhs.fetch($db) else { continue };
            delta_loop!(@remove $db, ($($rest)*) $b);
        }
    };

    (@insert $db:expr, ($lhs:ident << $r:ident . $fk:ident == $id:ident, $($rest:tt)*) $b:block) => {
        paste!{
            for $lhs in $db.[< fk_ $r _ $fk >].range(range_helper($id..=$id)).map(|((_, v),_)| *v) {
                delta_loop!(@insert $db, ($($rest)*) $b);
            }
        }
    };

    (@remove $db:expr, ($lhs:ident << $r:ident . $fk:ident == $id:ident, $($rest:tt)*) $b:block) => {
        paste!{
            for $lhs in $db.[< fk_ $r _ $fk >].range(range_helper($id..=$id)).map(|((_, v),_)| *v) {
                delta_loop!(@remove $db, ($($rest)*) $b);
            }
        }
    };

    (
        $db:expr,
        $t:expr,
        $(($r:ident . $attr:ident $p:pat => $($rest:tt)*))*
        $insert_block:block
        $remove_block:block
    ) => {
        paste!{
            for c in $db.changes.since($t)
            {
                match c {
                    $(
                        ChangeKind::[< $r _ $attr _Inserted >] $p => {
                            delta_loop!(@insert $db, ($($rest)*) $insert_block);
                        }
                        ChangeKind::[< $r _ $attr _Removed >] $p => {
                            delta_loop!(@remove $db, ($($rest)*) $remove_block);
                        }
                    )*
                    _ => {}
                }
            }
        }
    };
}

/*
    Track   { id: track_id, album: album_id, artist: artist_id, name: title, .. },
    Album   { id: album_id, album_artist: album_artist_id, title: album_title, .. },
    Artist  { id: album_artist_id, name: album_artist_name, .. },
    Artist  { id: artist_id, name: track_artist_name, .. },

    Problem with removal:
    - if a Track is removed, we don't have Track.album anymore to join to

    Solutions:
    - somehow keep the previous version of the DB => hard, need to keep a copy of the entire DB or use OrdMap which we decided not to use
    - have deletion records keep all attributes
        - ChangeKind::Track_Removed(Track { id: track_id, album: album_id, artist: artist_id, name: title, .. })
        - this way we have the Track.album
        - problem #2: album might have been deleted as well, thus we don't know the album title or the album artist id
            -> when looking up deleted values, also take into account deletion records?
            -> "fetch_at" method that returns the value at a given timestamp, somehow
    - Delete
*/

fn join_test(db: &DbStore4, t: u64) {

    delta_loop!(db, t,
        (Artist.name(artist_id, _) =>
            track_id << Track.artist == artist_id,
            track_id >> Track { name: title, album: album_id, .. },
            album_id >> Album { name: album_title, album_artist: Some(album_artist_id), .. },
            album_artist_id >> Artist { name: album_artist_name, .. },
        )
        (Album.album_artist(album_id, album_artist_id) =>
            track_id << Track.album == album_id,
            track_id >> Track { name: title, album: album_id, .. },
            album_id >> Album { name: album_title, album_artist: Some(album_artist_id), .. },
            album_artist_id >> Artist { name: album_artist_name, .. },
        )
        (Track.album(track_id, album_id) =>
            track_id >> Track { name: title, album: album_id, .. },
            album_id >> Album { name: album_title, album_artist: Some(album_artist_id), .. },
            album_artist_id >> Artist { name: album_artist_name, .. },
        )


        {
            println!("Inserted: {} from {} by {} ", title, album_title, album_artist_name);
        }

        /*(Track.name(track_id) =>
            track_id >> Track { name: title, album: album_id, artist: artist_id, .. },
            delete (track_id, album_id, .., ..)
        )*/

        {
            println!("Removed: {} from {} by {} ", title, album_title, album_artist_name);
        }
    );
}


trait Query<DB> {
    type Key;
    type Output;    // usually Box<dyn Widget>

    fn update(&mut self, db: &DB, change: &DB::Change) -> Delta<Self::Key, Self::Output>;
}

struct fn_root_0 {
    //nodes: BTreeMap<>
}

impl Query<TrackDB> for album_0 {
    fn update(&mut self, db: &TrackDB, change: &TrackDB::Change) -> Delta<Self::Key, Self::Output> {

    }
}

struct NodeList<K,V> {
    nodes: BTreeMap<K,V>
}

impl<K, V> NodeList<K,V> {
    fn update(&mut self, db: &DB, change: &DB::Change) {
        fn_root_0::update(db, change).apply(self.nodes);
        fn_root_1::update(db, change).apply(self.nodes);
        // ...
    }
}


/*

    fn track(id: TrackId) {
        ui! {
            select Track {id, name, album}, Album {id: album, title: album_title} {
                // [#1]
                // (dep={Track,Album})
                // (address=(parent, id, album, #1))
                // (add watch: Track.id, Album.id)
                // ...
            }
        }
    }

    fn album(album_id: AlbumId) {
        ui! {
            select Track {id, album=album_id} {
                // [#0]
                // (dep={Track})
                // (add watch: Track.id)
                // (address=(parent, id, #0))
                track(id)           // FIXME: this depends on Album as well, but nothing says so
            }
        }
    }

Resulting watches:

    // for block #0
    (Track.id(0)) -> ([#0], [.., 0, #0])
    ...
    (Track.id(N)) -> ([#0], [.., N, #0])

    (Track.id) -> ([#1], [parent, id, album, #1])
    (Album.id) -> ([#0], [parent, id, album, #0])

 */

////////////////////////////////////////////////////////////////////////////////////////////////////
/*#[derive(Clone)]
struct Db {
    track_db: TrackDbStore,
}

impl HasStore<TrackDbStore> for Db {
    fn store(&self) -> &TrackDbStore {
        &self.track_db
    }
    fn store_mut(&mut self) -> &mut TrackDbStore {
        &mut self.track_db
    }
}*/

#[test]
fn test_structs_and_enums_01() {
    type Db = DbStore4;

    let mut db = Db::default();
    let db = &mut db;

    let mut add_artist = |db: &mut Db, name: &str| {
        let artist = Artist::all(db)
            .find(|artist| artist.name == name)
            .map(|x| x.id);
        artist.unwrap_or_else(|| {
            Artist::insert(db, |id| Artist {
                id,
                name: name.to_string(),
            })
            .unwrap()
        })
    };

    let mut add_album = |db: &mut Db, name: &str, album_artist: &str, year: u32| {
        let album_artist = add_artist(db, album_artist);
        Album::insert(db, |id| Album {
            id,
            name: name.to_string(),
            year,
            album_artist: Some(album_artist),
        })
        .unwrap()
    };

    let mut add_track = |db: &mut Db, name: &str, artist_name: &str, album: AlbumId| {
        let artist = add_artist(db, artist_name);
        Track::insert(db, |id| Track {
            id,
            name: name.to_string(),
            album,
            artist,
        }).unwrap()
    };

    let syrufit_over = add_album(db, "over", "Syrufit", 2011);
    let voice_of_mist = add_track(db, "Voice of Mist", "Maurits\"禅\"Cornelis", syrufit_over);
    add_track(db, "Silent Story", "陽花", syrufit_over);
    add_track(
        db,
        "VAGRANT (MZC Falling Into Massive Galaxy Remix)",
        "衝動的の人",
        syrufit_over,
    );
    add_track(
        db,
        "With Me (MZC Paradigms To The Next Perspective Remix)",
        "Vivienne",
        syrufit_over,
    );
    add_track(
        db,
        "History of the Moon (MZC Rise Of The Phenomenal Core Remix)",
        "Cocoon",
        syrufit_over,
    );
    add_track(db, "Rendezvous", "Chen-U", syrufit_over);
    add_track(
        db,
        "Reflections (MZC Ever Fly By Twilight House Mix)",
        "Chen-U",
        syrufit_over,
    );
    add_track(
        db,
        "Surface Star (MZC The Myth Killed The Symbol Remix)",
        "SAKURA_bot",
        syrufit_over,
    );
    add_track(db, "Pages of A Star", "陽花", syrufit_over);
    add_track(
        db,
        "With Me (MZC Paradigms To The Next Perspective Remix Extended)",
        "Vivienne",
        syrufit_over,
    );
    add_track(
        db,
        "Reflections (MZC Ever Fly By Twilight House Mix Extended)",
        "Chen-U",
        syrufit_over,
    );

    let sally_sadomasochism = add_album(db, "サドマゾヒズム", "サリー", 2011);
    let track_enn = add_track(db, "enn～淵～", "NSY feat. 茶太", sally_sadomasochism);
    let track_sado = add_track(db, "サド", "ワニ feat. 茶太", sally_sadomasochism);
    let track_new_text = add_track(
        db,
        "新規テキストドキュメント",
        "NSY feat. IZNA",
        sally_sadomasochism,
    );
    let track_reikon = add_track(
        db,
        "霊魂ミソロジー",
        "シュリンプ feat. めらみぽっぷ",
        sally_sadomasochism,
    );
    let track_koumori = add_track(db, "コウモリ", "ワニ feat. IZNA", sally_sadomasochism);
    let track_loop = add_track(db, "L∞p", "ワニ feat. ランコ", sally_sadomasochism);
    let track_plaza = add_track(
        db,
        "plaza Blue age",
        "NSY feat. ランコ",
        sally_sadomasochism,
    );

    let touhou_jihen = add_album(db, "拈華微笑", "東方事変", 2019);
    let track_kokoronohame = add_track(db, "ココロノハナ", "NSY feat. IZNA", touhou_jihen);
    let track_kaitaishoujo = add_track(db, "解体少女", "NSY feat. IZNA", touhou_jihen);
    let track_reset_me = add_track(db, "リセットミー", "NSY feat. IZNA", touhou_jihen);
    let track_zoku = add_track(db, "俗-zoku-", "NSY feat. IZNA", touhou_jihen);
    let track_usaginimo = add_track(db, "兎にも角にも", "NSY feat. IZNA", touhou_jihen);
    let track_crazy_tonight = add_track(db, "Crazy☆Tonight", "NSY feat. IZNA", touhou_jihen);
    let track_imakokoni = add_track(db, "イマココニアルモノ", "NSY feat. IZNA", touhou_jihen);

    // print all tracks

    let print_all_tracks = |db: &Db| {
        for track in Track::all(db) {
            eprintln!(
                "#{} [{}] {} ({}) ",
                track.id.to_u32(),
                track.album.fetch(db).unwrap().name,
                track.name,
                track.artist.fetch(db).unwrap().name,
            );
        }
    };

    eprintln!("====== Initial state ======");
    print_all_tracks(db);

    //////////////////////////////////////////////////////////////////
    let old_timestamp = db.next();

    // remove the first track of each album
    let first_track = |db: &Db, album: AlbumId| {
        Track::all(db)
            .find(|track| track.album == album)
            .unwrap()
            .id
    };
    Track::delete(db, first_track(db, syrufit_over)).unwrap();
    Track::delete(db, first_track(db, sally_sadomasochism)).unwrap();
    Track::delete(db, first_track(db, touhou_jihen)).unwrap();

    eprintln!("====== After removing the first track of each album ======");
    print_all_tracks(db);

    //////////////////////////////////////////////////////////////////
    eprintln!("====== Join test ======");
    for change in db.changes.since(old_timestamp) {
        eprintln!("- {:?}", change);
    }
    join_test(db, old_timestamp);

    //////////////////////////////////////////////////////////////////
    let old_timestamp = db.next();

    // test deletion cascade
    syrufit_over.delete(db).unwrap();

    eprintln!("====== After deleting the album 'over' ======");
    print_all_tracks(db);

    //////////////////////////////////////////////////////////////////
    let old_timestamp = db.next();

    // move one track to another album
    track_koumori.set_album(db, touhou_jihen).unwrap();
    track_usaginimo.set_album(db, sally_sadomasochism).unwrap();

    eprintln!("====== After moving tracks to other albums ======");
    print_all_tracks(db);

    // show track changes
    eprintln!("====== Track changes ======");
    for change in db.changes.changes.iter() {
        eprintln!("- {:?}", change);
    }

    //////////////////////////////////////////////////////////////////
    eprintln!("====== Join test ======");
    join_test(db, old_timestamp);


    // album @ Album { id: album_id, name, .. },
    // track @ Track { id: track_id, name: track_name, album: album_id }

    // changes whenever an album is added/removed
    // or when a track is added/removed

    // only join on foreign keys
    // when an album change, join on all tracks in the current DB, without the tracks added, removing the tracks removed
    // same with tracks

    // Album added -> implies all tracks modified (added or foreign key modified)
    // Album removed -> implies all tracks modified (removed or foreign key modified)
    //
    // Issue: updating an unrelated property will

    /*let mut delta = BTreeMap::new();

    for c in db.change_Album.since(old_timestamp) {
        match c {
            ChangeKind::Inserted(album) => {
                //eprintln!("Inserted album: {}", album.name);
                for track in Track::all(db) {
                    if track.album == album.id {
                        delta.insert((album.id, track.id), (album, track));
                    }
                }
            }
            ChangeKind::Removed(album) => {
                for track in Track::all(db) {
                    if track.album == album.id {
                        delta.remove(&(album.id, track.id));
                    }
                }
            }
        }
    }*/



    /*let snapshot = db.clone();

    // add extra tracks to all albums to test snapshots
    add_track(db, "Extra track 1", "Extra artist", syrufit_over);
    add_track(db, "Extra track 2", "Extra artist", sally_sadomasochism);
    add_track(db, "Extra track 3", "Extra artist", touhou_jihen);

    // remove the first track of each album
    //let first_track =
    //    |db: &dyn TrackDb, album: Album| album.tracks(db).iter().next().unwrap().clone();
    db.remove::<Track>(voice_of_mist)
        .expect("track not found");
    db.remove::<Track>(track_enn)
        .expect("track not found");
    db.remove::<Track>(track_kokoronohame)
        .expect("track not found");

    // update some tracks
    let nsy_izna = add_artist(db, "NSY feat. IZNA (updated)");
    track_zoku.set_artist(db, nsy_izna).unwrap();
    track_kaitaishoujo.set_artist(db, nsy_izna).unwrap();
    track_reset_me.set_artist(db, nsy_izna).unwrap();

    // change the album of some tracks
    track_koumori.set_album(db, touhou_jihen).unwrap();
    track_usaginimo.set_album(db, sally_sadomasochism).unwrap();
    track_crazy_tonight.set_album(db, syrufit_over).unwrap();*/

    /*eprintln!("\n------\nAlbum tracks: \n------");
    for ((album, album_row), (track, track_row)) in
        join(Album::query_all(), Rel_Album_tracks, |x| x).iter(db)
    {
        eprintln!(
            "[{}] {} ({}) ",
            album_row.name,
            track_row.name,
            track_row.artist.name(db)
        );
    }

    eprintln!("\n------\nDelta album tracks: \n------");
    for track in join(Album::query_all(), Rel_Album_tracks, |x| x).delta(db, &snapshot) {
        match track {
            Delta::Insert(((album, album_row), (track, track_row))) => eprintln!(
                "Insert: [{}] {} ({}) ",
                album_row.name,
                track_row.name,
                track_row.artist.name(db)
            ),
            Delta::Remove(((album, album_row), (track, track_row))) => eprintln!(
                "Remove: [{}] {} ({}) ",
                album_row.name,
                track_row.name,
                track_row.artist.name(&snapshot)
            ),
            Delta::Update {
                old,
                new: ((album, album_row), (track, track_row)),
            } => eprintln!(
                "Update: [{}] {} ({}) ",
                album_row.name,
                track_row.name,
                track_row.artist.name(db)
            ),
        }
    }

    // triple-join:

    eprintln!("\n------\nDelta query artists: \n------");
    for delta in join(
        join(Album::query_all(), Rel_Album_tracks, |x| x),
        Rel_Track_artist,
        |(_, y)| y,
    )
    .delta(db, &snapshot)
    {
        match delta {
            Delta::Insert((((album, album_row), (track, track_row)), (artist, artist_row))) => {
                eprintln!(
                    "Insert: [{}] {} ({}) ",
                    album_row.name, track_row.name, artist_row.name
                )
            }
            Delta::Remove((((album, album_row), (track, track_row)), (artist, artist_row))) => {
                eprintln!(
                    "Remove: [{}] {} ({}) ",
                    album_row.name, track_row.name, artist_row.name
                )
            }
            Delta::Update {
                old,
                new: (((album, album_row), (track, track_row)), (artist, artist_row)),
            } => {
                eprintln!(
                    "Update: [{}] {} ({}) ",
                    album_row.name, track_row.name, artist_row.name
                )
            }
        }
    }

    eprintln!("------\nDelta artists: \n------");
    for artist in Artist::query_all().delta(db, &snapshot) {
        match artist {
            Delta::Insert(artist) => eprintln!("Insert: {}", artist.1.name),
            Delta::Remove(artist) => eprintln!("Remove: {}", artist.1.name),
            Delta::Update { old, new } => eprintln!("Update: {}", new.1.name),
        }
    }*/

    /*// want tuples like:
    // (group_index, album, track_group_index, track)
    eprintln!("\n------\nTitle of removed tracks: \n------");
    for track in query_album_tracks(Album::query_all())
        .map(|t, db| t.name(db).clone())
        .delta(db, &snapshot)
    {
        match track {
            Delta::Insert(track) => eprintln!("Insert: {}", track),
            Delta::Remove(track) => eprintln!("Remove: {}", track),
            Delta::Update(track) => eprintln!("Update: {}", track),
        }
    }

    eprintln!("\n------\nAlbum tracks with join: \n------");
    join_album_tracks(Album::query_all(), |album| album)
        .iter(db)
        .for_each(|(album, track)| {
            eprintln!(
                "({}) [{}] {} ({}) ",
                album.name(db),
                track.album(db).name(db),
                track.name(db),
                track.artist(db).name(db)
            )
        });

    // This query gives all that is needed to update a UI
    eprintln!("\n------\nAlbum tracks with join delta: \n------");
    join_album_tracks(Album::query_all(), |album| album)
        .delta(db, &snapshot)
        .for_each(|track| {
            eprintln!("{:?}", track);
        });

    // all unique artists
    eprintln!("\n------\nAll unique artists: \n------");
    for artist in Artist::all(db) {
        eprintln!("{}", artist.name(db));
    }

    // artists delta
    eprintln!("\n------\nAll unique artists delta: \n------");
    for artist in Artist::query_all().delta(db, &snapshot) {
        match artist {
            Delta::Insert(artist) => eprintln!("Insert: {}", artist.name(db)),
            Delta::Remove(artist) => eprintln!("Remove: {}", artist.name(&snapshot)),
            Delta::Update(artist) => eprintln!("Update: {}", artist.name(db)),
        }
    }

    // Artist tracks
    eprintln!("\n------\nArtist tracks: \n------");
    join_artist_tracks(Artist::query_all(), |artist| artist)
        .iter(db)
        .for_each(|(artist, track)| {
            eprintln!(
                "({}) [{}] {} ({}) ",
                artist.name(db),
                track.album(db).name(db),
                track.name(db),
                track.artist(db).name(db)
            )
        });

    // Artist tracks delta
    eprintln!("\n------\nArtist tracks delta: \n------");
    join_artist_tracks(Artist::query_all(), |artist| artist)
        .delta(db, &snapshot)
        .for_each(|track| match track {
            Delta::Insert((artist, track)) => eprintln!(
                "Insert: ({}) [{}] {} ({}) ",
                artist.name(db),
                track.album(db).name(db),
                track.name(db),
                track.artist(db).name(db)
            ),
            Delta::Remove((artist, track)) => eprintln!(
                "Remove: ({}) [{}] {} ({}) ",
                artist.name(&snapshot),
                track.album(&snapshot).name(&snapshot),
                track.name(&snapshot),
                track.artist(&snapshot).name(&snapshot)
            ),
            Delta::Update((artist, track)) => eprintln!(
                "Update: ({}) [{}] {} ({}) ",
                artist.name(db),
                track.album(db).name(db),
                track.name(db),
                track.artist(db).name(db)
            ),
        });

    eprintln!("\n------\nAlbum tracks with join: \n------");
    join(Album::query_all(), Rel_Album_tracks, |album| album)
        .delta(db, &snapshot)
        .for_each(|track| match track {
            Delta::Insert((album, track)) => eprintln!(
                "Insert: [{}] {} ({}) ",
                album.name(db),
                track.name(db),
                track.artist(db).name(db)
            ),
            Delta::Remove((album, track)) => eprintln!(
                "Remove: [{}] {} ({}) ",
                album.name(&snapshot),
                track.name(&snapshot),
                track.artist(&snapshot).name(&snapshot)
            ),
            Delta::Update((album, track)) => eprintln!(
                "Update: [{}] {} ({}) ",
                album.name(db),
                track.name(db),
                track.artist(db).name(db)
            ),
        });

    // triple join: updated artists in updated albums
    //eprintln!("\n------\nTriple join: updated artists in updated albums: \n------");*/
}
