//! `Vec` configuration types and trait definitions.

use core::fmt::Debug;
use core::marker::PhantomData;
use core::mem::MaybeUninit;
use core::ptr::{self, NonNull};

use const_default::ConstDefault;

use crate::error::StorageError;
use crate::index::{Grow, GrowDoubling, GrowExact, Index};
use crate::storage::alloc::{
    AllocHandle, AllocHandleParts, FatAllocHandle, FixedAlloc, RawAllocDefault, SpillAlloc,
    ThinAllocHandle,
};
use crate::storage::{
    ArrayStorage, Global, Inline, InlineBuffer, RawAlloc, RawAllocIn, SpillStorage, Thin,
};

use super::buffer::{VecBuffer, VecData, VecHeader};

/// Define the associated allocation handle for a `Vec` allocator.
pub trait VecAllocHandle {
    /// The associated allocator.
    type RawAlloc: RawAlloc;

    /// The type of the allocation handle.
    type AllocHandle<T, I: Index>: AllocHandle<Meta = VecData<T, I>, Alloc = Self::RawAlloc>;
}

impl<A: RawAlloc> VecAllocHandle for A {
    type RawAlloc = A;
    type AllocHandle<T, I: Index> = FatAllocHandle<VecData<T, I>, A>;
}

/// Define the associated types for `Vec` instances.
pub trait VecConfig {
    /// The internal buffer type.
    type Buffer<T>: VecBuffer<Item = T, Index = Self::Index>;

    /// The growth strategy.
    type Grow: Grow;

    /// The index type used to define the capacity and length.
    type Index: Index;
}

impl<H: VecAllocHandle> VecConfig for H {
    type Buffer<T> = H::AllocHandle<T, Self::Index>;
    type Grow = GrowDoubling;
    type Index = usize;
}

/// Configuration for `Vec` types supporting an allocator.
pub trait VecConfigAlloc<T>: VecConfig {
    /// The allocator instance type.
    type Alloc: RawAlloc;

    /// Access a reference to the allocator instance.
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

/// Support assembly and disassembly of an allocated `Vec` buffer.
pub trait VecConfigAllocParts<T>: VecConfigAlloc<T> {
    /// Create a `Vec` buffer instance from its constituent parts.
    fn vec_buffer_from_parts(
        data: NonNull<T>,
        length: Self::Index,
        capacity: Self::Index,
        alloc: Self::Alloc,
    ) -> Self::Buffer<T>;

    /// Disassemble a `Vec` buffer instance into its constituent parts.
    fn vec_buffer_into_parts(
        buffer: Self::Buffer<T>,
    ) -> (NonNull<T>, Self::Index, Self::Index, Self::Alloc);
}

impl<T, C: VecConfigAlloc<T>> VecConfigAllocParts<T> for C
where
    C::Buffer<T>: AllocHandleParts<Alloc = Self::Alloc, Meta = VecData<T, Self::Index>>,
{
    #[inline]
    fn vec_buffer_from_parts(
        data: NonNull<T>,
        length: Self::Index,
        capacity: Self::Index,
        alloc: Self::Alloc,
    ) -> Self::Buffer<T> {
        Self::Buffer::<T>::handle_from_parts(VecHeader { capacity, length }, data, alloc)
    }

    #[inline]
    fn vec_buffer_into_parts(
        buffer: Self::Buffer<T>,
    ) -> (NonNull<T>, Self::Index, Self::Index, Self::Alloc) {
        let (header, data, alloc) = buffer.handle_into_parts();
        (data, header.length, header.capacity, alloc)
    }
}

/// Support creation of new `Vec` instances without a storage reference.
pub trait VecConfigNew<T>: VecConfigSpawn<T> {
    /// Constant initializer for an empty buffer.
    const EMPTY_BUFFER: Self::Buffer<T>;

