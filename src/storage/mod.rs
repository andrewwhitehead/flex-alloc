//! Backing storage types for collections.

use core::fmt;
use core::mem::{ManuallyDrop, MaybeUninit};

pub(crate) mod alloc;

pub(crate) mod utils;

#[cfg(feature = "zeroize")]
pub(crate) mod zero;

pub use self::alloc::{FixedAlloc, Global, RawAlloc, RawAllocIn, RawAllocNew, SpillStorage, Thin};

#[cfg(feature = "zeroize")]
pub use self::zero::ZeroizingAlloc;

pub trait RawBuffer: Sized {
    type RawData: ?Sized;

    fn data_ptr(&self) -> *const Self::RawData;

    fn data_ptr_mut(&mut self) -> *mut Self::RawData;
}

pub trait WithAlloc<'a>: Sized {
    type NewIn<A: 'a>;

    #[inline]
    fn with_alloc(self) -> Self::NewIn<Global> {
        Self::with_alloc_in(self, Global)
    }

    fn with_alloc_in<A: RawAlloc + 'a>(self, alloc: A) -> Self::NewIn<A>;
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ArrayStorage<A>(pub A);

impl<A> ArrayStorage<A> {
    pub const fn new(inner: A) -> Self {
        Self(inner)
    }
}

impl<T, const N: usize> ArrayStorage<[MaybeUninit<T>; N]> {
    pub fn as_uninit_slice(&mut self) -> &mut [MaybeUninit<T>] {
        &mut self.0
    }
}

#[repr(C)]
pub union ByteStorage<T, const N: usize> {
    _align: [ManuallyDrop<T>; 0],
    data: [MaybeUninit<u8>; N],
}

impl<T, const N: usize> ByteStorage<T, N> {
    pub const fn new() -> Self {
        Self {
            data: unsafe { MaybeUninit::uninit().assume_init() },
        }
    }

    pub fn as_uninit_slice(&mut self) -> &mut [MaybeUninit<u8>] {
        unsafe { &mut self.data }
    }
}

impl<'a, T: 'static, const N: usize> WithAlloc<'a> for &'a mut ByteStorage<T, N> {
    type NewIn<A: 'a> = SpillStorage<'a, Self, A>;

    #[inline]
    fn with_alloc_in<A: RawAlloc + 'a>(self, alloc: A) -> Self::NewIn<A> {
        SpillStorage::new_in(self, alloc)
    }
}

impl<T, const N: usize> fmt::Debug for ByteStorage<T, N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ByteStorage").finish_non_exhaustive()
    }
}

pub const fn array_storage<T, const N: usize>() -> ArrayStorage<[MaybeUninit<T>; N]> {
    ArrayStorage::new(unsafe { MaybeUninit::uninit().assume_init() })
}

pub const fn byte_storage<const N: usize>() -> ByteStorage<u8, N> {
    ByteStorage::<u8, N>::new()
}

pub const fn aligned_byte_storage<T, const N: usize>() -> ByteStorage<T, N> {
    ByteStorage::<T, N>::new()
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Inline<const N: usize>;

#[derive(Debug)]
pub struct InlineBuffer<T, const N: usize> {
    pub data: [MaybeUninit<T>; N],
    pub length: usize,
}

impl<T, const N: usize> InlineBuffer<T, N> {
    pub const fn new() -> Self {
        Self {
            data: unsafe { MaybeUninit::uninit().assume_init() },
            length: 0,
        }
    }
}

impl<T, const N: usize> Default for InlineBuffer<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, T: 'a, const N: usize> RawBuffer for InlineBuffer<T, N> {
    type RawData = T;

    #[inline]
    fn data_ptr(&self) -> *const T {
        self.data.as_ptr().cast()
    }

    #[inline]
    fn data_ptr_mut(&mut self) -> *mut T {
        self.data.as_mut_ptr().cast()
    }
}
