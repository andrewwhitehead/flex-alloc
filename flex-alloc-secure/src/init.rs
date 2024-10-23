use core::mem::{size_of_val, MaybeUninit};
use core::ptr;
use core::slice;

use flex_alloc::alloc::Allocator;
use flex_alloc::boxed::Box;
use flex_alloc::index::Index;
use flex_alloc::vec::{config::VecConfig, Vec};
use rand_core::RngCore;
use zeroize::{DefaultIsZeroes, Zeroize};

use crate::boxed::SecureBox;
use crate::protect::IntoProtected;
use crate::vec::SecureVec;

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

/// Securely initialize a container type for a sized value.
pub trait SecureInit: IntoProtected + Sized {
    /// The type of the contained value.
    type Inner;
    /// The type of the uninitialized container.
    type Uninit: AsMut<MaybeUninit<Self::Inner>>;

    /// Create a new uninitialized container.
    fn new_uninit() -> Self::Uninit;

    /// Convert into an initialized container.
    /// # Safety
    /// The contents of the container must be initialized.
    unsafe fn assume_init(uninit: Self::Uninit) -> Self;

    /// Initialize the container from the result of a closure.
    fn new_with(f: impl FnOnce() -> Self::Inner) -> Self {
        let mut slf = Self::new_uninit();
        slf.as_mut().write(f());
        unsafe { Self::assume_init(slf) }
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

    /// Create a new container by copying and zeroizing an existing value.
    fn take(value: &mut Self::Inner) -> Self
    where
        Self::Inner: DefaultIsZeroes,
    {
        let mut slf = Self::new_uninit();
        unsafe { ptr::copy_nonoverlapping(value, slf.as_mut().as_mut_ptr(), 1) };
        value.zeroize();
        unsafe { Self::assume_init(slf) }
    }

    /// Return a slice of bytes initialized with a standard indicator value.
    fn uninit_bytes<const N: usize>() -> Self
    where
        Self: SecureInit<Inner = [u8; N]>,
    {
        let slf = Self::new_uninit();
        unsafe { Self::assume_init(slf) }
    }
}

impl<T> SecureInit for SecureBox<T> {
    type Inner = T;
    type Uninit = SecureBox<MaybeUninit<T>>;

    fn new_uninit() -> Self::Uninit {
        SecureBox::new_uninit()
    }

    unsafe fn assume_init(uninit: Self::Uninit) -> Self {
        uninit.assume_init()
    }
}

/// Securely initialize a container type for a slice of elements.
pub trait SecureInitSlice: IntoProtected + Sized {
    /// The type of the contained element.
    type Item;
    /// The index type of the collection.
    type Index: Index;
    /// The type of the uninitialized container.
    type Uninit: AsMut<[MaybeUninit<Self::Item>]>;

    /// Create a new uninitialized container of length `len`.
    fn new_uninit_slice(len: Self::Index) -> Self::Uninit;

    /// Convert into an initialized container.
    /// # Safety
    /// The contents of the container must be initialized.
    unsafe fn assume_init(uninit: Self::Uninit) -> Self;

    /// Create a new container with `len` default elements.
    fn default_slice(len: Self::Index) -> Self
    where
        Self::Item: Default,
    {
        let mut slf = Self::new_uninit_slice(len);
        slf.as_mut()
            .fill_with(|| MaybeUninit::new(Default::default()));
        unsafe { Self::assume_init(slf) }
    }

    /// Generate a new random slice of length `len`.
    fn random_slice(rng: impl RngCore, len: Self::Index) -> Self
    where
        Self::Uninit: FillBytes,
    {
        let mut slf = Self::new_uninit_slice(len);
        slf.fill_random(rng);
        unsafe { Self::assume_init(slf) }
    }

    /// Create a new container by copying and zeroizing an existing slice.
    fn take_slice(value: &mut [Self::Item]) -> Self
    where
        Self::Item: DefaultIsZeroes,
    {
        let len = value.len();
        let mut slf = Self::new_uninit_slice(Self::Index::from_usize(len));
        unsafe {
            ptr::copy_nonoverlapping(
                value.as_ptr(),
                slf.as_mut().as_mut_ptr() as *mut Self::Item,
                len,
            )
        };
        value.zeroize();
        unsafe { Self::assume_init(slf) }
    }

    /// Return a slice of bytes initialized with a standard indicator value.
    fn uninit_byte_slice(len: Self::Index) -> Self
    where
        Self: SecureInitSlice<Item = u8>,
    {
        let slf = Self::new_uninit_slice(len);
        unsafe { Self::assume_init(slf) }
    }
}

impl<T> SecureInitSlice for SecureBox<[T]> {
    type Item = T;
    type Index = usize;
    type Uninit = SecureBox<[MaybeUninit<T>]>;

    fn new_uninit_slice(len: Self::Index) -> Self::Uninit {
        SecureBox::new_uninit_slice(len)
    }

    unsafe fn assume_init(uninit: Self::Uninit) -> Self {
        uninit.assume_init()
    }
}

impl<T> SecureInitSlice for SecureVec<T> {
    type Item = T;
    type Index = usize;
    type Uninit = SecureBox<[MaybeUninit<T>]>;

    fn new_uninit_slice(len: Self::Index) -> Self::Uninit {
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
    use rand_core::OsRng;

    use super::{SecureInit, SecureInitSlice};
    use crate::{alloc::UNINIT_ALLOC_BYTE, boxed::SecureBox, vec::SecureVec};

    #[test]
    fn init_default() {
        let boxed = SecureBox::<[usize]>::default_slice(10);
        assert_eq!(&*boxed, &[0; 10]);
    }

    #[test]
    fn init_random() {
        let boxed = SecureBox::<[usize; 10]>::random(&mut OsRng);
        assert_ne!(boxed[0], 0);

        let boxed_slice = SecureBox::<[usize]>::random_slice(&mut OsRng, 10);
        assert_eq!(boxed_slice.len(), 10);
        assert_ne!(boxed_slice[1], 0);

        let vec = SecureVec::<usize>::random_slice(&mut OsRng, 12);
        assert_eq!(vec.len(), 12);
        assert_ne!(vec[1], 0);
    }

    #[test]
    fn init_take() {
        let mut value = 99usize;
        let boxed = SecureBox::take(&mut value);
        assert_eq!(value, 0);
        assert_eq!(&*boxed, &99usize);
    }

    #[test]
    fn init_uninit() {
        let boxed = SecureBox::<[u8]>::new_uninit_slice(10);
        assert_eq!(&*unsafe { boxed.assume_init() }, &[UNINIT_ALLOC_BYTE; 10]);

        let boxed = SecureBox::uninit_bytes::<15>();
        assert_eq!(&*boxed, &[UNINIT_ALLOC_BYTE; 15]);

        let boxed = SecureBox::uninit_byte_slice(10);
        assert_eq!(&*boxed, &[UNINIT_ALLOC_BYTE; 10]);
    }
}
