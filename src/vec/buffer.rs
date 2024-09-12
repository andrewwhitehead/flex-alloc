//! `Vec` buffer types and trait definitions.

use core::alloc::Layout;
use core::fmt::Debug;
use core::mem::{size_of, MaybeUninit};
use core::slice;

use crate::error::StorageError;
use crate::index::Index;
use crate::storage::alloc::{AllocHandle, AllocHeader, AllocLayout};
use crate::storage::utils::array_layout;
use crate::storage::{InlineBuffer, RawBuffer};

/// The header associated with each `Vec` instance.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct VecHeader<I: Index = usize> {
    /// The capacity of the buffer.
    pub capacity: I,
    /// The number of items stored in the buffer.
    pub length: I,
}

impl<I: Index> AllocHeader for VecHeader<I> {
    const EMPTY: Self = VecHeader {
        capacity: I::ZERO,
        length: I::ZERO,
    };

    #[inline]
    fn is_empty(&self) -> bool {
        self.capacity == I::ZERO
    }
}

/// An abstract type which captures the parameters of a `Vec` type's data representation.
#[derive(Debug)]
pub struct VecData<T, I: Index = usize>(T, I);

impl<T, I: Index> AllocLayout for VecData<T, I> {
    type Header = VecHeader<I>;
    type Data = T;

    #[inline]
    fn layout(header: &Self::Header) -> Result<Layout, StorageError> {
        array_layout::<T>(header.capacity.to_usize())
    }

    #[inline]
    fn update_header(header: &mut Self::Header, layout: Layout) {
        let t_size = size_of::<T>();
        header.capacity = I::from_usize(if t_size > 0 {
            (layout.size() / t_size).min(I::MAX_USIZE)
        } else {
            I::MAX_USIZE
        });
    }
}

/// A concrete `Vec` backing buffer.
pub trait VecBuffer: RawBuffer<RawData = Self::Item> {
    /// The type of the items stored in the `Vec`.
    type Item;

    /// The index type used to store the capacity and length.
    type Index: Index;

    /// Access the capacity of the buffer.
    fn capacity(&self) -> Self::Index;

    /// Access the number of items stored in the buffer.
    fn length(&self) -> Self::Index;

    /// Set the current length of the buffer.
    ///
    /// # Safety
    /// A zero length buffer may not have an active allocation, and so it is
    /// undefined behaviour to set its length, even if setting it to zero. Doing so
    /// may produce invalid memory access errors.
    unsafe fn set_length(&mut self, len: Self::Index);

    /// Access the contiguous memory contained in this buffer as a slice of
    /// `MaybeUnint<Self::Item>`. The length of this slice must correspond to
    /// `self.capacity()`.
    #[inline]
    fn as_uninit_slice(&mut self) -> &mut [MaybeUninit<Self::Item>] {
        unsafe { slice::from_raw_parts_mut(self.data_ptr_mut().cast(), self.capacity().to_usize()) }
    }

    /// Access the items contained in this buffer as a slice of `Self::Item`. The length
    /// of this slice must correspond to `self.length()`.
    #[inline]
    fn as_slice(&self) -> &[Self::Item] {
        unsafe { slice::from_raw_parts(self.data_ptr(), self.length().to_usize()) }
    }

    /// Access the items contained in this buffer as a mutable slice of `Self::Item`. The
    /// length of this slice must correspond to `self.length()`.
    #[inline]
    fn as_mut_slice(&mut self) -> &mut [Self::Item] {
        unsafe { slice::from_raw_parts_mut(self.data_ptr_mut(), self.length().to_usize()) }
    }

    /// Access an index of the buffer as a mutable reference to a `MaybeUninit<Self::Item>`.
    ///
    /// # Safety
    /// The index must be within the bounds of the buffer's capacity, otherwise a
    /// memory access error may occur.
    #[inline]
    unsafe fn uninit_index(&mut self, index: usize) -> &mut MaybeUninit<Self::Item> {
        &mut *self.data_ptr_mut().add(index).cast()
    }

    /// Attempt to resize this buffer to a new capacity. The `exact` flag determines
    /// whether a larger capacity would be acceptable.
    fn vec_try_resize(&mut self, capacity: Self::Index, exact: bool) -> Result<(), StorageError>;
}

impl<B, T, I: Index> VecBuffer for B
where
    B: AllocHandle<Meta = VecData<T, I>>,
{
    type Item = T;
    type Index = I;

    #[inline]
    fn capacity(&self) -> I {
        if self.is_empty_handle() {
            I::ZERO
        } else {
            unsafe { self.header() }.capacity
        }
    }

    #[inline]
    fn length(&self) -> I {
        if self.is_empty_handle() {
            I::ZERO
        } else {
            unsafe { self.header() }.length
        }
    }

    #[inline]
    unsafe fn set_length(&mut self, len: I) {
        self.header_mut().length = len;
    }

    #[inline]
    fn vec_try_resize(&mut self, capacity: Self::Index, exact: bool) -> Result<(), StorageError> {
        let length = self.length();
        self.resize_handle(VecHeader { capacity, length }, exact)?;
        Ok(())
    }
}

impl<'a, T: 'a, const N: usize> VecBuffer for InlineBuffer<T, N> {
    type Item = T;
    type Index = usize;

    #[inline]
    fn capacity(&self) -> usize {
        N
    }

    #[inline]
    fn length(&self) -> usize {
        self.length
    }

    #[inline]
    unsafe fn set_length(&mut self, len: usize) {
        self.length = len;
    }

    #[inline]
    fn vec_try_resize(&mut self, capacity: Self::Index, exact: bool) -> Result<(), StorageError> {
        if (!exact && capacity.to_usize() < N) || capacity.to_usize() == N {
            Ok(())
        } else {
            Err(StorageError::CapacityLimit)
        }
    }
}