    /// Try to create a new buffer instance with a given capacity.
    fn vec_buffer_try_new(
        capacity: Self::Index,
        exact: bool,
    ) -> Result<Self::Buffer<T>, StorageError>;
}

impl<T, C> VecConfigNew<T> for C
where
    C: VecConfig + VecAllocHandle,
    C::Buffer<T>: ConstDefault + AllocHandle<Meta = VecData<T, C::Index>>,
    C::RawAlloc: Clone,
{
    const EMPTY_BUFFER: Self::Buffer<T> = C::Buffer::<T>::DEFAULT;

    #[inline]
    fn vec_buffer_try_new(
        capacity: Self::Index,
        exact: bool,
    ) -> Result<Self::Buffer<T>, StorageError> {
        let mut buf = Self::EMPTY_BUFFER;
        buf.resize_handle(
            VecHeader {
                capacity,
                length: Self::Index::ZERO,
            },
            exact,
        )?;
        Ok(buf)
    }
}

/// Support creation of new `Vec` buffer instances from an existing instance.
pub trait VecConfigSpawn<T>: VecConfig {
    /// Try to create a new buffer instance with a given capacity.
    fn vec_buffer_try_spawn(
        buf: &Self::Buffer<T>,
        capacity: Self::Index,
        exact: bool,
    ) -> Result<Self::Buffer<T>, StorageError>;
}

impl<T, C> VecConfigSpawn<T> for C
where
    C: VecAllocHandle,
    C::RawAlloc: Clone,
{
    #[inline]
    fn vec_buffer_try_spawn(
        buf: &Self::Buffer<T>,
        capacity: Self::Index,
        exact: bool,
    ) -> Result<Self::Buffer<T>, StorageError> {
        buf.spawn_handle(
            VecHeader {
                capacity,
                length: Index::ZERO,
            },
            exact,
        )
    }
}

/// Parameterize `Vec` with a custom index type or growth behavior.
#[derive(Debug, Default)]
pub struct Custom<H: VecAllocHandle, I: Index = usize, G: Grow = GrowExact> {
    alloc: H::RawAlloc,
    _pd: PhantomData<(I, G)>,
}

impl<C, I: Index, G: Grow> ConstDefault for Custom<C, I, G>
where
    C: VecAllocHandle,
    C::RawAlloc: RawAllocDefault,
{
    /// An instance of this custom `Vec` definition, which may be used as an allocation target.
    const DEFAULT: Self = Self {
        alloc: C::RawAlloc::DEFAULT,
        _pd: PhantomData,
    };
}

impl<C: VecAllocHandle, I: Index, G: Grow> VecConfig for Custom<C, I, G> {
    type Buffer<T> = C::AllocHandle<T, I>;
    type Grow = G;
    type Index = I;
}

impl<T, C, I: Index, G: Grow> VecConfigNew<T> for Custom<C, I, G>
where
    C: VecAllocHandle,
    C::AllocHandle<T, I>: ConstDefault,
    C::RawAlloc: Clone,
{
    const EMPTY_BUFFER: Self::Buffer<T> = C::AllocHandle::<T, I>::DEFAULT;

    fn vec_buffer_try_new(
        capacity: Self::Index,
        exact: bool,
    ) -> Result<Self::Buffer<T>, StorageError> {
        let mut buf = Self::EMPTY_BUFFER;
        buf.resize_handle(
            VecHeader {
                capacity,
                length: Self::Index::ZERO,
            },
            exact,
        )?;
        Ok(buf)
    }
}

impl<T, C, I: Index, G: Grow> VecConfigSpawn<T> for Custom<C, I, G>
where
    C: VecAllocHandle,
    C::RawAlloc: Clone,
{
    #[inline]
    fn vec_buffer_try_spawn(
        buf: &Self::Buffer<T>,
        capacity: Self::Index,
        exact: bool,
    ) -> Result<Self::Buffer<T>, StorageError> {
        buf.spawn_handle(
            VecHeader {
                capacity,
                length: Index::ZERO,
            },
            exact,
        )
    }
}

impl<const N: usize> VecConfig for Inline<N> {
    type Buffer<T> = InlineBuffer<T, N>;
    type Index = usize;
    type Grow = GrowExact;
}

impl<T, const N: usize> VecConfigNew<T> for Inline<N> {
    const EMPTY_BUFFER: Self::Buffer<T> = InlineBuffer::<T, N>::DEFAULT;

    fn vec_buffer_try_new(
        capacity: Self::Index,
        exact: bool,
    ) -> Result<Self::Buffer<T>, StorageError> {
        InlineBuffer::try_for_capacity(capacity, exact)
    }
}

impl<T, const N: usize> VecConfigSpawn<T> for Inline<N> {
    #[inline]
    fn vec_buffer_try_spawn(
        _buf: &Self::Buffer<T>,
        capacity: Self::Index,
        exact: bool,
    ) -> Result<Self::Buffer<T>, StorageError> {
        InlineBuffer::try_for_capacity(capacity, exact)
    }
}

impl VecAllocHandle for Thin {
    type RawAlloc = Global;
    type AllocHandle<T, I: Index> = ThinAllocHandle<VecData<T, I>, Global>;
}

/// Support creation of a new `Vec` instance within an allocation target.
pub trait VecNewIn<T> {
    /// The associated `Vec` configuration type.
    type Config: VecConfig;

    /// Try to create a new buffer given an allocation target.
    fn vec_buffer_try_new_in(
        self,
        capacity: <Self::Config as VecConfig>::Index,
        exact: bool,
    ) -> Result<<Self::Config as VecConfig>::Buffer<T>, StorageError>;
}

impl<T, C: RawAllocIn> VecNewIn<T> for C {
    type Config = C::RawAlloc;

    #[inline]
    fn vec_buffer_try_new_in(
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

impl<T, C: VecAllocHandle, I: Index, G: Grow> VecNewIn<T> for Custom<C, I, G> {
    type Config = Self;

    #[inline]
    fn vec_buffer_try_new_in(
        self,
        capacity: <Self::Config as VecConfig>::Index,
        exact: bool,
    ) -> Result<<Self::Config as VecConfig>::Buffer<T>, StorageError> {
        C::AllocHandle::alloc_handle_in(
            self.alloc,
            VecHeader {
                capacity,
                length: Index::ZERO,
            },
            exact,
        )
    }
}

impl<'a, T, const N: usize> VecNewIn<T> for &'a mut ArrayStorage<T, N> {
    type Config = FixedAlloc<'a>;

    #[inline]
    fn vec_buffer_try_new_in(
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

impl<'a, T, A: RawAlloc> VecNewIn<T> for SpillStorage<'a, &'a mut [MaybeUninit<T>], A> {
    type Config = SpillAlloc<'a, A>;

    fn vec_buffer_try_new_in(
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
    fn vec_buffer_try_new_in(
        self,
        capacity: <Self::Config as VecConfig>::Index,
        exact: bool,
    ) -> Result<<Self::Config as VecConfig>::Buffer<T>, StorageError> {
        if capacity > N || (capacity < N && exact) {
            return Err(StorageError::CapacityLimit);
        }
        Ok(InlineBuffer::DEFAULT)
    }
}

impl<T> VecNewIn<T> for Thin {
    type Config = Thin;

    #[inline]
    fn vec_buffer_try_new_in(
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
