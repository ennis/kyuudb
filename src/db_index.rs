
/// Identifies a table in the database.
///
/// Internally it's just a newtype for a u32 index.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[repr(transparent)]
pub struct TableIndex(u32);

/// Identifies a value in a table.
///
/// Internally it's just a newtype for a u32 index.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[repr(transparent)]
pub struct Index(pub(crate) u32);

impl Index {
    pub const fn from_u32(x: u32) -> Self {
        Index(x)
    }
    pub const fn as_u32(self) -> u32 {
        self.0
    }

    pub const fn to_usize(&self) -> usize {
        self.0 as usize
    }

    pub const fn from_usize(index: usize) -> Self {
        Index(index as u32)
    }
}

/// A global database index that identifies a value in a database, across all tables.
///
/// It's a combination of a `TableIndex` that identifies the table, and a `u32` index that identifies the value within the table.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct DbIndex {
    /// Identifies the table.
    pub table: TableIndex,
    /// Identifies the value within the table.
    pub value: Index,
}

impl DbIndex {
    pub const fn new(table: TableIndex, value: Index) -> DbIndex {
        DbIndex { table, value }
    }
}
