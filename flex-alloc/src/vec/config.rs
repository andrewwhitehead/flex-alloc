//! `Vec` configuration types and trait definitions.

use core::fmt::Debug;
use core::marker::PhantomData;
use core::mem::MaybeUninit;
use core::ptr::{self, NonNull};

use const_default::ConstDefault;

use crate::alloc::{AllocateIn, Allocator, AllocatorDefault, Fixed, Global, Spill};
use crate::capacity::{Grow, GrowDoubling, GrowExact, Index};
use crate::error::StorageError;
use crate::storage::{ArrayStorage, FatBuffer, Inline, InlineBuffer, SpillStorage, ThinBuffer};

use super::buffer::{VecBuffer, VecHeader};

/// Define the associated types for `Vec` instances.
pub trait VecConfig {
    /// The internal buffer type.
    type Buffer<T>: VecBuffer<Item = T, Index = Self::Index>;

    /// The growth strategy.
    type Grow: Grow;

    /// The index type used to define the capacity and length.
    type Index: Index;
}

impl<A: Allocator> VecConfig for A {
    type Buffer<T> = FatBuffer<T, VecHeader<usize>, A>;
    type Grow = GrowDoubling;
    type Index = usize;
}

/// Configuration for `Vec` types supporting an allocator.
pub trait VecConfigAlloc<T>: VecConfig {
    /// The allocator instance type.
    type Alloc: Allocator;

    /// Get a reference to the allocator instance.
    fn allocator(buf: &Self::Buffer<T>) -> &Self::Alloc;

    /// Create a `Vec` buffer instance from its constituent parts.
    fn buffer_from_parts(
        data: NonNull<T>,
        length: Self::Index,
        capacity: Self::Index,
        alloc: Self::Alloc,
    ) -> Self::Buffer<T>;

    /// Disassemble a `Vec` buffer instance into its constituent parts.
    fn buffer_into_parts(
        buffer: Self::Buffer<T>,
    ) -> (NonNull<T>, Self::Index, Self::Index, Self::Alloc);
}

impl<T, A: Allocator> VecConfigAlloc<T> for A {
    type Alloc = A;

    #[inline]
    fn allocator(buf: &Self::Buffer<T>) -> &Self::Alloc {
        &buf.alloc
    }

    #[inline]
    fn buffer_from_parts(
        data: NonNull<T>,
        length: Self::Index,
        capacity: Self::Index,
        alloc: Self::Alloc,
    ) -> Self::Buffer<T> {
        FatBuffer::from_parts(VecHeader { capacity, length }, data, alloc)
    }

    #[inline]
    fn buffer_into_parts(
        buffer: Self::Buffer<T>,
    ) -> (NonNull<T>, Self::Index, Self::Index, Self::Alloc) {
        let (header, data, alloc) = buffer.into_parts();
        (data, header.length, header.capacity, alloc)
    }
}

/// Support creation of new `Vec` instances without a storage reference.
pub trait VecConfigNew<T>: VecConfigSpawn<T> {
    /// Constant initializer for an empty buffer.
    const EMPTY_BUFFER: Self::Buffer<T>;

    /// Try to create a new buffer instance with a given capacity.
    fn buffer_try_new(capacity: Self::Index, exact: bool) -> Result<Self::Buffer<T>, StorageError>;
}

impl<T, A: AllocatorDefault> VecConfigNew<T> for A {
    const EMPTY_BUFFER: Self::Buffer<T> = FatBuffer::<T, VecHeader<usize>, A>::DEFAULT;

    #[inline]
    fn buffer_try_new(capacity: Self::Index, exact: bool) -> Result<Self::Buffer<T>, StorageError> {
        FatBuffer::allocate_in(
            VecHeader {
                capacity,
                length: Self::Index::ZERO,
            },
            A::DEFAULT,
            exact,
        )
    }
}

/// Support creation of new `Vec` buffer instances from an existing instance.
pub trait VecConfigSpawn<T>: VecConfig {
    /// Try to create a new buffer instance with a given capacity.
    fn buffer_try_spawn(
        buf: &Self::Buffer<T>,
        capacity: Self::Index,
        exact: bool,
    ) -> Result<Self::Buffer<T>, StorageError>;
}

impl<T, A: Allocator + Clone> VecConfigSpawn<T> for A {
    #[inline]
    fn buffer_try_spawn(
        buf: &Self::Buffer<T>,
        capacity: Self::Index,
        exact: bool,
    ) -> Result<Self::Buffer<T>, StorageError> {
        FatBuffer::allocate_in(
            VecHeader {
                capacity,
                length: Index::ZERO,
            },
            buf.alloc.clone(),
            exact,
        )
    }
}

