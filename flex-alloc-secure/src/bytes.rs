use core::mem::{size_of_val, MaybeUninit};
use core::slice;

use flex_alloc::alloc::Allocator;
use flex_alloc::boxed::Box;
use flex_alloc::vec::{config::VecConfig, Vec};
use rand_core::RngCore;
use zeroize::Zeroize;

/// Access a value as a mutable slice of bytes.
///
/// # Safety
/// This trait must only be implemented for types which can accept any
/// pattern of bits as a legitimate value.
pub unsafe trait FillBytes: Zeroize {
    /// Access the value as a mutable slice of bytes.
    fn as_bytes_mut(&mut self) -> &mut [u8] {
        let len: usize = size_of_val(self);
        unsafe { slice::from_raw_parts_mut(self as *mut Self as *mut u8, len) }
    }

    /// Fill the value with repeated bytes.
    fn fill_bytes(&mut self, value: u8) {
        self.as_bytes_mut().fill(value);
    }

    /// Fill the value with random bytes.
    fn fill_random(&mut self, mut rng: impl RngCore) {
        rng.fill_bytes(self.as_bytes_mut());
    }

    /// Fill the value using a closure.
    fn fill_with<R>(&mut self, f: impl FnOnce(&mut [u8]) -> R) -> R {
        f(self.as_bytes_mut())
    }
}

unsafe impl FillBytes for u8 {}
unsafe impl FillBytes for u16 {}
unsafe impl FillBytes for u32 {}
unsafe impl FillBytes for u64 {}
unsafe impl FillBytes for u128 {}
unsafe impl FillBytes for usize {}

unsafe impl<T: Copy + FillBytes> FillBytes for MaybeUninit<T> {}
unsafe impl<T: Copy + FillBytes, const N: usize> FillBytes for [T; N] {}
unsafe impl<T: Copy + FillBytes> FillBytes for [T] where [T]: Zeroize {}

unsafe impl<T: FillBytes + ?Sized, A: Allocator> FillBytes for Box<T, A> {
    fn as_bytes_mut(&mut self) -> &mut [u8] {
        let len: usize = size_of_val(self.as_ref());
        unsafe { slice::from_raw_parts_mut(self.as_mut_ptr().cast::<u8>(), len) }
    }
}

unsafe impl<T: FillBytes, C: VecConfig> FillBytes for Vec<T, C> {
    fn as_bytes_mut(&mut self) -> &mut [u8] {
        let len: usize = size_of_val(self.as_ref());
        unsafe { slice::from_raw_parts_mut(self.as_mut_ptr().cast::<u8>(), len) }
    }
}
