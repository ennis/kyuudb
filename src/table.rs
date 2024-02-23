use slab::Slab;
use slotmap::{new_key_type, SlotMap};
use vec_map::VecMap;
use crate::db_index::TableIndex;
use crate::Index;


pub enum TableEvent {
    Inserted(Index),
    Removed(Index),
}


/*
// One-to-one: VecMap<u32>, or store inline as an option
// One-to-many: VecMap<Vec<u32>>, or store inline as a Vec<u32>

pub struct OneToManyRelation {
    rel: VecMap<Vec<u32>>,
}

impl OneToManyRelation {
    pub fn new() -> OneToManyRelation {
        OneToManyRelation {
            rel: VecMap::new(),
        }
    }
}

/// Represents a one-to-many relation between two entities: one A to many Bs.
impl OneToManyRelation {
    pub fn insert(&mut self, db: &dyn Database, a: Index, b: Index) {
        let a = a.to_usize();
        let b = b.to_usize();
        if a >= self.rel.len() {
            self.rel.resize(a+1, Vec::new());
        }
        self.rel[a].push(b as u32);
    }

    /// Removes the specified A from the relation
    pub fn remove(&mut self, db: &dyn Database, a: Index) {
        // what do we do with the B's?
        db.remove_one_to_many
    }

    /// Breaks the relation
    pub fn remove_all(&self, db: &dyn Database) {
        // what do we do with the B's?
    }
}

/// Represents a table.
pub struct Table<V, DB: ?Sized> {
    /// Index of this table in the database that holds it.
    index: TableIndex,
    /// Holds entity data.
    data: Slab<V>,
}



impl<V, DB:?Sized> Table<V, DB> {
    pub fn new(index: TableIndex) -> Table<V, DB> {
        Table {
            index,
            data: Slab::new(),
            event_handlers: Vec::new(),
        }
    }
}

/// Trait implemented by databases that have a store of the specified type.
pub trait HasStore<Store> {
    fn store(&self) -> &Store;
    fn store_mut(&mut self) -> &mut Store;
}
*/