/// Support creation of a new `Vec` instance within an allocation target.
pub trait VecNewIn<T> {
    /// The associated `Vec` configuration type.
    type Config: VecConfig;

    /// Try to create a new buffer given an allocation target.
    fn buffer_try_new_in(
        self,
        capacity: <Self::Config as VecConfig>::Index,
        exact: bool,
    ) -> Result<<Self::Config as VecConfig>::Buffer<T>, StorageError>;
}

/// Parameterize `Vec` with a custom index type or growth behavior.
#[derive(Debug, Default)]
pub struct Custom<A: Allocator, I: Index = usize, G: Grow = GrowExact> {
    alloc: A,
    _pd: PhantomData<(I, G)>,
}

impl<A: AllocatorDefault, I: Index, G: Grow> ConstDefault for Custom<A, I, G> {
    /// An instance of this custom `Vec` definition, which may be used as an allocation target.
    const DEFAULT: Self = Self {
        alloc: A::DEFAULT,
        _pd: PhantomData,
    };
}

impl<A: Allocator, I: Index, G: Grow> VecConfig for Custom<A, I, G> {
    type Buffer<T> = FatBuffer<T, VecHeader<I>, A>;
    type Grow = G;
    type Index = I;
}

impl<T, A: AllocatorDefault, I: Index, G: Grow> VecConfigNew<T> for Custom<A, I, G> {
    const EMPTY_BUFFER: Self::Buffer<T> = FatBuffer::DEFAULT;

    fn buffer_try_new(capacity: Self::Index, exact: bool) -> Result<Self::Buffer<T>, StorageError> {
        FatBuffer::allocate_in(
            VecHeader {
                capacity,
                length: Self::Index::ZERO,
            },
            A::DEFAULT,
            exact,
        )
    }
}

impl<T, A: Allocator + Clone, I: Index, G: Grow> VecConfigSpawn<T> for Custom<A, I, G> {
    #[inline]
    fn buffer_try_spawn(
        buf: &Self::Buffer<T>,
        capacity: Self::Index,
        exact: bool,
    ) -> Result<Self::Buffer<T>, StorageError> {
        FatBuffer::allocate_in(
            VecHeader {
                capacity,
                length: Index::ZERO,
            },
            buf.alloc.clone(),
            exact,
        )
    }
}

/// Parameterize `Vec` with a custom index type or growth behavior.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Thin<A: Allocator = Global, I: Index = usize, G: Grow = GrowExact> {
    alloc: A,
    _pd: PhantomData<(I, G)>,
}

impl<A: Allocator, I: Index, G: Grow> VecConfig for Thin<A, I, G> {
    type Buffer<T> = ThinBuffer<T, VecHeader<usize>, A>;
    type Grow = GrowDoubling;
    type Index = usize;
}

impl<A: AllocatorDefault, I: Index, G: Grow> ConstDefault for Thin<A, I, G> {
    const DEFAULT: Self = Self {
        alloc: A::DEFAULT,
        _pd: PhantomData,
    };
}

impl<T, A: Allocator, I: Index, G: Grow> VecConfigAlloc<T> for Thin<A, I, G> {
    type Alloc = A;

    #[inline]
    fn allocator(buf: &Self::Buffer<T>) -> &Self::Alloc {
        &buf.alloc
    }

    #[inline]
    fn buffer_from_parts(
        data: NonNull<T>,
        length: Self::Index,
        capacity: Self::Index,
        alloc: Self::Alloc,
    ) -> Self::Buffer<T> {
        ThinBuffer::from_parts(VecHeader { capacity, length }, data, alloc)
    }

    #[inline]
    fn buffer_into_parts(
        buffer: Self::Buffer<T>,
    ) -> (NonNull<T>, Self::Index, Self::Index, Self::Alloc) {
        let (header, data, alloc) = buffer.into_parts();
        (data, header.length, header.capacity, alloc)
    }
}

impl<T, A: AllocatorDefault, I: Index, G: Grow> VecConfigNew<T> for Thin<A, I, G> {
    const EMPTY_BUFFER: Self::Buffer<T> = ThinBuffer::<T, VecHeader<usize>, A>::DEFAULT;

    #[inline]
    fn buffer_try_new(capacity: Self::Index, exact: bool) -> Result<Self::Buffer<T>, StorageError> {
        ThinBuffer::allocate_in(
            VecHeader {
                capacity,
                length: Self::Index::ZERO,
            },
            A::DEFAULT,
            exact,
        )
    }
}

