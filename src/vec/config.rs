use core::marker::PhantomData;
use core::mem::MaybeUninit;
use core::ptr::NonNull;

use crate::error::StorageError;
use crate::index::{Grow, GrowDoubling, GrowExact, Index};
use crate::storage::alloc::{AllocHandle, FatAllocHandle, ThinAllocHandle};
use crate::storage::{
    AllocHandleParts, Fixed, FixedBuffer, Global, Inline, InlineBuffer, RawAlloc, RawAllocIn,
};
use crate::Thin;

use super::buffer::{VecBuffer, VecBufferNew, VecBufferSpawn, VecData, VecHeader};

pub trait VecConfigIndex {
    type IndexBuffer<T, I: Index>: VecBuffer<Data = T, Index = I>;
}

impl<A: RawAlloc> VecConfigIndex for A {
    type IndexBuffer<T, I: Index> = FatAllocHandle<VecData<T, I>, A>;
}

pub trait VecConfig {
    type Buffer<T>: VecBuffer<Data = T, Index = Self::Index>;
    type Grow: Grow;
    type Index: Index;
}

impl<C: VecConfigIndex> VecConfig for C {
    type Buffer<T> = C::IndexBuffer<T, Self::Index>;
    type Grow = GrowDoubling;
    type Index = usize;
}

pub trait VecConfigAllocator<T>: VecConfig {
    type Alloc: RawAlloc;

    fn allocator(buf: &Self::Buffer<T>) -> &Self::Alloc;
}

impl<T, C: VecConfig> VecConfigAllocator<T> for C
where
    C::Buffer<T>: AllocHandle,
{
    type Alloc = <C::Buffer<T> as AllocHandle>::Alloc;

    #[inline]
    fn allocator(buf: &Self::Buffer<T>) -> &Self::Alloc {
        buf.allocator()
    }
}

pub trait VecConfigAllocParts<T>: VecConfigAllocator<T> {
    fn vec_from_parts(
        data: NonNull<T>,
        length: Self::Index,
        capacity: Self::Index,
        alloc: Self::Alloc,
    ) -> Self::Buffer<T>;

    fn vec_into_parts(buf: Self::Buffer<T>) -> (NonNull<T>, Self::Index, Self::Index, Self::Alloc);
}

impl<T, C: VecConfig> VecConfigAllocParts<T> for C
where
    C::Buffer<T>: AllocHandleParts<Meta = VecData<T, Self::Index>>,
{
    #[inline]
    fn vec_from_parts(
        data: NonNull<T>,
        length: Self::Index,
        capacity: Self::Index,
        alloc: Self::Alloc,
    ) -> Self::Buffer<T> {
        C::Buffer::<T>::buffer_from_parts(VecHeader { capacity, length }, data, alloc)
    }

    #[inline]
    fn vec_into_parts(buf: Self::Buffer<T>) -> (NonNull<T>, Self::Index, Self::Index, Self::Alloc) {
        let (header, data, alloc) = buf.buffer_into_parts();
        (data, header.length, header.capacity, alloc)
    }
}

pub trait VecConfigNew<T>: VecConfigSpawn<T> {
    const NEW: Self::Buffer<T>;

    fn vec_try_new(capacity: Self::Index, exact: bool) -> Result<Self::Buffer<T>, StorageError>;
}

impl<T, C: VecConfigSpawn<T>> VecConfigNew<T> for C
where
    Self::Buffer<T>: VecBufferNew,
{
    const NEW: Self::Buffer<T> = Self::Buffer::<T>::NEW;

    #[inline]
    fn vec_try_new(capacity: Self::Index, exact: bool) -> Result<Self::Buffer<T>, StorageError> {
        Self::Buffer::<T>::vec_try_new(capacity, exact)
    }
}

pub trait VecConfigSpawn<T>: VecConfig {
    fn vec_try_spawn(
        buf: &Self::Buffer<T>,
        capacity: Self::Index,
        exact: bool,
    ) -> Result<Self::Buffer<T>, StorageError>;
}

impl<T, C: VecConfig> VecConfigSpawn<T> for C
where
    Self::Buffer<T>: VecBufferSpawn,
{
    #[inline]
    fn vec_try_spawn(
        buf: &Self::Buffer<T>,
        capacity: Self::Index,
        exact: bool,
    ) -> Result<Self::Buffer<T>, StorageError> {
        Self::Buffer::<T>::vec_try_spawn(buf, capacity, exact)
    }
}

#[derive(Debug, Default)]
pub struct Custom<C, I: Index = usize, G: Grow = GrowExact>(PhantomData<(C, I, G)>);

impl<C: VecConfigIndex, I: Index, G: Grow> VecConfig for Custom<C, I, G> {
    type Buffer<T> = C::IndexBuffer<T, Self::Index>;
    type Grow = G;
    type Index = I;
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

impl VecConfigIndex for Thin {
    type IndexBuffer<T, I: Index> = ThinAllocHandle<VecData<T, I>, Global>;
}

pub trait VecNewIn<T> {
    type Config: VecConfig;

    fn vec_try_new_in(
        self,
        capacity: <Self::Config as VecConfig>::Index,
        exact: bool,
    ) -> Result<<Self::Config as VecConfig>::Buffer<T>, StorageError>;
}

impl<T, C: RawAllocIn> VecNewIn<T> for C {
    type Config = C::RawAlloc;

    #[inline]
    fn vec_try_new_in(
        self,
        capacity: <Self::Config as VecConfig>::Index,
        exact: bool,
    ) -> Result<<Self::Config as VecConfig>::Buffer<T>, StorageError> {
        <Self::Config as VecConfig>::Buffer::<T>::alloc_handle_in(
            self,
            VecHeader {
                capacity,
                length: Index::ZERO,
            },
            exact,
        )
    }
}

impl<'a, T, const N: usize> VecNewIn<T> for &'a mut [MaybeUninit<T>; N] {
    type Config = Fixed<'a>;

    fn vec_try_new_in(
        self,
        mut capacity: <Self::Config as VecConfig>::Index,
        exact: bool,
    ) -> Result<<Self::Config as VecConfig>::Buffer<T>, StorageError> {
        if capacity > N {
            return Err(StorageError::CapacityLimit);
        }
        if !exact {
            capacity = N;
        }
        Ok(FixedBuffer::new(
            VecHeader {
                capacity,
                length: 0,
            },
            unsafe { NonNull::new_unchecked(self.as_mut_ptr()).cast() },
        ))
    }
}

impl<T, const N: usize> VecNewIn<T> for Inline<N> {
    type Config = Inline<N>;

    #[inline]
    fn vec_try_new_in(
        self,
        capacity: <Self::Config as VecConfig>::Index,
        exact: bool,
    ) -> Result<<Self::Config as VecConfig>::Buffer<T>, StorageError> {
        if capacity > N || (capacity < N && exact) {
            return Err(StorageError::CapacityLimit);
        }
        Ok(InlineBuffer::new())
    }
}

impl<T> VecNewIn<T> for Thin {
    type Config = Thin;

    #[inline]
    fn vec_try_new_in(
        self,
        capacity: <Self::Config as VecConfig>::Index,
        exact: bool,
    ) -> Result<<Self::Config as VecConfig>::Buffer<T>, StorageError> {
        <Self::Config as VecConfig>::Buffer::<T>::alloc_handle_in(
            Global,
            VecHeader {
                capacity,
                length: Index::ZERO,
            },
            exact,
        )
    }
}
