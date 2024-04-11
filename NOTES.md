# Goals
* consistency when adding and removing entities (deletion cascade)
* zero-code undo/redo
* serialization and persistence 
* copy-paste
* extensibility


# Implementation details

* _Entities_ identified by Copy+Clone objects
* Operations on _entities_
  * `is_valid(self, db)`
  * `remove(self, db)`
  * `create(db, fields...)`
* Entities: index into slotmap. If all tables are maintained on every entity change, don't need generational indices. 

Issue: `Entity::create(db)` needs the table index for the entity type. We only have the type at this point.
We don't know a lot about `db` either: if it's a type-erased database we can't statically get the table corresponding to the entity type: must do a lookup by typeid, and that's overhead.


Solution 1: `db` is a trait object that knows about specific entity types.
Issue: the db must be created with compile-time knowledge of all tables in advance

Solution 2: give up and look up the table by hashing the type-id

Solution 3: stash the table index in a global static (this assumes there's only one database). Never seen this pattern in the wild.

Note: there's no reason to have more than one DB in an application. However, if it's a global var, must make sure that there's only one instance of the global static. That's an issue if two versions of the library are used (esp. accidentally).
Makes more sense for the final application to define the database type by combining required table sets.

```

// Split the DB into "stores". One table group = set of tables and associated queries.

// To define a table group:

store! {
    store TrackDb;
    
    Album(
      name: String,
      rel tracks: Track* -> Track.album   // one side of a relationship: one-to-many
    );
    
    Track(
       // attributes
       name: String,
       year: u32,
    
       // the other side of the relationship: one-to-one
       rel album: Album  -> Album.tracks
    );
    
    Playlist(
        name: String,
        rel tracks: Track*      // one-to-many relationship, not mirrored
    );
    
    
    // Hierarchy
    Node(
        name: String,
        rel parent: Node? -> Node.children,
        rel ports: Port* -> Port.parent,
        rel children: Node* -> Node.parent,
    );
    
    Port(
        name: String,
        kind: PortKind,
        rel parent: Node -> Node.ports,
        rel connected_to: Port? -> Port.connected_to,   // optional reflexive one-to-one relationship 
    );
}

// Generated:
pub struct TrackDbStorage {
  Album: Table<...>,
  Track: Table<...>,
  Playlist: Table<...>,
  
  Album_Track_album_tracks: Table<...>,
  
  node: Table<...>,
  port: Table<...>,
  Node_Node_parent_children: Table<...>, 
}

pub struct Album(Id);
pub struct Track(Id);
pub struct Playlist(Id);

pub trait TrackDb {
  fn add_album(&self) -> Album;
}


// In the application:

type AppDb = Database![BaseTableSet, ExtendedTableSet, ...]; 

// Entity IDs:



```

# Relations

Option A: single source of truth, store relations in tables.
Codegen is easier


Types of relations:
- one (optional) to one (optional) (e.g. mentor and mentoree)
- one (optional) to one 
- one (optional) to many (e.g. album tracks)
- one to many (e.g. track artist)
- one to one (?)
- many to many

Operations:
- add relation
- break relation
- delete source
- delete target

# Alternative: focus on foreign key constraints (only one(optional) to many relationship)

Types of relations:
 
- (optional) one to one
- (optional) one to many
- one to many

Operations:
- add entity
- set relation
- delete foreign entity


One-to-one relation

    

    OneToOne::set(source, $srcrel, dest, $dstrel?) {
      // check for nulls
      if !dest && !$srcrel.optional {
        return Err(...);
      }
      if HAS_INVERSE {
        if !source && !$dstrel.optional { return Err(...) }
      }

      // short-cut if dest is the same
      if source && source.$srcrel == dest {
        return;
      }

      // validation of the inverse relation
      if HAS_INVERSE {
        if source.$srcrel && !$dstrel.optional {
          // if there's already a relation to some destination entity, 
          // and the relation on the destination side is not optional,
          // we can't break it
          return Err(...);
        }
        if dest.$dstrel && !$srcrel.optional { 
          // same but for the relation from the dest to the source
          return Err(...);
        }
      }

      // upkeep
      if source {
        if HAS_INVERSE && source.$srcrel {
          source.$srcrel.$dstrel = null;
        }
        source.$srcrel = dest;
      }
      if HAS_INVERSE && dest {
        if dest.$dstrel {
          dest.$dstrel.$srcrel = null;
        }
        dest.$dstrel = source;
      }
    }

One-to-many relation



    OneToMany::set(source, $srcrel, dest, $dstrel?) {
        // many "sources" to one "dest"
        // source holds one ref (to_one)
        // dest holds a list    (to_many) 
        // dest relation is optional
        
        assert(source || dest);        

        // null source
        if !source {
          if HAS_DSTREL {
            if $dstrel.optional {
              return Err();
            } 
          } else {
            return;
          }
        }

        // shortcuts
        if HAS_INVERSE {
          if source && source.$srcrel.contains(dest) {
             return;
          }
        }

        // validate the inverse relation
        if !source && 

    }


source: optional one / one / many / none 
dest: optional one /  one / many / none


| src / dst | 0..1                                        | 1 | * | N/A |
|-----------|---------------------------------------------|---|---|-----|
| 0..1      | d0.s = null; s0.d = null; s.d = d; d.s = s; |   |   |     |
| 1         |                                             |   |   |     |
| *         |                                             |   |   |     |
| N/A       |                                             |   |   | N/A |



# Copy/paste

# undo/redo

Issue: ID stability for undo/redo
- generational indices won't work (the generations of the indices stored in the undo commands become out of date)
- need something that guarantees the index of the next inserted element 
- alternatively: never reuse indices
- alternatively: use immutable data structures
  - would require data to be `.clone()`

Ideally: get next ID, build closures, run closure.redo, push closure in undo stack
Unfortunately: can't get ID before inserting the item

- Change(2)
- Remove(2)
- Insert() -> 2
- Insert() -> 3
- Undo (insert), free list .. -> 3 
- Undo (insert), free list .. -> 2
- Undo (remove) // which index? 2 or 3?

Ideal data structure: sparse map from ID->Data, fast enough when IDs are compact, can try to insert with a specific ID
Maybe a BTreeMap?

Should we use sqlite under the hood, and always store data there? And let the user cache the results?
It's not super ergonomic.


# Alternative approach

"Active records":
- load or create an entity (`Track::load`, `Track::create`) -> returns data object
- modify the struct (regular rust methods)
- save it -> data is validated and stored in the DB

Issues: 
- full traversal can be costly, but full traversals are rare
- data objects aren't supposed to live long: create them, modify the data, save them, end cycle.
- possible to have two data objects that represent the same entity but disagree on the value of its attributes
- need to keep loading/reloading data objects
 

# Q: Is there any good example of an in-memory "desktop application database" that handles:
- arbitrary types
- relationships between entities
- undo/redo
- copy/paste
- data consistency on entity removal

While keeping the data mostly in-memory, accessible via direct reference?

Disqualified:
- most ORMs fetch data and make copies, they are not really made for desktop apps

Qualified:
1. Core data

# Our approach

- In-memory first: there's no "fetching" necessary
- Objects represented by indices, possible to get temp references to the actual data 
- Serialization comes after


# UI questions

First approach: UI widget state is stored in the database?

```
GridLayout {
  columns: 2,
  rows: 3,
  
  List {    // Actually nothing more than a VBox flex layout
     
     // content expression
     for track in db.tracks where track.album = "whatever" {
        Item {
          // ...
        }
     }
  }
}

trait Query<T> {
  fn map() -> impl Query<T> {
  }
}


fn AlbumView_query(album: impl Query<Album>) -> impl Query<impl Widget> {
  
    let title = Album::query_title(album);    // Query<Album>
    
    // for track in album.tracks
    // join between album and tracks on relation "tracks"
    
    let track = Album::join_tracks(album);   // impl Query<(Album, Track)>
    
    track.map(|(album, track)| {
        // called on every change to the query above
    })
}

// 
AlbumView(album: Album) {
  CollapsibleHeader {   // #0
    title: album.title
   
    
    for track in album.tracks {
      HBox {         // #1
        Label {
          text: track.title + " on " + track.album.title
        }
        Button {
          label: "remove",
          on_click: |db| {
            // can access album, track; they are IDs so it's easy to move them into the closure
            // they take a `&mut dyn DB` reference
          }
        }
      } 
    }
  }
}


VBox {
  for album in albums {
    AlbumView(album)  /* #0 */
    AnotherAlbumView(album) /* #1 */
  }
}

fn Root_query() -> impl ??? {
    let album = Album::query_all();
    let __0 = AlbumView_query(album);
    let __1 = AnotherAlbumView_query(album);
    
    album.map(|| {
    
    })
    compose! {
       #__0
       #__1
    }
}

// 

// Individual widget
TrackView(track: Track) {
  HBox {
    Text {
      color: if $disabled { X } else { Y }
    }
    Text {
      text: track.name
    }
  }  
}


VBox {
  for album in albums {
  
    // trigger: when album inserted/removed, run closure 
    
    // everything in the `for` loop is tied to a specific `album`, it's a map: album ID -> widget ID
  
    CollapsibleHeader {   // #0
      title: album.title
      
      if !album.is_compilation {
        for track in album.tracks {
        
          // trigger: when track inserted/removed && track.album = album, run closure to insert/remove the corresponding entry
          
          for album in Albums { // #1
            
          }
          
          HBox {         // #2
            Label {
              text: track.title
            }
            Button {
              label: "remove",
              on_click: |db| {
                // can access album, track; they are IDs so it's easy to move them into the closure
                // they take a `&mut dyn DB` reference
              }
            }
          } 
        }
      }
    }
  }
}

// translation into joins:

for album in albums { for track into album.tracks { ... }}
=>
album @ Album(id, ..), // introduces binding `id`, and `album`
track @ Track(album = id, ..) // constraint
album @ Album

#0: album @ Album(..)
#1: album @ Album(album=id, ..) + track @ Track(album=id)     // (incremental on `track`)
#2: album @ Album(album=id, ..) + track @ Track(album=id) + album2 @ Album()    // (incremental on `album2`)


Given an updated entry in the join above, how to get the ID/position of the widget to update/insert/remove?

e.g. for #1

#0 => Widget ID = (album, #0) 
#1 => Widget ID = (album, #0, track, #1) = (parent, track, #1)

Two things:
- widget ID: hash of the key-path to the root of the tree
- position: where to insert the item in the parent widget: usually an entity index + group index


```

## Construction of widgets

There must be two different types: one for specification, another for the retained tree.
The user cannot construct the retained node directly

## Issues:
- events
- local state
- ambient state


```rust


fn View() {
  
  ui.enter::<VBox>(|ui| {
     for album in Albums::changes(db) {
         ui.update((0,album), |ui: &mut CollapsibleHeader| {
            
         });
     }
  });
}

// Query![Album] == filter over the stream of changes

// #1: select Album(album) from Album
// #2: select Track(..) from Track where Track.album = album


// Changes will be in the form:
// Insert/Modify/Remove Entity
//
// E.g.
// Insert,Album,#1
// Remove,Album,#1
// Insert,Track,#0
// 


// Will be called every time an album changes
fn album_view(album: Query![Album] /* #1 */) -> impl Widget {
  Heading {
    title: album.title(), 
    content: VBox { 
        /* #2 */
        content: album.tracks().map(move |track: Query![Track]| {
            // will automatically insert/remove/items when a track is added
            // it can possibly bypass album_view, but not sure that's useful
        })
    }
  }
}

// Widget == a diff over an existing view tree
fn TrackView(track: Track) -> impl Widget {
  //let content = 
  
  Diff::<HBox> {
    insert: |cx| -> HBox {
        let v = HBox::new();
        let color = if cx.state(disabled) {
            //...
        } else {
            //...
        };
        let mut __1 = Text::new();
        __1.color = color;
        v.insert(color);
        // ...
    },
    update: |cx, hbox| {
        
    }
  }
}


fn update_ui(ui: &mut Ui, db: &Db) {
  let revision = ui.last_revision;
      ui.enter::<GridLayout>(move |ui: &mut GridLayout| {
            
        for change in Track::changes_since(revision) {
            match change {
              Insert(track) => {
                // ...
                ui.insert(
                    
                );
              }
              Remove(track) => {
                // ...
              }
              Change(track) => {
                // remove + insert
              }
            }
        }
        
  });
}
```

Issue: fully incremental update (i.e. something changes in the DB, the corresponding list view is updated) would require individually 
addressable UI elements => IDs or pointers.
Otherwise, updating the UI is a recursive traversal (with unchanged branches skipped). 

It makes more sense to make widgets individually addressable. Hence, store them in a slotmap, with stable IDs.

Advantages:
- simplified event delivery
- simplified, more efficient update of parts of the UI

Inconvenients:
- impacts the implementation of container widgets: possibly more verbose than directly owning child elements
- borrowing troubles: if methods on the widget trait take a `&mut Tree`, the widget must be moved out of the tree beforehand
- dynamic downcasting necessary



# DB change streams

```
AlbumChange {
  Insert(ID),
  Remove(ID),
  Modify(ID)
}
```

# UI Query

    VBox {
        for album in Album::all() {
            Selectable {
                Text(album.name)
            }
        }
    }

    // Iterate over all changes


    fn update(db: &mut DB) {
        if let vbox = tree.get_or_insert::<VBox>() {
            
        }
    }


# Conclusions:

- Use the existing kyute framework
- Collections take incremental queries instead
- Each query is associated to a join on the DB

```rust
fn track_view(track: Query<Track>) -> impl Widget {
  // ...
}

// (+,Album(id))
// (+,Track(id))

// The fact that it's a `Query<Album>` means that it's interested in anything that affects the album
// - modifications on an album row
// - relations that target an Album
// - transitively, anything that affects tracks as well
// Problem: because of relations, we can end up depending on the whole data model instead of just the parts that matter to the view
//
// Problem: it should also be called for everything that affects `Track` as well
fn album_view(album: Query<Album> /* #0 */) -> impl Widget {
  List {
    // The `content` is updated completely independently of `album_view`, it can be called without `album_view` being called higher in the call stack
    // Issue: when album_view is called again, there's no way to ensure that the query is the same (save for somehow "comparing" the queries),
    // and thus we must assume that the content is completely different
    // -> the content must be defined outside of the view, it cannot depend on the album
    
    content: album.tracks().map(|track| {
        track_view(track)
    })
  }
}




// First pass: album_view is called, returns the List widget
// Second pass: some method is called on the `Widget`, gets the DB changes
//      - list content is evaluated

// Conceptually, `impl Widget` is a `template` to update the tree from DB changes; it's not meant to change
// meaning that there cannot be control flow in the function (it might as well be data)
```

# Proposal: pointers to objects instead of IDs?

No, instead, when querying, also return a pointer to the data; put the ID inside to minimize type complexity for joins.
Reverse the entity trait to be on the row type instead


# Three options for storing relations

A. Separate tables for each attribute
-> No; too much overhead

B. Non-relation attributes in the same table, relations in separate tables (a.k.a. junction tables)
-> additional lookups necessary on joins
-> advantage: can delta on items added to relations

C. All relations stored inline, except many-to-many relations
-> many different code paths

D. All relations stored inline, including many-to-many relations


Idea: joins operate on "complete" rows, which are composed of the entity ID + a reference to the content


# Evolution

```
    pub store TrackDb;

    Album(
        name: String,
        rel album_artist: Artist,
        year: u32,
    );

    Track(
        name: String,
        rel artist: Artist,
        rel album: Album
    );

    Playlist(
        name: String,
    );  

    Artist(
        name: String,
    );
    
    // External relation
    rel PlaylistTracks(Playlist, Track);  // equivalent of putting the relation in `Playlist`
```

Issue: `rel` are stored inline, but `rel*` use junction tables. The row types are different even though they "look" the same in the schema.

`rel` introduces a foreign-key relation.
An index is created (`Index_Album_Track_album<(Album, Track) -> ()>`). This index is updated when a track is added.

`rel*` introduces a to-many foreign-key relation.
This creates one relation table and an index table:
`Rel_Playlist_Track_tracks<(Playlist,Track) -> ()>`: tracks in each playlist (authoritative)
`Index_Playlist_Track_tracks<(Track,Playlist) -> ()>`: playlists that each track is in

Specifying the inverse relation?




## Iteration

## Joins

```
// Returns tuples (&'a Album, &'a Track) joined on the `Track.album` relation
join!( Album, Track by Track.album )

// query over album -> &'a Album
// join with relation

```


Joins are like pattern matching:

`album @ Album { id, name, .. }, Track { album: id, .. }`

Join on `album.id` (primary key) and `track.album` (foreign key).
Delta join: union of `Album x Delta(Track) + Delta(Album) x Track`

Duplicates?
E.g. adding album and adding a track in that album:



### Types of joins

* Joins on a one-to-one or one-to-many relations
* Joins on foreign-key relations have special optimizations for incremental evaluation



```
// Iteration
for album @ Album { id, name, .. } in albums { 
  for Track { .. } in index!(Track, album).iter(store, id) { // index query; doesn't matter that there's a relation
     // ... 
  } 
}

// Delta query

// DV = Lnew*DR - Lprev*dR + DL*Rnew - dL*Rprev

for delta in albums.delta() {   
  match delta {
    Added(..) => {      // D+R
    }
    Removed(..) => {    // D-R
      
    }
    Updated(..) => {    // D-R, D+R
    }
  }
}

```


## Indices

    // Index for foreign keys:
    Map<(FK,S)>
    // Index for foreign keys:
    Map<FK -> S>

```rust
trait RelIndex<K,V> {
  fn check(&self, k: K, v: V) -> bool;
}

```
  

# Datalog-like

No "foreign keys", instead relations are in separate tables
Makes the logic easier to implement, but can be costly (one extra lookup for joins).

Q: Why is it cleaner?
A: Because it removes an indirection level: before, we had entities, and inside entities, relations
   The goal is to minimize the number of concepts and put them on the same level

joins: album @ Album { id: album_id, name, .. } & track @ Track { id: track_id, name: track_name } & TrackAlbum(track_id, album_id)
Is there a way to make the integrity constraints more "explicit", and more "principled"?

Since the relations are immutable, perform the change, then check the constraints
If the constraints are not satisfied, revert to the previous state
    
Alternative to on delete rules: have the user handle the deletion of related entities
(+) more flexible: no need for predefined behaviors (cascade, nullify, delete) -> users update the table themselves
(+) more explicit
(+) integrity is still checked after insertion, if there's a problem, revert the changes
(-) more error-prone: user may forget to write the deletion cascade code
(-) less extensible: another inheriting store would need to know when a referenced entity is deleted, like a "trigger"


Issue: datalog by itself has nothing to do with "integrity constraints" or "delete rules"

Idea: delete rules are just a special case of "triggers"
E.g. when a table has a foreign-key relation, register a "trigger" on the foreign table that deletes / nullifies the FK.

A trigger would look something like this:

```rust
trait Trigger {
    type Store;
    type Entity;
    //fn before_insert(&self, store: )
    fn before_delete(&self, store: &Self::Store, deleting: Self::Entity::Id) -> Result<(), Error>;
    fn after_delete(&self, store: &mut Self::Store, deleted: Self::Entity::Id) -> Result<(), Error>;
}


impl Trigger for FK_Track_Album_DeleteCascade {
    fn on_delete(&self, store: &mut Store, deleted: AlbumId) {
        let album_tracks = fk_index!(store:Track.album).get(deleted).collect::<Vec<_>>();
        for t in album_tracks {
            store.remove(t);
        }
    }
}

```

It's different from datalog rules because they can also update user-modifiable tables (the extensional database).
Issue: it's not declarative anymore, and algorithms can't rely on the fact that referential integrity is preserved. 

Tentative design:
- entities are stored in B+Trees, with the primary key as the key
- each foreign-key ref to an entity has an associated index
- unique constraints are implemented as indices
- triggers are run before/after insertion/deletion to ensure integrity

See also:
- inclusion dependencies

## Cost of OrdMaps?

- Memory-heavy (chunks are duplicated on every update)
- Can't experiment with alternative data structures (vecs, hashmaps, etc.)
- diffs are not exactly free

Alternative:
- the history: a big sequence of modifications (insertions, retractions)
- can play forwards, backwards

Issue: sometimes there can be lots of intermediate states that we don't want to keep in the log
(e.g. the position of a control point being dragged => we only care about the states before and after the gesture)

Q: in the history, should we track modifications to individual attributes, or only whole rows?

## Modeling individual attributes

Main use case: string & array-valued attribute pooling. 
Since we want undo, might as well allocate strings and arrays in an append-only pool.

Issue: currently, when declaring an entity in the schema, it is expected that the data is stored and accessed via structs
that are declared exactly the same as the entity.

Q: Maybe this shouldn't be the case? 
A: In most cases, required attributes should be stored next to each other in the entity record. 

It makes no sense to split the attributes among different tables if all of them are required:
* it increases memory overhead
* it increases the number of lookups
* it makes deleting entities harder because we need to remove all related attributes

## Tentative
- Model data relational-style: entities with required attributes; all attributes of an entity are stored in row structs
  - arrays and strings may be allocated in special pools?
- Changes are tracked per-attribute
- Use mutable BTreeMaps for the indices: it's important that they support range queries 
- Changes to the same attribute and the same entity within the same revision are coalesced

## Diffs
Q: what kind of diffs do we want? 
A: Diffs of the form:

```rust
// Holds value
enum Change<K,V> {
  Insert(K,V),
  Remove(K,V)
}

// Holds pointer to immutable data
enum Change<K,V> {
  Insert(K, *const V),
  Remove(K, *const V),
}
```

Since data is immutable, can store rows in pools with stable addresses. 
In theory, can replace foreign keys with pointers to data (saves a lookup).
However, iteration on the main index is slower since the data is a pointer away from the index structure instead of directly within.

In databases: clustered VS non-clustered indices.

    Clustered indexes sort and store the data rows in the table or view based on their key values.

    Nonclustered indexes have a structure separate from the data rows. 
    A nonclustered index contains the nonclustered index key values and each key value entry has a pointer to the data row that contains the key value.
    The pointer from an index row in a nonclustered index to a data row is called a row locator. 
    The structure of the row locator depends on whether the data pages are stored in a heap or a clustered table. For a heap, a row locator is a pointer to the row. For a clustered table, the row locator is the clustered index key.


Basically, sort the data physically in the order in which you're going to iterate over it.
E.g. for tracks, sort by album.

Table is TrackId -> Track
But is stored in the DB as (Album, TrackId) -> Track

Q: What should the entity type be? Cloneable?
A: maybe not directly the type stored in the database: we may want to store raw pointers
also, it shouldn't have setters (impossible since it may hold borrows)

So, in terms of types, we have:
- the user-facing "entity" type (e.g. Album)
- the primary key type (e.g. AlbumId)
- the internal row type (e.g. AlbumRow)
- the initializer type used when inserting rows

## UI trees from deltas

    for album in albums {

        // (dependency=album)

        AlbumItem(album.title) (QA)   // album -> string (album_title)

        Container {
            for track in album.tracks(db) {
                // dependency=album,track

                with Artist { id=track.artist, name }         
                    TrackItem(track.title, track.artist, album.artist) (QB)       
              
                // Album  { id=album_id, album_artist=album_artist_id, .. }, 
                // Track  { album=album_id, artist=artist_id, title, .. }, 
                // Artist { id=album_artist_id, name: album_artist_name, .. },
                // Artist { id=artist_id, name: artist_name, .. }
                
                Text(album.title + track.title) (QC)
                
                // ...
                // every item here is associated to two IDs: album and track
    
                // album remove -> delete (album, ..)
                // (track,album) removed -> delete (album, track)
    
                // album added
            }
        }
    }


QB:

Album.Artist { id=album_id, album_artist=album_artist_id },
Album.Title  { id=album_id, album_title=album_title },
Track.Album  { id=track_id, album=album_id },
Track.Artist { id=track_id, artist=artist_id },
Track.Title  { id=track_id, title=title }
Artist.Name  { id=album_artist_id, name=album_artist_name },
Artist.Name  { id=artist_id, name=track_artist_name },

Variables:
- album_id (pk)
- track_id (pk)
- album_artist_id (pk)
- artist_id (pk)
- album_artist_name
- track_artist_name
- title
- album_title

Key structure:
(album_id, track_id, album_artist_id, artist_id)

Dependencies:
- Album.Artist
- Album.Title
- Track.Album
- Track.Artist
- Track.Title
- Artist.Name

Somehow, when, say, Artist.Name changes, must be able to reconstruct all pks (album_id, track_id, album_artist_id, artist_id)

Artist.id
-> Track.Artist { id=track_id, artist=artist_id },      // (via index)
-> A
    
    for track_id in db.fk_Track_artist.keys(artist_id) {
      if let album_id = db.pk_Track[track_id].album {
        if let album_artist_id = db.pk_Album[album_id].album_artist {
          // we have every key

        }
      }
    }
    


QA == album @ Album(..) // watch for changes in albums

QB == album @ Album(id,..), track @ Track(album=id,..) 
// Item depends on album, track
// album removed -> remove
// track removed -> remove

// watch for changes in tracks, return (album, track) where album = track.album
// => don't care about changes in albums

QC == album @ Album(id,..), track @ Track(album=id,..)
// needs to be updated when album title changes

example changelogs:
removing an album:
- remove album 0
- remove track 1
- remove track 2
- remove track 3

-> remove AlbumItem(0)
    -> will also remove TrackItem(0,1..3)
-> remove TrackItem(0, 1..3) => no-ops

renaming an album:
- remove album
- remove track 1
- remove track 2
- remove track 3
- add album
- add track 1
- add track 2
- add track 3 
(FIXME: track.album index is broken)

-> remove AlbumItem(0)
  -> will also remove TrackItem(0,1..3)
-> remove TrackItem(0, 1..3) => no-ops
-> add AlbumItem(0)
-> add TrackItem(0,1..3)

Problem: state of TrackItems are lost

-------------------------

renaming an album:
- remove album_title(0)
- insert album_title(0)

tracks are unaffected
=> modifications must be tracked per-attribute


## Rule: we must track changes per-attribute

Otherwise, if we only track changes at the entity level, we must assume that every attribute may have changed,
including foreign keys, which introduces false dependencies.

e.g. assume we have a UI of tracks, grouped by album. The group widget depends only on the name of the album.
The tracks to display within a group are determined by a join (Album x Track) on Track.album; if the only title of the album changes,
but we consider that the whole album might have changed (i.e. removed and added), then we must recalculate the join.
(there are other examples, but not written down yet)

-------------------------

- delete rules
- foreign key unique constraint
- clustered indices
- nullable foreign keys
- foreign key update
- redundant index when an FK is used in a clustered index


Issue: 
Insert/remove log items for individual attributes make no sense: we can't remove an attribute from an entity 


We must "normalize" the change log somehow to make delta queries more tractable.

Example of tricky situation:

- Remove track(1).name 
- Remove track(1).album 
- Remove track(2).name 
- Remove track(2).album
- Remove track(3).name
- Remove track(3).album
- Remove album(id).name
- Remove album(id).artist

Query: track_name(id), track_album(id,album), album_name(album,name), album_artist(album,artist)

- track(1).name removed
-> compute the resulting delta on the join
-> problem: track_album(1,album) is not there anymore

- track(1).album removed
-> we have id, album_id
-> can't get album_name
-> can't get artist because album was removed

Solution: delete ranges
Each item in a UI query has an associated key in the form `(a,b,c,d....)` composed of the PKs of each entity in the join.
E.g. `(album,track,artist,album_artist)`. The query results are ordered by this key.

if `track_album(1,album)` is not there, then album has been deleted, thus there will be a remove entry
for `album`. In which case we can delete `(album,*,*,*)`
if `track_album(1,album)` is still there, we can delete `(album,track,*,*)`.

Basically, when an entity that appears in the composite key is deleted, try to reconstruct the keys that appear before in the composite:
e.g. `track -> album`, `artist -> track -> album`, `artist -> album`
If that's impossible, give up: this means that there's another "remove" entry for a "parent" entity



1. Insert/Remove only whole tuples
2. Split attributes in different tables by update frequency
3. eliminate redundant removal and insertions


## Diffs across many checkpoints?

## Reality check: is it going to be more efficient than tree diffs?
Not sure, and it will be hard to measure: would need two frameworks to compare.

Plus there's a lot of things that remain unimplemented:
- delta queries
- UI macros


## Reusable components

    fn track(id: TrackId) {
        ui! {
            select Track {id, name, album}, Album {id: album, title: album_title} {
                // QB
                // (dep={Track,Album})
                // (address=(parent, id, album, #0))
                // (add watch: Track.id, Album.id)
                // ...
                
            }
        }
    }

    impl Query for TrackQ {
        type Key = (i32, TrackId, AlbumId);
    }


    fn album(album_id: AlbumId) {
        ui! {
            // dep=???
            select Track {id, album=album_id} {
                // #QA
                // (dep={Track})
                // (add watch: Track.id)
                // (address=(parent, id, #0))
                // __album_0
                track(id)           // FIXME: this depends on Album as well, but nothing says so
            }
        }
    }

    fn root() {
        select Album{id} {
            // #QR
            // dep=Album, key = (#0, Album.id, #0)

            // __root_0
            album(id)
        }
    }

(#QR, album=0) => `album(id)`
(#QR, album=0, #QA, track=0) => `track(id)`
(#QR, album=0, #QA, track=1) => `track(id)`
...
(#QR, album=0, #QA, track=9) => `track(id)`

(#QR, album=0, #QA, track=9, #QB, album=1) => `track(id)`

```rust
#[derive(PartialEq, Hash)]
enum RootKey {
    __root_0(AlbumId, <album as Query>::Key)
}


#[derive(PartialEq, Hash)]
enum album_Key {
    __album_0(TrackId, AlbumId, <track as Query>::Key)
}

impl QueryKey for album_Key {
    fn affected(&self, db: &TrackDB, change: ChangeKind) {
        match change {
            Track_Inserted(v) | Track_Removed(v) if v == self.0 => {
                true
            }
            // ...
            _ => {
                self.2.affected(db, change);
            }
        }
    }
}

// on every change, iterate over the whole list of nodes to see which are affected => not super efficient

#[derive(PartialEq, Hash)]
enum track_Key {
    __track_0(TrackId, AlbumId)
}

impl QueryKey for track_Key {
    //const DEPENDS: &[Attribute] = &[Track::A_ID, Album::A_ID, Track::A_NAME, Track::A_ALBUM, Album::A_TITLE];
    
    fn affected(&self, db: &TrackDB, change: ChangeKind) {
        match change {
            Track_Inserted(v) | Track_Removed(v) if v == self.0 => {
                true
            } 
            Album_Inserted(v) | Album_Removed(v) if v == self.1 => {
                true
            }
            Album_Name_Inserted(v,_) | Album_Name_Removed(v, _) if v == self.1 => {
                true
            }
            _ => false
        }
    }
}

// Given a key, need to know if the element associated to the key is affected by the change (i.e. must be deleted and recreated)
impl RootKey {
    
}

fn test() {
    let k = RootKey::__root_0(0, album_Key::__album_0(0, 0, track_Key::__track_0(0, 0)));
    let k = RootKey::__root_0(0, album_Key::__album_0(1, 0, track_Key::__track_0(1, 0)));
    let k = RootKey::__root_0(0, album_Key::__album_0(2, 0, track_Key::__track_0(2, 0)));
    let k = RootKey::__root_0(0, album_Key::__album_0(3, 0, track_Key::__track_0(3, 0)));
    let k = RootKey::__root_0(0, album_Key::__album_0(4, 0, track_Key::__track_0(4, 0)));
    let k = RootKey::__root_0(0, album_Key::__album_0(5, 0, track_Key::__track_0(5, 0)));
    
    let nodes : HashMap<RootKey, Node> = Default::default();
}




```

        VBox {
            // select Album { id: album_id, name: album_name }
            (album_id=0, #0, track_id=default) Header { "Album Name" }         // 
            (album_id=0, #1, track_id=default) Separator

            // select Track { id: track_id, album, name: track_name } where album == album_id
            (album_id=0, #2, track_id=0) Group { Header("Track 1") }
            (album_id=0, #2, track_id=0) Group { Header("Track 2") }
            (album_id=0, #2, track_id=0) Group { Header("Track 3") }
            (album_id=0, #2, track_id=0) Group { Header("Track 4") }
            (album_id=0, #2, track_id=0) Group { Header("Track 5") }

            // ... other albums ...
        }



```rust
impl QAlbum {

    fn update(&mut self, db: &DB, change: &DB::Change, parent: ()) {
        match change {
            ch!(inserted: Album(id)) => {
                for query!(Album {id: album_id, name: album_name} where album_id == id) in db.query() {
                    // run the query
                }
            }
            ch!(removed: Album(id)) => {
                list.remove_range((id, .., ..));
            }            
        }
        
        // update subqueries
        // this loops over all albums
        for query!(Album {id: album_id, name: album_name} ) in db.query() {
            self.subquery.update(db, change, album_id);
        }
        
    }
    
}

impl QTrack {
    
    fn update(&mut self, db: &DB, change: &DB::Change, (album_id, album_name):() ) {
        // where is album_id and album_name?
        //
    }
}

```


Album/tracks:
- syrufit: over
  - Maurits"禅"Cornelis - Voice of Mist
  - 陽花 - Silent Story
   - 衝動的の人 - VAGRANT (MZC Falling Into Massive Galaxy Remix)
   - Vivienne - With Me (MZC Paradigms To The Next Perspective Remix)
   - Cocoon - History of the Moon (MZC Rise Of The Phenomenal Core Remix)
   - Chen-U - Rendezvous

When a track is added: 
- re-run QA

When the title of an album is changed:
- re-run QB with album=(the id of the changed album)
  - remove (track_id, album_id) where album_id = change.album_id
  - insert

When an album is removed:
- remove nodes that depend on Album.id
  - QR: 

Track.id:
  - QA: 

Alternatives to full queries: run the procedure to build the tree, and it watches changes to the rows that it depends on. 


Problem: in the macro, we know nothing about the components. 
Basically, since we're **not a compiler**, we can't build and optimize queries across component boundaries.
Solutions: 
1. a separate compiler...
2. build queries at runtime
3. don't provide reusable components inside rust modules (a separate module system?)


(2.) Building queries at runtime: meh, the philosophy was to generate straightforward rust code, but now we have to go through
a runtime "interpreter" for queries, that are specified at runtime. They _could_ in theory be generated at compile-time,
but we can't because we don't know enough about referenced queries during macro expansion.
=> kind of a cop-out

(3.) This means that we can't distribute UI/model components in crates. That's unacceptable (might as well design the whole language from scratch). 

Meta note: it seems that we always end up with the same limitations of doing things with macros over an existing language:
- graal/mlr for GPU pipelines: having a true compiler, we would be able to reason about how resources are used in the program, so that the user doesn't have to worry about usage flags, etc.
- kyute: with a compiler we'd be able to better optimize `#[composable]` function calls with compile-time constant arguments.
- and now this
It's always suboptimal somehow (not enough information about the program is available).
 

Big issue: it's easy to do stuff in a "self-contained" way, but it's **hard** to make that composable when it's embedded in another language (i.e. rust).
E.g. you have query Q1 in a crate, Q2 in another, with optimized code generated for both queries in their respective crates. 
How do you generate optimized code for a query Q3 that is a composition of the two? At this point, we lost the information
that was used to generate the code Q1 and Q2, and that could be used to generate an optimized version of Q3.
(the "information" here is the code passed to the macro used to define the queries)
=> we must generate code for Q3 without knowing *anything* about the other queries (at macro expansion time)
(there might be alternate approaches based on the type system, but they may rapidly devolve into inscrutable and unmaintainable type-level code)



## Roadblock: hierarchies?

Recursive queries seem hard, e.g.

    fn node(id: NodeId) {
        ui! {
            Indent {
                VBox {
                    select Node{id: child_id, parent: id} {     // QA
                        node(child_id)
                    }
                }
            }
        }
    }

First, it's not clear how to "invoke" or reuse queries.
In this case, it's possible to run QA with parent=root, but if no nodes are added nothing will ever update

From [A UI library for a relational language](https://www.scattered-thoughts.net/writing/relational-ui/#implementation)
> Currently templates are limited to a fixed depth, so they can't express eg a file browser where the depth depends on the data. 
> Allowing components to include themselves recursively would fix this, but it's non-obvious how to combine recursion with the query-based implementation I described earlier. 
> It's probably not impossible, but I won't attempt to deal with it until I definitely need it.
 

## UI tree

    fn album_view(album: Album) -> impl Widget {
        // If it takes an Album, then the function must be called again every time the album changes; 
        // It must return either a diff for the tree, or the new widget.
        // A complication is that parts of the returned view may change, regardless of whether the album changes 
        // (e.g. a track may change, but the album row itself doesn't)
        // which means that `album_view` must be invoked again if **anything inside** (i.e. any track) changes,
        // since we can't call `track_view` independently of `album_view`
    }

    // Another approach: indirection
    fn album_view(album: impl Query<Album>) -> impl Query<Widget> {
        // effectively, this is called only once to set-up the incremental program
        // Q: what about conditionals? 
        // A: they would have to be encoded in the program

        // Q: widget local state?
        // A: 
    }

    

    fn root() -> impl Widget {
        VBox::new()
            .children(
                // query
            )
    }


## Subscriptions?

Subscribe to data changes. Child lists are "reactive lists".


# Reality check

All of this might be too complicated to implement in rust with macros (even proc-macros).
Setting complexity aside, it's also complicated for users:
- macro-heavy, so poor autocompletion
- special syntax to learn in order to define the data model
- no control over the data structures
- mismatch between relational model and object graphs
- control-flow, complexity is not obvious (abstracted away)
   (should still be able to reason about perf though)
- (?) hard to adapt/tweak to specific use cases (maybe?)

Some concepts are worth keeping:
- integrity rules
- automatic change log for undo/redo

However, incremental UI trees are complicated:
- two-way incremental joins are doable
- but composing multiple components with joins is not easy

# Alternative

Inspired by https://docs.rs/x-bow/0.2.0/x_bow/

- Create your own structure for the application data: either relational or object graph, as required.
- Automatically derive "lenses" to identify every part of the state (fields, array elements, map elements, ...)
- Subscribe to changes on the state via lenses
- Change state via lenses

* Undo/redo can be implemented generically (since every change has to go through our lens methods).
  * undo log entries are `(lens, value)` tuples
* Emulate integrity rules by reacting to changes

It's very much like veda's addresses.

Q: how do we update the UI? Do callbacks hold pointers to parts of the UI tree?
A: 


Q: unsubscribing to changes?
A: subscriptions are organized in trees; when a data item is removed, all associated subscriptions and those below are deleted.

# About JS gui frameworks

https://xi.zulipchat.com/#narrow/stream/147932-chatter/topic/the.20taxonomy.20of.20GUI.20paradigms/near/233018647

> But there are flaws in relying too heavily on the JS world. One is that you basically have to assume DOM as an axiom. 
> None of this helps answer these questions: Is DOM a good idea? If you were building an intermediate representation for GUI from scratch, how would you improve on DOM? 
> What things currently handled by DOM should be moved into the higher level (CSS being too heavy-handed is a good candidate here)? Lower level?
> And there's also, I think, an insidious effect of DOM on academic writing to sweep things under the rug that are handled by DOM. 
> How do you express things like text entry state? Tab focus? Accessibility? In the DOM world, the answer to all these questions is basically, "the browser handles it." 
> And so you see a pervasive underemphasis of persistent object identity in the academic literature on UI. I'm especially calling out the functional camp on this.
 

# Rewriting the UI tree -- structure of the UI tree

What kind of structure do we want for the UI tree?
Options:

### (OWNED) containers own their children / no generic traversal
aka container-owns in druid
(+) Minimal boilerplate, intuitive, less noisy
(+) Very intuitive to build for the user (it's a regular composition of objects)
(-) no traversal without cooperation (additional code) of the widget -> widgets are responsible for propagating events / visitors to their children
(-) can't refer to a particular widget in the tree (which is important for several features): need widget IDs for that, and a separate hierarchy structure for widget IDs that must be in sync: error-prone
Note: druid had bloom filters on widget IDs to "accelerate" delivery of messages to widgets with a specific ID, but they probably accelerate almost nothing in practice (abandoned for xilem)
xilem has a separate hierarchy that must be kept in sync


### (DOM) type-erased DOM-like structure

(-) Might be more noisy
(-) Widgets cannot constrain the type of their children, or wrap children in container-specific wrappers (e.g. `GridItem<T>`)
(-) Might be **much less intuitive** for the user to build trees of widgets with this kind of structure (it's more complicated than just composing objects together)
    -> it hurts especially for widget wrappers
(+) Traversal does not need cooperation from the widget impl
(+) Can refer and reach individual widgets deep inside the tree without a separate hierarchy


Proposal: owned tree,  


# Questioning the relational model

Do we really need/want a flat relational model for the data? It makes sense for some applications (e.g. music database)
but for simpler applications it can get in the way, compared to a traditional data model based on object trees.

The model used in DBs come with interesting features like integrity checks, but it's not particularly complicated to enforce
with object trees either; i.e. data consistency isn't the main motivator for a state management system.

I'd argue that the main motivator and problem to solve is the question of reactivity, specifically related to UI updates.
This entails: 
- how to identify parts of the data,
- track when/how parts of the data change 
- how to specify that a part of the UI depends on some data 
  - equivalently, how to _associate_ the identity of a UI element with some data in the data model
- how to update the UI efficiently (that is, incrementally) when dependent data changes


One advantage of the relational model is that it's flat, so it's easy to identify bits of data by using a tuple 
(table, attribute, primary key).
With object trees, you need to store the "path" to the bit of state in the tree, which can be arbitrarily long.

Another thing with object trees is that UI elements will be associated (i.e. identified with) nodes of this tree. 
However, this complicates UI elements that depend on two nodes in that are "far away":
UI element end up depending on the "nearest common ancestor" of the two nodes, which can cause the element to 
invalidate more frequently than needed.

The relational data model may work better in this case: with joins we can precisely tell which tables+attributes 
the element depends on (the dependency graph is more or less arbitrary).