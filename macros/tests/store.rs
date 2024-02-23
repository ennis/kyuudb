use kyuudb::HasStore;
use kyuudb_macros::store;


store!{
    pub store TrackDb;

    //
    Album(
        name: String,
        album_artist: String,
        year: u32,
        rel tracks: Track*.album
    );

    Track(
        name: String,
        artist: String,
        rel album: Album.tracks
    );


    Playlist(
        name: String,
        rel tracks: Track*
    );
}


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


#[test]
fn test_structs_and_enums_01() {

    /*Maurits"禅"Cornelis - Voice of Mist
陽花 - Silent Story
衝動的の人 - VAGRANT (MZC Falling Into Massive Galaxy Remix)
Vivienne - With Me (MZC Paradigms To The Next Perspective Remix)
Cocoon - History of the Moon (MZC Rise Of The Phenomenal Core Remix)
Chen-U - Rendezvous
Chen-U - Reflections (MZC Ever Fly By Twilight House Mix)
SAKURA_bot - Surface Star (MZC The Myth Killed The Symbol Remix)
陽花 - Pages of A Star
Vivienne - With Me (MZC Paradigms To The Next Perspective Remix Extended)
Chen-U - Reflections (MZC Ever Fly By Twilight House Mix Extended)*/

    let mut db = Db {
        track_db: TrackDbStore::new(),
    };

    let db = &mut db;

    let album = AlbumRow {
        name: "over".to_string(),
        album_artist: "Syrufit".to_string(),
        year: 2011,
        tracks: vec![],
    }.insert(db).unwrap();

    let _ = TrackRow {
        name: "Voice of Mist".to_string(),
        artist: "Maurits\"禅\"Cornelis".to_string(),
        album,
    }.insert(db).unwrap();

    let _ = TrackRow {
        name: "Silent Story".to_string(),
        artist: "陽花".to_string(),
        album,
    }.insert(db).unwrap();

    let _ = TrackRow {
        name: "VAGRANT (MZC Falling Into Massive Galaxy Remix)".to_string(),
        artist: "衝動的の人".to_string(),
        album,
    }.insert(db).unwrap();

    let _ = TrackRow {
        name: "With Me (MZC Paradigms To The Next Perspective Remix)".to_string(),
        artist: "Vivienne".to_string(),
        album,
    }.insert(db).unwrap();

    let _ = TrackRow {
        name: "History of the Moon (MZC Rise Of The Phenomenal Core Remix)".to_string(),
        artist: "Cocoon".to_string(),
        album,
    }.insert(db).unwrap();

    let _ = TrackRow {
        name: "Rendezvous".to_string(),
        artist: "Chen-U".to_string(),
        album,
    }.insert(db).unwrap();

    let _ = TrackRow {
        name: "Reflections (MZC Ever Fly By Twilight House Mix)".to_string(),
        artist: "Chen-U".to_string(),
        album,
    }.insert(db).unwrap();

    let _ = TrackRow {
        name: "Surface Star (MZC The Myth Killed The Symbol Remix)".to_string(),
        artist: "SAKURA_bot".to_string(),
        album,
    }.insert(db).unwrap();

    let _ = TrackRow {
        name: "Pages of A Star".to_string(),
        artist: "陽花".to_string(),
        album,
    }.insert(db).unwrap();

    let _ = TrackRow {
        name: "With Me (MZC Paradigms To The Next Perspective Remix Extended)".to_string(),
        artist: "Vivienne".to_string(),
        album,
    }.insert(db).unwrap();

    let _ = TrackRow {
        name: "Reflections (MZC Ever Fly By Twilight House Mix Extended)".to_string(),
        artist: "Chen-U".to_string(),
        album,
    }.insert(db).unwrap();

    //track.remove(&mut db).unwrap();

    for track in album.tracks(db) {
        println!(" - {} - {} ", track.name(db), track.artist(db));
    }
}