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
