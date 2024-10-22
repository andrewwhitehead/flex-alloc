//! Data structures with extra flexible storage.

#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(feature = "nightly", feature(allocator_api))]
#![warn(missing_docs)]

#[doc = include_str!("../../README.md")]
#[cfg(doctest)]
struct _ReadmeDoctests;

#[cfg(test)]
#[macro_use]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc as alloc_crate;

pub mod alloc;

pub mod boxed;

pub mod borrow;

pub(crate) mod error;

pub mod index;

pub mod storage;

pub mod vec;

pub use self::error::{StorageError, UpdateError};
