//! `Vec` buffer types and trait definitions.

use core::alloc::Layout;
use core::fmt::Debug;
use core::mem::MaybeUninit;
use core::ptr::NonNull;
use core::slice;

use crate::alloc::Allocator;
use crate::error::StorageError;
use crate::index::Index;
use crate::storage::{BufferHeader, FatBuffer, InlineBuffer, RawBuffer, ThinBuffer};

/// The header associated with each `Vec` instance.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct VecHeader<I: Index = usize> {
    /// The capacity of the buffer.
    pub capacity: I,
    /// The number of items stored in the buffer.
    pub length: I,
}

impl<T, I: Index> BufferHeader<T> for VecHeader<I> {
    const EMPTY: Self = VecHeader {
        capacity: I::ZERO,
        length: I::ZERO,
    };

    #[inline]
    fn is_empty(&self) -> bool {
        self.capacity == I::ZERO
    }

    #[inline]
    fn layout(&self) -> Result<Layout, core::alloc::LayoutError> {
        Layout::array::<T>(self.capacity.to_usize())
    }

    #[inline]
    fn update_for_alloc(&mut self, ptr: NonNull<[u8]>, exact: bool) -> NonNull<T> {
        if !exact {
            let t_size = size_of::<T>();
            self.capacity = if t_size > 0 {
                I::from_usize((ptr.len() / t_size).min(I::MAX_USIZE))
            } else {
                I::from_usize(I::MAX_USIZE)
            };
        }
        ptr.cast()
    }
}

/// An abstract type which captures the parameters of a `Vec` type's data representation.
#[derive(Debug)]
pub struct VecData<T, I: Index = usize>(T, I);

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
    /// undefined behavior to set its length, even if setting it to zero. Doing so
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
    fn grow_buffer(&mut self, capacity: Self::Index, exact: bool) -> Result<(), StorageError>;

    /// Attempt to resize this buffer to a new, smaller capacity.
    fn shrink_buffer(&mut self, capacity: Self::Index) -> Result<(), StorageError>;
}

impl<T, I: Index, A: Allocator> VecBuffer for FatBuffer<T, VecHeader<I>, A> {
    type Item = T;
    type Index = I;

    #[inline]
    fn capacity(&self) -> I {
        self.header.capacity
    }

    #[inline]
    fn length(&self) -> I {
        self.header.length
    }

    #[inline]
    unsafe fn set_length(&mut self, len: I) {
        self.header.length = len;
    }

    #[inline]
    fn grow_buffer(&mut self, capacity: Self::Index, exact: bool) -> Result<(), StorageError> {
        let length = self.length();
        self.grow(VecHeader { capacity, length }, exact)?;
        Ok(())
    }

    #[inline]
    fn shrink_buffer(&mut self, capacity: Self::Index) -> Result<(), StorageError> {
        let length = self.length();
        self.shrink(VecHeader { capacity, length })?;
        Ok(())
    }
}

impl<T, I: Index, A: Allocator> VecBuffer for ThinBuffer<T, VecHeader<I>, A> {
    type Item = T;
    type Index = I;

    #[inline]
    fn capacity(&self) -> I {
        self.header().capacity
    }

    #[inline]
    fn length(&self) -> I {
        self.header().length
    }

    #[inline]
    unsafe fn set_length(&mut self, len: I) {
        self.set_header(VecHeader {
            capacity: self.capacity(),
            length: len,
        });
    }

    #[inline]
    fn grow_buffer(&mut self, capacity: Self::Index, exact: bool) -> Result<(), StorageError> {
        let length = self.length();
        self.grow(VecHeader { capacity, length }, exact)?;
        Ok(())
    }

    #[inline]
    fn shrink_buffer(&mut self, capacity: Self::Index) -> Result<(), StorageError> {
        let length = self.length();
        self.shrink(VecHeader { capacity, length })?;
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
    fn grow_buffer(&mut self, capacity: Self::Index, exact: bool) -> Result<(), StorageError> {
        if (!exact && capacity.to_usize() < N) || capacity.to_usize() == N {
            Ok(())
        } else {
            Err(StorageError::CapacityLimit)
        }
    }

    #[inline]
    fn shrink_buffer(&mut self, _capacity: Self::Index) -> Result<(), StorageError> {
        Ok(())
    }
}
