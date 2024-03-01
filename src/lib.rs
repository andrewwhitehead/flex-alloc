//! Data structures with flexible storage
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(test)]
#[macro_use]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod borrow;

pub mod boxed;

pub(crate) mod error;

pub mod index;

pub mod storage;

pub mod vec;

pub use self::error::{InsertionError, StorageError};
