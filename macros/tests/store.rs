use kyuudb::{join, Delta, Entity, EntityStore, HasStore, Query, Rel};
use kyuudb_macros::store;
use std::io::Read;
use std::iter::once;
use std::marker::PhantomData;

store! {
    /// Database schema for a music library.
    pub store TrackDb;

    /// Represents an album.
    Album(
        /// Name of the album.
        name: String,
        /// Album artist.
        rel album_artist: Artist,
        /// Year of release.
        year: u32,
        /// Tracks in the album.
        rel tracks: Track*.album,

        // if you don't want to track the tracks, you can use a query instead
        // query tracks: Track* = Track.album == this
    );


    /// Represents a track of an album.
    Track(
        /// Name of the track.
        name: String,
        /// Track artist.
        rel artist: Artist.tracks,
        /// Album the track is part of.
        rel album: Album.tracks
    );

    /// Represents a playlist.
    Playlist(
        /// Name of the playlist.
        name: String,
        /// Tracks in the playlist.
        rel tracks: Track*
    );

    Artist(
        /// Name of the artist.
        name: String,
        /// All songs by the artist.
        rel tracks: Track*.artist
    );
}

#[derive(Clone)]
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
}

/*
fn query_album_tracks<DB>(
    album_query: impl Query<DB, Item = Album> + Clone + 'static,
) -> impl Query<DB, Item = Track>
where
    DB: HasStore<TrackDbStore> + ?Sized,
{
    #[derive(Clone)]
    struct Q<A>(A);

    impl<A, DB> Query<DB> for Q<A>
    where
        DB: HasStore<TrackDbStore> + ?Sized,
        A: Query<DB, Item = Album> + Clone + 'static,
    {
        type Item = Track;

        fn iter<'a>(self, db: &'a DB) -> impl Iterator<Item = Track> + 'a {
            self.0
                .iter(db)
                .flat_map(|album| album.tracks(db).iter().cloned())
        }

        fn delta<'a>(self, db: &'a DB, prev: &'a DB) -> impl Iterator<Item = Delta<Track>> + 'a {
            self.0.iter(db).flat_map(move |album| {
                // Might be good to store the delta in a vec instead of traversing the store multiple times
                db.store()
                    .Track
                    .delta(&prev.store().Track)
                    .filter_map(move |track| match track {
                        Delta::Insert(track) => {
                            if track.album(db) == album {
                                Some(Delta::Insert(track))
                            } else {
                                None
                            }
                        }
                        Delta::Remove(track) => {
                            if track.album(prev) == album {
                                Some(Delta::Remove(track))
                            } else {
                                None
                            }
                        }
                        Delta::Update(track) => {
                            let prev_album = track.album(prev);
                            let new_album = track.album(db);

                            match (prev_album, new_album) {
                                (prev_album, new_album)
                                    if prev_album == new_album && new_album == album =>
                                {
                                    Some(Delta::Update(track))
                                }
                                (prev_album, _) if prev_album == album => {
                                    Some(Delta::Remove(track))
                                }
                                (_, new_album) if new_album == album => Some(Delta::Insert(track)),
                                _ => None,
                            }
                        }
                    })
            })
        }
    }

    Q(album_query)
}*/

struct Rel_Album_tracks;
struct Rel_Track_album;
struct Rel_Track_artist;
struct Rel_Artist_tracks;

impl Rel for Rel_Album_tracks {
    type Src = Album;
    type Dst = Track;
    type Inverse = Rel_Track_album;
    fn targets(src: &AlbumRow) -> impl Iterator<Item = Track> + '_ {
        src.tracks.iter().cloned()
    }
}

impl Rel for Rel_Track_album {
    type Src = Track;
    type Dst = Album;
    type Inverse = Rel_Album_tracks;
    fn targets(src: &TrackRow) -> impl Iterator<Item = Album> + '_ {
        once(src.album)
    }
}

impl Rel for Rel_Track_artist {
    type Src = Track;
    type Dst = Artist;
    type Inverse = Rel_Artist_tracks;
    fn targets(src: &TrackRow) -> impl Iterator<Item = Artist> + '_ {
        once(src.artist)
    }
}

impl Rel for Rel_Artist_tracks {
    type Src = Artist;
    type Dst = Track;
    type Inverse = Rel_Track_artist;
    fn targets(src: &ArtistRow) -> impl Iterator<Item = Track> + '_ {
        src.tracks.iter().cloned()
    }
}

