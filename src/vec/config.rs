use core::fmt::Debug;
use core::marker::PhantomData;
use core::mem::MaybeUninit;
use core::ptr::{self, NonNull};

use crate::error::StorageError;
use crate::index::{Grow, GrowDoubling, GrowExact, Index};
use crate::storage::alloc::{
    AllocHandle, AllocHandleParts, FatAllocHandle, FixedAlloc, SpillAlloc, ThinAllocHandle,
};
use crate::storage::{
    ArrayStorage, Global, Inline, InlineBuffer, RawAlloc, RawAllocIn, RawAllocNew, SpillStorage,
    Thin, WithAlloc,
};

use super::buffer::{VecBuffer, VecBufferNew, VecBufferSpawn, VecData, VecHeader};

pub trait VecAlloc: Debug {
    type RawAlloc: RawAlloc;
    type AllocHandle<T, I: Index>: AllocHandle<Alloc = Self::RawAlloc, Meta = VecData<T, I>>;
}

impl<A: RawAlloc> VecAlloc for A {
    type RawAlloc = A;
    type AllocHandle<T, I: Index> = FatAllocHandle<VecData<T, I>, A>;
}

pub trait VecConfig: Debug {
    type Buffer<T>: VecBuffer<Data = T, Index = Self::Index>;
    type Grow: Grow;
    type Index: Index;
}

impl<C: VecAlloc> VecConfig for C {
    type Buffer<T> = C::AllocHandle<T, Self::Index>;
    type Grow = GrowDoubling;
    type Index = usize;
}

pub trait VecConfigAlloc<T>: VecConfig {
    type Alloc: RawAlloc;

    fn allocator(buf: &Self::Buffer<T>) -> &Self::Alloc;
}

impl<T, C: VecConfig> VecConfigAlloc<T> for C
where
    C::Buffer<T>: AllocHandle<Meta = VecData<T, Self::Index>>,
{
    type Alloc = <C::Buffer<T> as AllocHandle>::Alloc;

    #[inline]
    fn allocator(buf: &Self::Buffer<T>) -> &Self::Alloc {
        buf.allocator()
    }
}

pub trait VecConfigAllocParts<T>: VecConfigAlloc<T> {
    fn vec_from_parts(
        data: NonNull<T>,
        length: Self::Index,
        capacity: Self::Index,
        alloc: Self::Alloc,
    ) -> Self::Buffer<T>;

    fn vec_into_parts(
        buffer: Self::Buffer<T>,
    ) -> (NonNull<T>, Self::Index, Self::Index, Self::Alloc);
}

