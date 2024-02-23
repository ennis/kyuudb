# Goals
* consistency when adding and removing entities (deletion cascade)
* zero-code undo/redo
* serialization and persistence / copy-paste
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