/*
// join on a many-to-one relationship, incremental on the right side
macro_rules! decl_join {
    ([$store:ident] $f:ident ($left:ident . $left_rel:ident => $right:ident . $right_rel:ident) ) => {
        fn $f<DB, Q, T, FI>(query: Q, in_fn: FI) -> impl Query<DB, Item = (T, $right)>
        where
            DB: HasStore<$store> + ?Sized,
            Q: Query<DB, Item = T> + Clone + 'static,
            T: Clone + 'static,
            FI: Fn(T) -> $left + Clone + 'static,
        {
            #[derive(Clone)]
            struct Join<Q, FI> {
                query: Q,
                in_fn: FI,
            }

            impl<Q, T, DB, FI> Query<DB> for Join<Q, FI>
            where
                T: Clone + 'static,
                DB: HasStore<$store> + ?Sized,
                Q: Query<DB, Item = T> + Clone + 'static,
                FI: Fn(T) -> $left + Clone + 'static,
            {
                type Item = (T, $right);

                fn iter<'a>(self, db: &'a DB) -> impl Iterator<Item = (T, $right)> + 'a {
                    self.query.iter(db).flat_map(move |left| {
                        (self.in_fn)(left.clone())
                            .$left_rel(db)
                            .iter()
                            .cloned()
                            .map(move |right| (left.clone(), right))
                    })
                }

                fn delta<'a>(
                    self,
                    db: &'a DB,
                    prev: &'a DB,
                ) -> impl Iterator<Item = Delta<(T, $right)>> + 'a {
                    self.query.iter(db).flat_map(move |leftval| {
                        let left = (self.in_fn)(leftval.clone());
                        db.store()
                            .$right
                            .delta(&prev.store().$right)
                            .filter_map(move |delta| match delta {
                                Delta::Insert(right) if right.$right_rel(db) == left => {
                                    Some(Delta::Insert(right))
                                }
                                Delta::Remove(right) if right.$right_rel(prev) == left => {
                                    Some(Delta::Remove(right))
                                }
                                Delta::Update(right) => {
                                    let old = right.$right_rel(prev);
                                    let new = right.$right_rel(db);
                                    match (old, new) {
                                        (old, new) if old == new && new == left => {
                                            Some(Delta::Update(right))
                                        }
                                        (old, _) if old == left => Some(Delta::Remove(right)),
                                        (_, new) if new == left => Some(Delta::Insert(right)),
                                        _ => None,
                                    }
                                }
                                _ => None,
                            })
                            .map(move |delta| match delta {
                                Delta::Insert(right) => Delta::Insert((leftval.clone(), right)),
                                Delta::Remove(right) => Delta::Remove((leftval.clone(), right)),
                                Delta::Update(right) => Delta::Update((leftval.clone(), right)),
                            })
                    })
                }
            }

            Join { query, in_fn }
        }
    };
}*/

//decl_join!([TrackDbStore] join_album_tracks(Album.tracks => Track.album));
//decl_join!([TrackDbStore] join_artist_tracks(Artist.tracks => Track.artist));

/*
struct Join<A, B>(A, B);
impl<A, B, T, U, DB> Query<(T, U), DB> for Join<A, B> where DB: HasStore<TrackDbStore>, A: Query<T, DB>, B: Query<U, DB>
{
    fn iter(&self, db: &DB) -> impl Iterator<Item = (A::Item, B::Item)> {
        self.0.iter(db).flat_map(move |a| {
            self.1.iter(db).map(move |b| (a, b))
        })
    }

    fn delta(&self, db: &DB, prev: &DB) -> impl Iterator<Item=Delta<(T, U)>> {
        self.1.delta(db, prev).flat_map(move |b| {
            self.0.iter(db).map(move |a| {
                match b {
                    Delta::Insert(b) => Delta::Insert((a, b)),
                    Delta::Remove(b) => Delta::Remove((a, b)),
                }
            })
        })
    }
}*/

#[test]
fn test_structs_and_enums_01() {
    let mut db = Db {
        track_db: TrackDbStore::new(),
    };

    let db = &mut db;

    let add_artist = |db: &mut Db, name: &str| {
        let artist = Artist::all(db).find(|artist| artist.name(db) == name);

        artist.unwrap_or_else(|| {
            ArtistRow {
                name: name.to_string(),
                tracks: vec![],
            }
            .insert(db)
            .unwrap()
        })
    };

    let add_album = |db: &mut Db, name: &str, album_artist: &str, year: u32| {
        let album_artist = add_artist(db, album_artist);
        AlbumRow {
            name: name.to_string(),
            album_artist,
            year,
            tracks: vec![],
        }
        .insert(db)
        .unwrap()
    };

    let add_track = |db: &mut Db, name: &str, artist_name: &str, album: Album| {
        let artist = add_artist(db, artist_name);
        TrackRow {
            name: name.to_string(),
            artist,
            album,
        }
        .insert(db)
        .unwrap()
    };

    let syrufit_over = add_album(db, "over", "Syrufit", 2011);
    add_track(db, "Voice of Mist", "Maurits\"禅\"Cornelis", syrufit_over);
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

    let snapshot = db.clone();

    // add extra tracks to all albums to test snapshots
    add_track(db, "Extra track 1", "Extra artist", syrufit_over);
    add_track(db, "Extra track 2", "Extra artist", sally_sadomasochism);
    add_track(db, "Extra track 3", "Extra artist", touhou_jihen);

    // remove the first track of each album
    let first_track =
        |db: &dyn TrackDb, album: Album| album.tracks(db).iter().next().unwrap().clone();
    first_track(db, syrufit_over)
        .remove(db)
        .expect("track not found");
    first_track(db, sally_sadomasochism)
        .remove(db)
        .expect("track not found");
    first_track(db, touhou_jihen)
        .remove(db)
        .expect("track not found");

    // update some tracks
    let nsy_izna = add_artist(db, "NSY feat. IZNA (updated)");
    track_zoku.set_artist(db, nsy_izna).unwrap();
    track_kaitaishoujo.set_artist(db, nsy_izna).unwrap();
    track_reset_me.set_artist(db, nsy_izna).unwrap();

    // change the album of some tracks
    track_koumori.set_album(db, touhou_jihen).unwrap();
    track_usaginimo.set_album(db, sally_sadomasochism).unwrap();
    track_crazy_tonight.set_album(db, syrufit_over).unwrap();

    eprintln!("\n------\nAlbum tracks: \n------");
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
    }

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
