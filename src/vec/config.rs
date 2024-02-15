use core::mem::MaybeUninit;
use core::ptr::NonNull;

use crate::error::StorageError;
use crate::index::Index;
use crate::storage::utils::min_non_zero_cap;
use crate::storage::{
    Alloc, AllocBuffer, AllocBufferNew, AllocBufferParts, AllocMethod, Fixed, FixedBuffer, Global,
    Inline, InlineBuffer, RawAlloc, Thin, ThinAlloc,
};

use super::buffer::{VecBuffer, VecData, VecHeader};

pub trait Grow<I: Index> {
    fn next_capacity<T>(prev: I, minimum: I) -> I;
}

#[derive(Debug, Copy, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct GrowExact;

impl<I: Index> Grow<I> for GrowExact {
    #[inline]
    fn next_capacity<T>(_prev: I, minimum: I) -> I {
        minimum
    }
}

#[derive(Debug, Copy, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct GrowDoubling;

impl<I: Index> Grow<I> for GrowDoubling {
    #[inline]
    fn next_capacity<T>(prev: I, minimum: I) -> I {
        let preferred = if prev == I::ZERO {
            I::from_usize(min_non_zero_cap::<T>())
        } else {
            prev.saturating_mul(2)
        };
        preferred.max(minimum)
    }
}
pub trait VecConfig {
    type Buffer<T>: VecBuffer<Data = T, Index = Self::Index>;
    type Index: Index;
    type Grow: Grow<Self::Index>;
}

pub trait VecConfigNew<T>: VecConfig {
    const NEW: Self::Buffer<T>;
}

pub trait VecConfigAllocIn<T>: VecConfig {
    type Alloc: RawAlloc;

    fn allocator(buf: &Self::Buffer<T>) -> &Self::Alloc;

    fn vec_new_in(alloc: Self::Alloc) -> Self::Buffer<T>;

    fn vec_with_capacity_in(
        capacity: Self::Index,
        alloc: Self::Alloc,
    ) -> Result<Self::Buffer<T>, StorageError>;
}

pub trait VecConfigAlloc<T>: VecConfigAllocIn<T> {
    fn vec_with_capacity(capacity: Self::Index) -> Result<Self::Buffer<T>, StorageError>;
}

pub trait VecConfigAllocParts<T>: VecConfigAllocIn<T> {
    fn vec_from_parts(
        header: VecHeader<Self::Index>,
        data: NonNull<T>,
        alloc: Self::Alloc,
    ) -> Self::Buffer<T>;

    fn vec_into_parts(buf: Self::Buffer<T>) -> (VecHeader<Self::Index>, NonNull<T>, Self::Alloc);
}

impl<T, C: VecConfigAllocIn<T>> VecConfigAllocParts<T> for C
where
    C::Buffer<T>: AllocBufferParts<Alloc = Self::Alloc, Meta = VecData<T, Self::Index>>,
{
    fn vec_from_parts(
        header: VecHeader<Self::Index>,
        data: NonNull<T>,
        alloc: Self::Alloc,
    ) -> Self::Buffer<T> {
        Self::Buffer::<T>::buffer_from_parts(header, data, alloc)
    }

    fn vec_into_parts(buf: Self::Buffer<T>) -> (VecHeader<Self::Index>, NonNull<T>, Self::Alloc) {
        buf.buffer_into_parts()
    }
}

impl<I: Index, M: AllocMethod> VecConfig for Alloc<I, M> {
    type Buffer<T> = M::Buffer<VecData<T, I>>;
    type Index = I;
    type Grow = GrowDoubling;
}

impl<T, I: Index, M: AllocMethod> VecConfigNew<T> for Alloc<I, M>
where
    Self::Buffer<T>: AllocBufferNew,
{
    const NEW: Self::Buffer<T> = Self::Buffer::NEW;
}

impl<I: Index, T, M: AllocMethod> VecConfigAllocIn<T> for Alloc<I, M> {
    type Alloc = M::Alloc;

    #[inline]
    fn allocator(buf: &Self::Buffer<T>) -> &Self::Alloc {
        buf.allocator()
    }

    #[inline]
    fn vec_new_in(alloc: Self::Alloc) -> Self::Buffer<T> {
        M::Buffer::new_buffer(alloc)
    }

    #[inline]
    fn vec_with_capacity_in(
        capacity: Self::Index,
        alloc: Self::Alloc,
    ) -> Result<Self::Buffer<T>, StorageError> {
        M::Buffer::alloc_buffer(
            alloc,
            VecHeader {
                capacity,
                length: I::ZERO,
            },
        )
    }
}

impl<I: Index, T> VecConfigAlloc<T> for Alloc<I, Global> {
    fn vec_with_capacity(capacity: Self::Index) -> Result<Self::Buffer<T>, StorageError> {
        Self::vec_with_capacity_in(capacity, Global)
    }
}

impl<'a> VecConfig for Fixed<'a> {
    type Buffer<T> = FixedBuffer<VecHeader<usize>, T>;
    type Index = usize;
    type Grow = GrowExact;
}

impl<const N: usize> VecConfig for Inline<N> {
    type Buffer<T> = InlineBuffer<T, N>;
    type Index = usize;
    type Grow = GrowExact;
}

impl<T, const N: usize> VecConfigNew<T> for Inline<N> {
    const NEW: Self::Buffer<T> = InlineBuffer {
        data: unsafe { MaybeUninit::uninit().assume_init() },
        length: 0,
    };
}

impl<I: Index> VecConfig for ThinAlloc<I> {
    type Buffer<T> = <Thin as AllocMethod>::Buffer<VecData<T, I>>;
    type Index = I;
    type Grow = GrowDoubling;
}

impl<T, I: Index> VecConfigNew<T> for ThinAlloc<I>
where
    Self::Buffer<T>: AllocBufferNew,
{
    const NEW: Self::Buffer<T> = Self::Buffer::NEW;
}

impl<I: Index, T> VecConfigAlloc<T> for ThinAlloc<I> {
    fn vec_with_capacity(capacity: Self::Index) -> Result<Self::Buffer<T>, StorageError> {
        Self::vec_with_capacity_in(capacity, Global)
    }
}

impl<I: Index, T> VecConfigAllocIn<T> for ThinAlloc<I> {
    type Alloc = <Self::Buffer<T> as AllocBuffer>::Alloc;

    #[inline]
    fn allocator(buf: &Self::Buffer<T>) -> &Self::Alloc {
        buf.allocator()
    }

    #[inline]
    fn vec_new_in(alloc: Self::Alloc) -> Self::Buffer<T> {
        Self::Buffer::new_buffer(alloc)
    }

    #[inline]
    fn vec_with_capacity_in(
        capacity: Self::Index,
        alloc: Self::Alloc,
    ) -> Result<Self::Buffer<T>, StorageError> {
        Self::Buffer::alloc_buffer(
            alloc,
            VecHeader {
                capacity,
                length: I::ZERO,
            },
        )
    }
}
