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
        for track in album.tracks(db) {
            // ...
            // every item here is associated to two IDs: album and track

            // album remove -> delete (album, ..)
            // (track,album) removed -> delete (album, track)

            // album added
        }
    }