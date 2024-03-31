use crate::db::EntityId;
use crate::Entity;
use im::ordmap::{DiffItem, OrdMap};
use std::ops::{Index, IndexMut};

#[derive(Clone)]
struct Row<T> {
    data: T,
    revision: u32,
}

impl<T> PartialEq for Row<T> {
    fn eq(&self, other: &Self) -> bool {
        self.revision == other.revision
    }
}

type Map<T: Entity> = OrdMap<u32, Row<T>>;

#[derive(Clone, Debug)]
pub enum Delta<V> {
    Insert(V),
    Remove(V),
    Update { old: V, new: V },
}

/// Stores entity data.
#[derive(Clone)]
pub struct Table<T: Entity> {
    pub(crate) data: Map<T>,
    next_id: u32,
}

impl<T: Entity> Table<T> {
    pub fn new() -> Table<T> {
        Table {
            data: OrdMap::new(),
            next_id: 0,
        }
    }

    pub fn insert_at(&mut self, data: T) -> T::Id {
        assert_eq!(data.id(), self.next_id());
        let id = data.id();
        self.next_id += 1;
        self.data.insert(id.to_u32(), Row { data, revision: 0 });
        id
    }

    pub fn remove(&mut self, id: T::Id) -> Option<T> {
        self.data.remove(&id.to_u32()).map(|row| row.data)
    }

    pub fn get(&self, id: T::Id) -> Option<&T> {
        self.data.get(&id.to_u32()).map(|row| &row.data)
    }

    pub fn get_mut(&mut self, id: T::Id) -> Option<&mut T> {
        if let Some(row) = self.data.get_mut(&id.to_u32()) {
            row.revision += 1;
            Some(&mut row.data)
        } else {
            None
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.data.iter().map(|(id, data)| &data.data)
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn clear(&mut self) {
        self.data.clear();
    }

    pub fn contains(&self, id: T::Id) -> bool {
        self.data.contains_key(&id.to_u32())
    }

    pub fn keys(&self) -> impl Iterator<Item = T::Id> + '_ {
        self.data.keys().map(|id| T::Id::from_u32(*id))
    }

    pub fn values(&self) -> impl Iterator<Item = &T> {
        self.data.values().map(|row| &row.data)
    }

    pub fn next_id(&self) -> T::Id {
        T::Id::from_u32(self.next_id)
    }

    pub fn delta<'a>(&'a self, prev: &'a Table<T>) -> impl Iterator<Item = Delta<&'a T>> + 'a {
        prev.data.diff(&self.data).map(|item| match item {
            DiffItem::Add(k, v) => Delta::Insert(&v.data),
            DiffItem::Update { old, new } => Delta::Update {
                old: &old.1.data,
                new: &new.1.data,
            },
            DiffItem::Remove(k, v) => Delta::Remove(&v.data),
        })
    }
}

impl<T: Entity> Index<T::Id> for Table<T> {
    type Output = T;
    fn index(&self, id: T::Id) -> &Self::Output {
        &self.data[&id.to_u32()].data
    }
}

impl<T: Entity> IndexMut<T::Id> for Table<T> {
    fn index_mut(&mut self, id: T::Id) -> &mut Self::Output {
        self.get_mut(id).unwrap()
    }
}

impl<T: Entity> Default for Table<T> {
    fn default() -> Self {
        Self::new()
    }
}
