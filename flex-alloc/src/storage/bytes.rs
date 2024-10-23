use core::alloc::Layout;
use core::fmt;
use core::mem::{ManuallyDrop, MaybeUninit};
use core::ptr::NonNull;

use const_default::ConstDefault;

use super::{spill::SpillStorage, utils::layout_aligned_bytes};
use crate::alloc::{AllocError, AllocateIn, Allocator, FixedAlloc, WithAlloc};

/// A reusable storage buffer consisting of an array of bytes.
#[repr(C)]
pub union ByteStorage<T, const N: usize> {
    _align: [ManuallyDrop<T>; 0],
    data: [MaybeUninit<u8>; N],
}

impl<T, const N: usize> ByteStorage<T, N> {
    /// Access the buffer contents as a mutable slice.
    pub fn as_uninit_slice(&mut self) -> &mut [MaybeUninit<u8>] {
        unsafe { &mut self.data }
    }
}

impl<'a, T, const N: usize> AllocateIn for &'a mut ByteStorage<T, N> {
    type Alloc = FixedAlloc<'a>;

    #[inline]
    fn allocate_in(self, layout: Layout) -> Result<(NonNull<[u8]>, Self::Alloc), AllocError> {
        let ptr = layout_aligned_bytes(self.as_uninit_slice(), layout).map_err(|_| AllocError)?;
        let alloc = FixedAlloc::default();
        Ok((ptr, alloc))
    }
}

impl<T, const N: usize> ConstDefault for ByteStorage<T, N> {
    const DEFAULT: Self = Self {
        data: unsafe { MaybeUninit::uninit().assume_init() },
    };
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
    fn with_alloc_in<A: Allocator + 'a>(self, alloc: A) -> Self::NewIn<A> {
        SpillStorage::new_in(self, alloc)
    }
}

#[cfg(feature = "zeroize")]
impl<T, const N: usize> zeroize::Zeroize for ByteStorage<T, N> {
    #[inline]
    fn zeroize(&mut self) {
        self.as_uninit_slice().zeroize()
    }
}
