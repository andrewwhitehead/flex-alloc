use core::marker::PhantomData;
use core::mem::MaybeUninit;
use core::ptr::NonNull;

use crate::error::StorageError;
use crate::index::{Grow, GrowDoubling, GrowExact, Index};
use crate::storage::alloc::{AllocHandle, FatAllocHandle, ThinAllocHandle};
use crate::storage::{Fixed, FixedBuffer, Global, Inline, InlineBuffer, RawAlloc, RawAllocIn};
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

    fn allocator(buf: &Self::Buffer<T>) -> &Self::Alloc {
        buf.allocator()
    }
}

pub trait VecConfigNew<T>: VecConfig {
    const NEW: Self::Buffer<T>;

    fn vec_try_new(capacity: Self::Index, exact: bool) -> Result<Self::Buffer<T>, StorageError>;
}

impl<T, C: VecConfig> VecConfigNew<T> for C
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
