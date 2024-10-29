//! Secure memory management for flex-alloc.

#![warn(missing_docs)]

#[cfg(all(not(unix), not(windows), not(miri)))]
compile_error!("Only Unix and Windows platforms are currently supported");

mod bytes;
mod protect;

pub use flex_alloc;

pub use self::{
    bytes::FillBytes,
    protect::{ExposeProtected, ProtectedInit, ProtectedInitSlice},
};

pub mod alloc;
pub mod boxed;
pub mod stack;
pub mod vec;
