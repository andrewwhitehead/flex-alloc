//! Flexible vectors
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(test)]
#[macro_use]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod boxed;

pub(crate) mod error;

pub(crate) mod index;

pub mod storage;

pub mod vec;

pub use {
    self::error::{InsertionError, StorageError},
    self::storage::{
        aligned_byte_buffer, array_buffer, byte_buffer, Alloc, Fixed, Inline, ThinAlloc,
    },
    self::vec::Vec,
};
