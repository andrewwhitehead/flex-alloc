use core::mem::{size_of_val, MaybeUninit};
use core::slice;

use flex_alloc::alloc::Allocator;
use flex_alloc::boxed::Box;
use flex_alloc::vec::{config::VecConfig, Vec};
use rand_core::RngCore;

/// Access a value as a mutable slice of bytes.
///
/// # Safety
/// This trait must only be implemented for types which can accept any
/// pattern of bits as a legitimate value.
pub unsafe trait FillBytes {
    /// Access the value as a mutable slice of bytes.
    #[inline(always)]
    fn as_bytes_mut(&mut self) -> &mut [u8] {
        let len: usize = size_of_val(self);
        unsafe { slice::from_raw_parts_mut(self as *mut Self as *mut u8, len) }
    }

    /// Fill the value with repeated bytes.
    #[inline(always)]
    fn fill_bytes(&mut self, value: u8) {
        self.as_bytes_mut().fill(value);
        self.canonicalize_bytes();
    }

    /// Fill the value with random bytes.
    #[inline(always)]
    fn fill_random(&mut self, mut rng: impl RngCore) {
        rng.fill_bytes(self.as_bytes_mut());
        self.canonicalize_bytes();
    }

    /// Fill the value using a closure.
    #[inline(always)]
    fn fill_with<R>(&mut self, f: impl FnOnce(&mut [u8]) -> R) -> R {
        f(self.as_bytes_mut())
    }

    /// After filling the bytes, try to ensure consistency across platforms.
    fn canonicalize_bytes(&mut self) {}
}

macro_rules! impl_fill_bytes_int {
    ($name:ty) => {
        unsafe impl FillBytes for $name {
            #[inline(always)]
            fn canonicalize_bytes(&mut self) {
                *self = self.to_le();
            }
        }

        unsafe impl FillBytes for Option<::core::num::NonZero<$name>> {
            #[inline(always)]
            fn canonicalize_bytes(&mut self) {
                *self = ::core::num::NonZero::new(
                    <$name>::from_ne_bytes(self.as_bytes_mut().try_into().unwrap()).to_le()
                );
            }
        }

        unsafe impl FillBytes for ::core::num::Saturating<$name> {
            #[inline(always)]
            fn canonicalize_bytes(&mut self) {
                *self = ::core::num::Saturating(self.0.to_le());
            }
        }

        unsafe impl FillBytes for ::core::num::Wrapping<$name> {
            #[inline(always)]
            fn canonicalize_bytes(&mut self) {
                *self = ::core::num::Wrapping(self.0.to_le());
            }
        }
    };
    ($name:ty, $($rest:ty),+) => {
        impl_fill_bytes_int!($name);
        impl_fill_bytes_int!($($rest),+);
    };
}

impl_fill_bytes_int!(u8, u16, u32, u64, u128, usize);
impl_fill_bytes_int!(i8, i16, i32, i64, i128, isize);

unsafe impl<T: Copy + FillBytes> FillBytes for MaybeUninit<T> {
    #[inline(always)]
    fn canonicalize_bytes(&mut self) {
        unsafe { self.assume_init_mut() }.canonicalize_bytes();
    }
}

unsafe impl<T: Copy + FillBytes, const N: usize> FillBytes for [T; N] {
    #[inline(always)]
    fn canonicalize_bytes(&mut self) {
        for item in self.iter_mut() {
            item.canonicalize_bytes();
        }
    }
}

unsafe impl<T: Copy + FillBytes> FillBytes for [T] {
    #[inline(always)]
    fn canonicalize_bytes(&mut self) {
        for item in self.iter_mut() {
            item.canonicalize_bytes();
        }
    }
}

unsafe impl<T: FillBytes + ?Sized, A: Allocator> FillBytes for Box<T, A> {
    #[inline(always)]
    fn as_bytes_mut(&mut self) -> &mut [u8] {
        let len: usize = size_of_val(self.as_ref());
        unsafe { slice::from_raw_parts_mut(self.as_mut_ptr().cast::<u8>(), len) }
    }

    #[inline(always)]
    fn canonicalize_bytes(&mut self) {
        self.as_mut().canonicalize_bytes();
    }
}

unsafe impl<T: FillBytes, C: VecConfig> FillBytes for Vec<T, C> {
    #[inline(always)]
    fn as_bytes_mut(&mut self) -> &mut [u8] {
        let len: usize = size_of_val(self.as_ref());
        unsafe { slice::from_raw_parts_mut(self.as_mut_ptr().cast::<u8>(), len) }
    }

    #[inline(always)]
    fn canonicalize_bytes(&mut self) {
        for item in self.iter_mut() {
            item.canonicalize_bytes();
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{alloc::UNINIT_ALLOC_BYTE, FillBytes};
    use core::num::NonZero;

    #[test]
    fn nonzero_check() {
        let mut a: Option<NonZero<u64>> = None;
        a.fill_bytes(UNINIT_ALLOC_BYTE);
        assert_eq!(
            a.as_ref().copied(),
            NonZero::new(u64::from_ne_bytes([UNINIT_ALLOC_BYTE; 8]).to_le())
        );

        let mut b = [Option::<NonZero<u64>>::None; 10];
        b.fill_bytes(UNINIT_ALLOC_BYTE);
        assert_eq!(
            b[0].as_ref().copied(),
            NonZero::new(u64::from_ne_bytes([UNINIT_ALLOC_BYTE; 8]).to_le())
        );
        b[..2].fill_bytes(UNINIT_ALLOC_BYTE);
    }
}
