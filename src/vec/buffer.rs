use core::alloc::Layout;
use core::fmt::Debug;
use core::marker::PhantomData;
use core::mem::{size_of, MaybeUninit};
use core::slice;

use crate::error::StorageError;
use crate::index::Index;
use crate::storage::alloc::{AllocHandle, AllocHandleNew, AllocHeader, AllocLayout};
use crate::storage::utils::array_layout;
use crate::storage::{InlineBuffer, RawBuffer};

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct VecHeader<I: Index = usize> {
    pub capacity: I,
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

#[derive(Debug)]
pub struct VecData<T, I: Index = usize>(PhantomData<(T, I)>);

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

pub trait VecBuffer: RawBuffer<RawData = Self::Data> {
    type Data;
    type Index: Index;

    fn capacity(&self) -> Self::Index;

    fn length(&self) -> Self::Index;

    /// # Safety
    /// A zero length buffer may not have an active allocation, and so it is
    /// undefined behaviour to set its length, even if setting it to zero. Doing so
    /// may produce invalid memory access errors.
    unsafe fn set_length(&mut self, len: Self::Index);

    #[inline]
    fn as_uninit_slice(&mut self) -> &mut [MaybeUninit<Self::Data>] {
        unsafe { slice::from_raw_parts_mut(self.data_ptr_mut().cast(), self.capacity().to_usize()) }
    }

    #[inline]
    fn as_slice(&self) -> &[Self::Data] {
        unsafe { slice::from_raw_parts(self.data_ptr(), self.length().to_usize()) }
    }

    #[inline]
    fn as_mut_slice(&mut self) -> &mut [Self::Data] {
        unsafe { slice::from_raw_parts_mut(self.data_ptr_mut(), self.length().to_usize()) }
    }

    /// # Safety
    /// The index must be within the bounds of the buffer's capacity, otherwise a
    /// memory access error may occur.
    #[inline]
    unsafe fn uninit_index(&mut self, index: usize) -> &mut MaybeUninit<Self::Data> {
        &mut *self.data_ptr_mut().add(index).cast()
    }

    fn vec_try_resize(&mut self, capacity: Self::Index, exact: bool) -> Result<(), StorageError>;
}

impl<B, T, I: Index> VecBuffer for B
where
    B: AllocHandle<Meta = VecData<T, I>>,
{
    type Data = T;
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

pub trait VecBufferNew: VecBufferSpawn {
    const NEW: Self;

    fn vec_try_new(capacity: Self::Index, exact: bool) -> Result<Self, StorageError>;
}

impl<B> VecBufferNew for B
where
    B: VecBufferSpawn + AllocHandleNew<Meta = VecData<Self::Data, Self::Index>>,
{
    const NEW: Self = Self::NEW;

    #[inline]
    fn vec_try_new(capacity: Self::Index, exact: bool) -> Result<Self, StorageError> {
        Self::alloc_handle_in(
            Self::NEW_ALLOC,
            VecHeader {
                capacity,
                length: Self::Index::ZERO,
            },
            exact,
        )
    }
}

pub trait VecBufferSpawn: VecBuffer {
    fn vec_try_spawn(&self, capacity: Self::Index, exact: bool) -> Result<Self, StorageError>;
}

impl<B, T, I: Index> VecBufferSpawn for B
where
    B: AllocHandle<Meta = VecData<T, I>>,
    B::Alloc: Clone,
{
    #[inline]
    fn vec_try_spawn(&self, capacity: Self::Index, exact: bool) -> Result<Self, StorageError> {
        self.spawn_handle(
            VecHeader {
                capacity,
                length: Index::ZERO,
            },
            exact,
        )
    }
}

impl<'a, T: 'a, const N: usize> VecBuffer for InlineBuffer<T, N> {
    type Data = T;
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

impl<T, const N: usize> VecBufferNew for InlineBuffer<T, N> {
    const NEW: Self = InlineBuffer::new();

    #[inline]
    fn vec_try_new(capacity: Self::Index, exact: bool) -> Result<Self, StorageError> {
        if (!exact && capacity.to_usize() < N) || capacity.to_usize() == N {
            Ok(Self::NEW)
        } else {
            Err(StorageError::CapacityLimit)
        }
    }
}

impl<T, const N: usize> VecBufferSpawn for InlineBuffer<T, N> {
    #[inline]
    fn vec_try_spawn(&self, capacity: Self::Index, exact: bool) -> Result<Self, StorageError> {
        Self::vec_try_new(capacity, exact)
    }
}
