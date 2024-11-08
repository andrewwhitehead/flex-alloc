//! Backing storage types for collections.

use const_default::ConstDefault;

mod alloc;
mod array;
pub(crate) mod boxed;
mod bytes;
mod inline;
pub(crate) mod insert;
mod spill;
pub(crate) mod utils;

pub use self::{
    alloc::{BufferHeader, FatBuffer, ThinBuffer},
    array::ArrayStorage,
    bytes::ByteStorage,
    inline::{Inline, InlineBuffer},
    spill::SpillStorage,
};

/// Create a new array storage buffer for type `T` and maximum capacity `N`.
pub const fn array_storage<T, const N: usize>() -> ArrayStorage<T, N> {
    ArrayStorage::DEFAULT
}

/// Create a new byte storage buffer for a maximum byte capacity `N`.
pub const fn byte_storage<const N: usize>() -> ByteStorage<u8, N> {
    ByteStorage::DEFAULT
}

/// Create a new byte storage buffer for a maximum byte capacity `N`, with
/// a memory alignment matching type `T`.
pub const fn aligned_byte_storage<T, const N: usize>() -> ByteStorage<T, N> {
    ByteStorage::DEFAULT
}

/// Provide access to the associated data for abstract buffer types.
pub trait RawBuffer: Sized {
    /// The concrete data type.
    type RawData: ?Sized;

    /// Access the data as a readonly pointer.
    fn data_ptr(&self) -> *const Self::RawData;

    /// Access the data as a mutable pointer.
    fn data_ptr_mut(&mut self) -> *mut Self::RawData;
}
