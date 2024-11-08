//! Data structures with extra flexible storage.
//!
//! This crate provides highly flexible container types (currently
//! [`Box`](crate::boxed::Box), [`Cow`](crate::borrow::Cow), and
//! [`Vec`](crate::vec::Vec)) which mimic the API provided in `std`,
//! with allocation flexibility going beyond what is supported by
//! unstable features such as `allocator-api`.
//!
//! Both `no-std` and `no-alloc` environments are supported.
//!
//! ## Highlights
//!
//! - Optional `alloc` support, such that application may easily alternate between fixed buffers and heap allocation.
//! - Custom allocator implementations, including the ability to spill from a small stack allocation to a heap allocation.
//! - Additional fallible update methods, allowing for more ergonomic fixed size collections and handling of allocation errors.
//! - `const` initializers.
//! - Support for inline collections.
//! - Custom index types and growth behavior to manage memory usage.
//!
//! ## Feature flags
//!
//! - The `std` flag (off by default) enables compatibility with the `std::error::Error` trait for error types, adds `io::Write` support to `Vec`, and also enables the `alloc` feature.
//! - With the `alloc` feature (on by default), access to the global allocator is enabled, and default constructors for allocated containers (such as `Vec::new`) are supported.
//! - The `allocator-api2` feature enables integration with the `allocator-api2` crate, which offers support for the `allocator-api` feature set on stable Rust. This can allow for allocators implementing the API to be passed to `Vec::new_in`.
//! - The `nightly` feature enables compatibility with the unstable Rust `allocator-api` feature. This requires a nightly Rust compiler build.
//! - The `zeroize` feature enables integration with the `zeroize` crate, including a zeroizing allocator. This can be used to automatically zero out allocated memory for allocated types, including the intermediate buffers produced during resizing in the case of `Vec`.

#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(feature = "nightly", feature(allocator_api, coerce_unsized, unsize))]
#![warn(missing_docs)]

#[doc = include_str!("../README.md")]
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

pub mod capacity;

pub(crate) mod error;

pub mod storage;

pub mod vec;

pub use self::error::{StorageError, UpdateError};