impl<T, A: Allocator + Clone, I: Index, G: Grow> VecConfigSpawn<T> for Thin<A, I, G> {
    #[inline]
    fn buffer_try_spawn(
        buf: &Self::Buffer<T>,
        capacity: Self::Index,
        exact: bool,
    ) -> Result<Self::Buffer<T>, StorageError> {
        ThinBuffer::allocate_in(
            VecHeader {
                capacity,
                length: Index::ZERO,
            },
            buf.alloc.clone(),
            exact,
        )
    }
}

impl<T, A: AllocatorDefault, I: Index, G: Grow> VecNewIn<T> for Thin<A, I, G> {
    type Config = Thin<A, I, G>;

    #[inline]
    fn buffer_try_new_in(
        self,
        capacity: <Self::Config as VecConfig>::Index,
        exact: bool,
    ) -> Result<<Self::Config as VecConfig>::Buffer<T>, StorageError> {
        ThinBuffer::allocate_in(
            VecHeader {
                capacity,
                length: Index::ZERO,
            },
            A::DEFAULT,
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

    fn buffer_try_new(capacity: Self::Index, exact: bool) -> Result<Self::Buffer<T>, StorageError> {
        InlineBuffer::try_for_capacity(capacity, exact)
    }
}

impl<T, const N: usize> VecConfigSpawn<T> for Inline<N> {
    #[inline]
    fn buffer_try_spawn(
        _buf: &Self::Buffer<T>,
        capacity: Self::Index,
        exact: bool,
    ) -> Result<Self::Buffer<T>, StorageError> {
        InlineBuffer::try_for_capacity(capacity, exact)
    }
}

impl<T, C: AllocateIn> VecNewIn<T> for C {
    type Config = C::Alloc;

    #[inline]
    fn buffer_try_new_in(
        self,
        capacity: <Self::Config as VecConfig>::Index,
        exact: bool,
    ) -> Result<<Self::Config as VecConfig>::Buffer<T>, StorageError> {
        FatBuffer::allocate_in(
            VecHeader {
                capacity,
                length: Index::ZERO,
            },
            self,
            exact,
        )
    }
}

impl<T, A: Allocator, I: Index, G: Grow> VecNewIn<T> for Custom<A, I, G> {
    type Config = Self;

    #[inline]
    fn buffer_try_new_in(
        self,
        capacity: <Self::Config as VecConfig>::Index,
        exact: bool,
    ) -> Result<<Self::Config as VecConfig>::Buffer<T>, StorageError> {
        FatBuffer::allocate_in(
            VecHeader {
                capacity,
                length: Index::ZERO,
            },
            self.alloc,
            exact,
        )
    }
}

impl<'a, T, const N: usize> VecNewIn<T> for &'a mut ArrayStorage<T, N> {
    type Config = Fixed<'a>;

    #[inline]
    fn buffer_try_new_in(
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
        Ok(FatBuffer::from_parts(
            VecHeader {
                capacity,
                length: Index::ZERO,
            },
            unsafe { NonNull::new_unchecked(self.0.as_mut_ptr()) }.cast(),
            Fixed::default(),
        ))
    }
}

impl<'a, T, A: Allocator> VecNewIn<T> for SpillStorage<'a, &'a mut [MaybeUninit<T>], A> {
    type Config = Spill<'a, A>;

    fn buffer_try_new_in(
        self,
        mut capacity: <Self::Config as VecConfig>::Index,
        exact: bool,
    ) -> Result<<Self::Config as VecConfig>::Buffer<T>, StorageError> {
        if capacity > self.buffer.len() {
            return FatBuffer::allocate_in(
                VecHeader {
                    capacity,
                    length: Index::ZERO,
                },
                Spill::new(self.alloc, ptr::null(), Fixed::DEFAULT),
                exact,
            );
        }
        if !exact {
            capacity = self.buffer.len();
        }
        let ptr = self.buffer.as_mut_ptr();
        Ok(FatBuffer::from_parts(
            VecHeader {
                capacity,
                length: Index::ZERO,
            },
            unsafe { NonNull::new_unchecked(ptr) }.cast(),
            Spill::new(self.alloc, ptr.cast(), Fixed::DEFAULT),
        ))
    }
}

impl<T, const N: usize> VecNewIn<T> for Inline<N> {
    type Config = Inline<N>;

    #[inline]
    fn buffer_try_new_in(
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