impl<T, C: VecConfigAlloc<T>> VecConfigAllocParts<T> for C
where
    C::Buffer<T>: AllocHandleParts<Alloc = Self::Alloc, Meta = VecData<T, Self::Index>>,
{
    #[inline]
    fn vec_from_parts(
        data: NonNull<T>,
        length: Self::Index,
        capacity: Self::Index,
        alloc: Self::Alloc,
    ) -> Self::Buffer<T> {
        Self::Buffer::<T>::handle_from_parts(VecHeader { capacity, length }, data, alloc)
    }

    #[inline]
    fn vec_into_parts(
        buffer: Self::Buffer<T>,
    ) -> (NonNull<T>, Self::Index, Self::Index, Self::Alloc) {
        let (header, data, alloc) = buffer.handle_into_parts();
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
pub struct Custom<C: VecAlloc, I: Index = usize, G: Grow = GrowExact>(
    C::RawAlloc,
    PhantomData<(I, G)>,
);

// FIXME add generic ConstDefault trait
impl<C: RawAllocNew, I: Index, G: Grow> Custom<C, I, G> {
    #[inline]
    pub const fn new() -> Self {
        Self(C::NEW, PhantomData)
    }
}

impl<C: VecAlloc, I: Index, G: Grow> VecConfig for Custom<C, I, G> {
    type Buffer<T> = C::AllocHandle<T, Self::Index>;
    type Grow = G;
    type Index = I;
}

// impl<'a> VecAlloc for Fixed<'a> {
//     type RawAlloc = FixedAlloc<'a>;
//     type AllocHandle<T, I: Index> = FatAllocHandle<VecData<T, I>, Self::RawAlloc>;
// }

impl<const N: usize> VecConfig for Inline<N> {
    type Buffer<T> = InlineBuffer<T, N>;
    type Index = usize;
    type Grow = GrowExact;
}

impl VecAlloc for Thin {
    type RawAlloc = Global;
    type AllocHandle<T, I: Index> = ThinAllocHandle<VecData<T, I>, Global>;
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

impl<T, C: VecAlloc, I: Index, G: Grow> VecNewIn<T> for Custom<C, I, G> {
    type Config = Self;

    #[inline]
    fn vec_try_new_in(
        self,
        capacity: <Self::Config as VecConfig>::Index,
        exact: bool,
    ) -> Result<<Self::Config as VecConfig>::Buffer<T>, StorageError> {
        C::AllocHandle::alloc_handle_in(
            self.0,
            VecHeader {
                capacity,
                length: Index::ZERO,
            },
            exact,
        )
    }
}

impl<'a, T> VecNewIn<T> for ArrayStorage<&'a mut [MaybeUninit<T>]> {
    type Config = FixedAlloc<'a>;

    #[inline]
    fn vec_try_new_in(
        self,
        mut capacity: <Self::Config as VecConfig>::Index,
        exact: bool,
    ) -> Result<<Self::Config as VecConfig>::Buffer<T>, StorageError> {
        let len = self.0.len();
        if capacity.to_usize() > len {
            return Err(StorageError::CapacityLimit);
        }
        if !exact {
            capacity = Index::from_usize(len.min(<Self::Config as VecConfig>::Index::MAX_USIZE));
        }
        Ok(<Self::Config as VecConfig>::Buffer::<T>::handle_from_parts(
            VecHeader {
                capacity,
                length: Index::ZERO,
            },
            unsafe { NonNull::new_unchecked(self.0.as_mut_ptr()) }.cast(),
            FixedAlloc::default(),
        ))
    }
}

impl<'a, T: 'a> WithAlloc<'a> for ArrayStorage<&'a mut [MaybeUninit<T>]> {
    type NewIn<A: 'a> = SpillStorage<'a, &'a mut [MaybeUninit<T>], A>;

    #[inline]
    fn with_alloc_in<A: RawAlloc + 'a>(self, alloc: A) -> Self::NewIn<A> {
        SpillStorage::new_in(self.0, alloc)
    }
}

impl<'a, T, const N: usize> VecNewIn<T> for &'a mut ArrayStorage<[MaybeUninit<T>; N]> {
    type Config = FixedAlloc<'a>;

    #[inline]
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
        Ok(<Self::Config as VecConfig>::Buffer::<T>::handle_from_parts(
            VecHeader {
                capacity,
                length: Index::ZERO,
            },
            unsafe { NonNull::new_unchecked(self.0.as_mut_ptr()) }.cast(),
            FixedAlloc::default(),
        ))
    }
}

impl<'a, T: 'a, const N: usize> WithAlloc<'a> for &'a mut ArrayStorage<[MaybeUninit<T>; N]> {
    type NewIn<A: 'a> = SpillStorage<'a, &'a mut [MaybeUninit<T>], A>;

    #[inline]
    fn with_alloc_in<A: RawAlloc + 'a>(self, alloc: A) -> Self::NewIn<A> {
        SpillStorage::new_in(&mut self.0, alloc)
    }
}

impl<'a, T, A: RawAlloc> VecNewIn<T> for SpillStorage<'a, &'a mut [MaybeUninit<T>], A> {
    type Config = SpillAlloc<'a, A>;

    fn vec_try_new_in(
        self,
        mut capacity: <Self::Config as VecConfig>::Index,
        exact: bool,
    ) -> Result<<Self::Config as VecConfig>::Buffer<T>, StorageError> {
        if capacity > self.buffer.len() {
            return <Self::Config as VecConfig>::Buffer::<T>::alloc_handle_in(
                SpillAlloc::new(self.alloc, ptr::null()),
                VecHeader {
                    capacity,
                    length: Index::ZERO,
                },
                exact,
            );
        }
        if !exact {
            capacity = self.buffer.len();
        }
        let ptr = self.buffer.as_mut_ptr();
        Ok(<Self::Config as VecConfig>::Buffer::<T>::handle_from_parts(
            VecHeader {
                capacity,
                length: Index::ZERO,
            },
            unsafe { NonNull::new_unchecked(ptr) }.cast(),
            SpillAlloc::new(self.alloc, ptr.cast()),
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
