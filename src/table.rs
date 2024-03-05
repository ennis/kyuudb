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

type Map<T: Entity> = OrdMap<u32, Row<T::Row>>;

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

    pub fn insert(&mut self, data: T::Row) -> T {
        let id = self.next_id;
        self.next_id += 1;
        self.data.insert(id, Row { data, revision: 0 });
        T::from_u32(id)
    }

    pub fn remove(&mut self, id: T) -> Option<T::Row> {
        self.data.remove(&id.to_u32()).map(|row| row.data)
    }

    pub fn get(&self, id: T) -> Option<&T::Row> {
        self.data.get(&id.to_u32()).map(|row| &row.data)
    }

    pub fn get_mut(&mut self, id: T) -> Option<&mut T::Row> {
        if let Some(row) = self.data.get_mut(&id.to_u32()) {
            row.revision += 1;
            Some(&mut row.data)
        } else {
            None
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (T, &T::Row)> {
        self.data
            .iter()
            .map(|(id, data)| (T::from_u32(*id), &data.data))
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

    pub fn contains(&self, id: T) -> bool {
        self.data.contains_key(&id.to_u32())
    }

    pub fn keys(&self) -> impl Iterator<Item = T> + '_ {
        self.data.keys().map(|id| T::from_u32(*id))
    }

    pub fn values(&self) -> impl Iterator<Item = &T::Row> {
        self.data.values().map(|row| &row.data)
    }

    pub fn next_id(&self) -> u32 {
        self.next_id
    }

    pub fn delta<'a>(
        &'a self,
        prev: &'a Table<T>,
    ) -> impl Iterator<Item = Delta<(T, &'a T::Row)>> + 'a {
        prev.data.diff(&self.data).map(|item| match item {
            DiffItem::Add(k, v) => Delta::Insert((T::from_u32(*k), &v.data)),
            DiffItem::Update { old, new } => Delta::Update {
                old: (T::from_u32(*old.0), &old.1.data),
                new: (T::from_u32(*new.0), &new.1.data),
            },
            DiffItem::Remove(k, v) => Delta::Remove((T::from_u32(*k), &v.data)),
        })
    }
}

impl<T: Entity> Index<T> for Table<T> {
    type Output = T::Row;
    fn index(&self, id: T) -> &Self::Output {
        &self.data[&id.to_u32()].data
    }
}

impl<T: Entity> IndexMut<T> for Table<T> {
    fn index_mut(&mut self, id: T) -> &mut Self::Output {
        self.get_mut(id).unwrap()
    }
}

impl<T: Entity> Default for Table<T> {
    fn default() -> Self {
        Self::new()
    }
}
