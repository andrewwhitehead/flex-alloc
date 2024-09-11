//! Backing storage types for collections.

use core::fmt;
use core::mem::{ManuallyDrop, MaybeUninit};

pub(crate) mod alloc;

pub(crate) mod utils;

#[cfg(feature = "zeroize")]
pub(crate) mod zero;

pub use self::alloc::{
    FixedAlloc, Global, RawAlloc, RawAllocIn, RawAllocNew, SpillAlloc, SpillStorage, Thin,
};

#[cfg(feature = "zeroize")]
pub use self::zero::ZeroizingAlloc;

/// Provide access to the associated data for abstract buffer types.
pub trait RawBuffer: Sized {
    /// The concrete data type.
    type RawData: ?Sized;

    /// Access the data as a readonly pointer.
    fn data_ptr(&self) -> *const Self::RawData;

    /// Access the data as a mutable pointer.
    fn data_ptr_mut(&mut self) -> *mut Self::RawData;
}

/// Attach an allocator to a fixed allocation buffer. Once the initial
/// buffer is exhausted, additional buffer(s) may be requested from the
/// new allocator instance.
pub trait WithAlloc<'a>: Sized {
    /// The concrete type of resulting allocation target.
    type NewIn<A: 'a>;

    /// Consume the allocator instance, returning a new allocator
    /// which spills into the Global allocator.
    #[inline]
    fn with_alloc(self) -> Self::NewIn<Global> {
        Self::with_alloc_in(self, Global)
    }

    /// Consume the allocator instance, returning a new allocator
    /// which spills into the provided allocator instance `alloc`.
    fn with_alloc_in<A: RawAlloc + 'a>(self, alloc: A) -> Self::NewIn<A>;
}

/// A storage buffer consisting of an uninitialized `MaybeUnit` array.
#[repr(transparent)]
pub struct ArrayStorage<T, const N: usize>(pub [MaybeUninit<T>; N]);

impl<T, const N: usize> ArrayStorage<T, N> {
    /// Constant initializer.
    pub const DEFAULT: Self = Self(unsafe { MaybeUninit::uninit().assume_init() });

    /// Access the buffer contents as a mutable slice.
    pub fn as_uninit_slice(&mut self) -> &mut [MaybeUninit<T>] {
        &mut self.0
    }
}

impl<T, const N: usize> fmt::Debug for ArrayStorage<T, N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ArrayStorage").finish_non_exhaustive()
    }
}

impl<T, const N: usize> Default for ArrayStorage<T, N> {
    #[inline]
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl<'a, T: 'a, const N: usize> WithAlloc<'a> for &'a mut ArrayStorage<T, N> {
    type NewIn<A: 'a> = SpillStorage<'a, &'a mut [MaybeUninit<T>], A>;

    #[inline]
    fn with_alloc_in<A: RawAlloc + 'a>(self, alloc: A) -> Self::NewIn<A> {
        SpillStorage::new_in(&mut self.0, alloc)
    }
}

/// A reusable storage buffer consisting of an array of bytes.
#[repr(C)]
pub union ByteStorage<T, const N: usize> {
    _align: [ManuallyDrop<T>; 0],
    data: [MaybeUninit<u8>; N],
}

impl<T, const N: usize> ByteStorage<T, N> {
    /// Constant initializer.
    pub const DEFAULT: Self = Self {
        data: unsafe { MaybeUninit::uninit().assume_init() },
    };

    /// Access the buffer contents as a mutable slice.
    pub fn as_uninit_slice(&mut self) -> &mut [MaybeUninit<u8>] {
        unsafe { &mut self.data }
    }
}

impl<T, const N: usize> fmt::Debug for ByteStorage<T, N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ByteStorage").finish_non_exhaustive()
    }
}

impl<T, const N: usize> Default for ByteStorage<T, N> {
    #[inline]
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl<'a, T: 'static, const N: usize> WithAlloc<'a> for &'a mut ByteStorage<T, N> {
    type NewIn<A: 'a> = SpillStorage<'a, Self, A>;

    #[inline]
    fn with_alloc_in<A: RawAlloc + 'a>(self, alloc: A) -> Self::NewIn<A> {
        SpillStorage::new_in(self, alloc)
    }
}

/// Create a new array storage buffer for type `T` and maximum capacity `N`.
pub const fn array_storage<T, const N: usize>() -> ArrayStorage<T, N> {
    ArrayStorage::DEFAULT
}

/// Create a new byte storage buffer for a maximum byte capacity `N`.
pub const fn byte_storage<const N: usize>() -> ByteStorage<u8, N> {
    ByteStorage::DEFAULT
}

/// Create a new byte storage buffer for a maximum byte capacity `N`, with
/// a memory alignment matching type `T`.
pub const fn aligned_byte_storage<T, const N: usize>() -> ByteStorage<T, N> {
    ByteStorage::DEFAULT
}

/// A marker type used to indicate the inline allocation strategy, which
/// stores all items within the collection handle.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Inline<const N: usize>;

/// An inline storage buffer.
#[derive(Debug)]
pub struct InlineBuffer<T, const N: usize> {
    pub(crate) storage: ArrayStorage<T, N>,
    pub(crate) length: usize,
}

impl<T, const N: usize> InlineBuffer<T, N> {
    /// Constant initializer.
    pub const DEFAULT: Self = Self {
        storage: ArrayStorage::DEFAULT,
        length: 0,
    };
}

impl<T, const N: usize> Default for InlineBuffer<T, N> {
    #[inline]
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl<'a, T: 'a, const N: usize> RawBuffer for InlineBuffer<T, N> {
    type RawData = T;

    #[inline]
    fn data_ptr(&self) -> *const T {
        self.storage.0.as_ptr().cast()
    }

    #[inline]
    fn data_ptr_mut(&mut self) -> *mut T {
        self.storage.0.as_mut_ptr().cast()
    }
}
