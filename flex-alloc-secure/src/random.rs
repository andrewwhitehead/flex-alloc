use core::mem::{size_of_val, MaybeUninit};
use core::slice;

use flex_alloc::alloc::Allocator;
use flex_alloc::boxed::Box;
use flex_alloc::vec::{config::VecConfig, Vec};
use rand_core::RngCore;

use crate::boxed::SecureBox;
use crate::vec::SecureVec;

/// Access a value as a mutable slice of bytes.
///
/// # Safety
/// This trait must only be implemented for types which can accept any
/// pattern of bits as a legitimate value.
pub unsafe trait FillBytes {
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

    /// Fill the value using a coroutine.
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

unsafe impl<T> FillBytes for MaybeUninit<T> {}
unsafe impl<T: FillBytes, const N: usize> FillBytes for [T; N] {}
unsafe impl<T: FillBytes> FillBytes for [T] {}

pub trait SecureInit: Sized {
    type Item;
    type Uninit: AsMut<MaybeUninit<Self::Item>>;

    fn new_uninit() -> Self::Uninit;
    unsafe fn assume_init(uninit: Self::Uninit) -> Self;

    fn new_with_bytes(f: impl FnOnce(&mut [u8])) -> Self
    where
        Self::Uninit: FillBytes,
    {
        let mut slf = Self::new_uninit();
        slf.fill_with(f);
        unsafe { Self::assume_init(slf) }
    }

    fn try_new_with_bytes<E>(f: impl FnOnce(&mut [u8]) -> Result<(), E>) -> Result<Self, E>
    where
        Self::Uninit: FillBytes,
    {
        let mut slf = Self::new_uninit();
        slf.fill_with(f)?;
        Ok(unsafe { Self::assume_init(slf) })
    }

    /// Generate a new random value.
    fn random(rng: impl RngCore) -> Self
    where
        Self::Uninit: FillBytes,
    {
        let mut slf = Self::new_uninit();
        slf.fill_random(rng);
        unsafe { Self::assume_init(slf) }
    }
}

impl<T> SecureInit for SecureBox<T> {
    type Item = T;
    type Uninit = SecureBox<MaybeUninit<T>>;

    fn new_uninit() -> Self::Uninit {
        SecureBox::new_uninit()
    }

    unsafe fn assume_init(uninit: Self::Uninit) -> Self {
        uninit.assume_init()
    }
}

pub trait SecureInitLen: Sized {
    type Item;
    type Index;
    type Uninit: AsMut<[MaybeUninit<Self::Item>]>;

    fn new_uninit_len(len: Self::Index) -> Self::Uninit;
    unsafe fn assume_init(uninit: Self::Uninit) -> Self;

    fn new_len_with_bytes(len: Self::Index, f: impl FnOnce(&mut [u8])) -> Self
    where
        Self::Uninit: FillBytes,
    {
        let mut slf = Self::new_uninit_len(len);
        slf.fill_with(f);
        unsafe { Self::assume_init(slf) }
    }

    fn try_new_len_with_bytes<E>(
        len: Self::Index,
        f: impl FnOnce(&mut [u8]) -> Result<(), E>,
    ) -> Result<Self, E>
    where
        Self::Uninit: FillBytes,
    {
        let mut slf = Self::new_uninit_len(len);
        slf.fill_with(f)?;
        Ok(unsafe { Self::assume_init(slf) })
    }

    /// Generate a new random value of length `len`.
    fn random_len(rng: impl RngCore, len: Self::Index) -> Self
    where
        Self::Uninit: FillBytes,
    {
        let mut slf = Self::new_uninit_len(len);
        slf.fill_random(rng);
        unsafe { Self::assume_init(slf) }
    }
}

impl<T> SecureInitLen for SecureBox<[T]> {
    type Item = T;
    type Index = usize;
    type Uninit = SecureBox<[MaybeUninit<T>]>;

    fn new_uninit_len(len: Self::Index) -> Self::Uninit {
        SecureBox::new_uninit_slice(len)
    }

    unsafe fn assume_init(uninit: Self::Uninit) -> Self {
        uninit.assume_init()
    }
}

impl<T> SecureInitLen for SecureVec<T> {
    type Item = T;
    type Index = usize;
    type Uninit = SecureBox<[MaybeUninit<T>]>;

    fn new_uninit_len(len: Self::Index) -> Self::Uninit {
        SecureBox::new_uninit_slice(len)
    }

    unsafe fn assume_init(uninit: Self::Uninit) -> Self {
        uninit.assume_init().into_vec()
    }
}

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

#[cfg(test)]
mod tests {
    use crate::{boxed::SecureBox, vec::SecureVec};

    use super::{SecureInit, SecureInitLen};

    #[test]
    fn rand_box() {
        use rand_core::OsRng;
        let boxed = SecureBox::<[usize; 10]>::random(&mut OsRng);
        assert!(boxed[0] != 0);

        let boxed_slice = SecureBox::<[usize]>::random_len(&mut OsRng, 10);
        assert!(boxed_slice[0] != 0);

        let vec = SecureVec::<usize>::random_len(&mut OsRng, 10);
        assert!(vec[0] != 0);

        // let a = boxed as SecureBox<dyn core::any::Any>;

        // let x = SecureBox::into_raw(boxed);
        // let y = unsafe { &mut *x } as &mut dyn core::any::Any;
        // let boxed = unsafe { SecureBox::from_raw(y as *mut dyn core::any::Any) };
    }
}
