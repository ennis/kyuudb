#![feature(macro_metavar_expr)]
pub mod db;
mod db_index;
mod error;
mod index_vec;
mod table;
mod circuit;

pub use db::{ Database, Entity, HasStore, EntityId};
pub use db_index::{DbIndex, Index};
pub use error::Error;
pub use table::{Delta, Table};

#[doc(hidden)]
pub use im;
