//! Data structures with extra flexible storage.
//!
//! # Usage
//!
//! ## Fixed storage
//!
//! Containers may be allocated in fixed storage, a buffer which might be
//! stored on the stack or statically.
//!
//! ```
//! use flex_alloc::{boxed::Box, storage::byte_storage};
//!
//! let mut buf = byte_storage::<1024>();
//! let b = Box::new_in(22usize, &mut buf);
//! ```
//!
//! A fixed storage buffer may also be chained to an allocator, meaning that
//! and when the capacity of the buffer is exceeded, then the allocator
//! will be used to obtain additional memory. For critical sections where the
//! size of the input is variable but may often fit on the stack, this can
//! help to eliminate costly allocations and lead to performance improvements.
//!
//! ```
//! # #[cfg(feature = "alloc")] {
//! use flex_alloc::{storage::{array_storage, WithAlloc}, vec::Vec};
//!
//! let mut buf = array_storage::<_, 100>();
//! let mut v = Vec::new_in(buf.with_alloc());
//! v.extend(1..1000);
//! # }
//! ```
//!
//! ### Custom allocators
//!
//! TODO
//!
//! ### `allocator-api2` integration
//!
//! TODO
//!
//! ### `zeroize` integration
//!
//! Integration with `zeroize` is implemented at the allocator level in order
//! to ensure that all buffers are zeroized (including intermediate
//! allocations produced when a `Vec` is resized). This means that `Box` and
//! `Vec` will implement `Zeroize` and `ZeroizeOnDrop` when appropriate.
//!
//! ```
//! # #[cfg(feature = "zeroize")] {
//! use flex_alloc::boxed::ZeroizingBox;
//!
//! let b = ZeroizingBox::new(99usize);
//! # }
//! ```
//!
//! ```
//! # #[cfg(feature = "zeroize")] {
//! use flex_alloc::vec::ZeroizingVec;
//!
//! let v = ZeroizingVec::from([1, 2, 3]);
//! # }
//! ```
//!
//! Fixed storage buffers may be wrapped in `Zeroizing`. Allocations produced
//! on overflow when `with_alloc` is used will automatically be zeroized as
//! well.
//!
//! ```
//! # #[cfg(feature = "zeroize")] {
//! use flex_alloc::{storage::{array_storage, WithAlloc}, vec::Vec};
//! use zeroize::Zeroizing;
//!
//! let mut buf = Zeroizing::new(array_storage::<usize, 10>());
//! let v = Vec::new_in(buf.with_alloc());
//! # }
//! ```
//!
//! ### Inline vectors
//!
//! `Vec` can support inline storage of the contained data. This may be
//! appropriate when the maximum number of elements is known and the `Vec`
//! is not being passed around to other functions:
//!
//! ```
//! use flex_alloc::vec::InlineVec;
//!
//! let v = InlineVec::<usize, 5>::from_iter([1, 2, 3, 4, 5]);
//! ```
//!
//! ```
//! use flex_alloc::{vec, storage::Inline};
//!
//! let v = vec![in Inline::<5>; 1, 2, 3, 4, 5];
//! ```
//!
//! ### Thin vectors
//!
//! Like the `thin-vec` crate (but without compatibility with Gecko), `Vec`
//! may be customized to use a pointer-sized representation with the capacity
//! and length stored in the allocation.
//!
//! Note that unlike with a standard `Vec`, an allocation is required to store
//! a collection of ZSTs (zero-sized types).
//!
//! ```
//! # #[cfg(feature = "alloc")] {
//! use flex_alloc::vec::ThinVec;
//!
//! let v = ThinVec::<usize>::from(&[1, 2, 3, 4, 5]);
//! # }
//! ```
//!
//! ### Custom index sizes
//!
//! `Vec` may be parameterized to use an alternative index type when memory
//! consumption is a concern. The supported index types are `u8`, `u16`,
//! `u32`, and `usize` (the default).
//!
//! ```
//! # #[cfg(feature = "alloc")] {
//! use flex_alloc::{storage::Global, vec::{config::Custom, Vec}};
//!
//! type Cfg = Custom<Global, u8>;
//! let v = Vec::<usize, Cfg>::new();
//! # }
//! ```

#![cfg_attr(not(feature = "std"), no_std)]
// #![warn(missing_docs)]

#[doc = include_str!("../README.md")]
#[cfg(doctest)]
struct _ReadmeDoctests;

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
