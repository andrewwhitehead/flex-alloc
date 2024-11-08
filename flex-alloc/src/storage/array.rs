use core::fmt;
use core::mem::MaybeUninit;

use const_default::ConstDefault;

use super::spill::SpillStorage;
use crate::alloc::{Allocator, SpillAlloc};

/// A storage buffer consisting of an uninitialized `MaybeUnit` array.
#[repr(transparent)]
pub struct ArrayStorage<T, const N: usize>(pub [MaybeUninit<T>; N]);

impl<T, const N: usize> ArrayStorage<T, N> {
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

impl<T, const N: usize> ConstDefault for ArrayStorage<T, N> {
    const DEFAULT: Self = Self(unsafe { MaybeUninit::uninit().assume_init() });
}

impl<T, const N: usize> Default for ArrayStorage<T, N> {
    #[inline]
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl<'a, T: 'a, const N: usize> SpillAlloc<'a> for &'a mut ArrayStorage<T, N> {
    type NewIn<A: 'a> = SpillStorage<'a, &'a mut [MaybeUninit<T>], A>;

    #[inline]
    fn spill_alloc_in<A: Allocator + 'a>(self, alloc: A) -> Self::NewIn<A> {
        SpillStorage::new_in(&mut self.0, alloc)
    }
}

#[cfg(feature = "zeroize")]
impl<T, const N: usize> zeroize::Zeroize for ArrayStorage<T, N> {
    #[inline]
    fn zeroize(&mut self) {
        self.0.zeroize()
    }
}
