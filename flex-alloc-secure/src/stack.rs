//! Support for stack-allocated types which employ memory locking.

use core::any::type_name;
use core::fmt;
use core::mem::MaybeUninit;
use core::ptr;
use core::slice;

use const_default::ConstDefault;
use rand_core::RngCore;
use zeroize::{DefaultIsZeroes, Zeroize};

use crate::{
    alloc::{lock_pages, unlock_pages, UNINIT_ALLOC_BYTE},
    bytes::FillBytes,
    protect::SecureRef,
};

/// A stack-allocated value protected by locking its virtual memory page in physical memory.
#[repr(align(4096))]
#[cfg_attr(all(target_arch = "aarch64", target_os = "macos"), repr(align(16384)))]
pub struct Secured<T: Copy>(MaybeUninit<T>);

impl<T: Copy> Secured<T> {
    /// For an existing `Secured` instance, fill with the default value
    /// of `T` and call the closure `f` with a mutable reference.
    pub fn borrow_default<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(SecureRef<&mut T>) -> R,
        T: Default,
    {
        let lock = SecuredGuard::new(&mut self.0);
        lock.0.write(T::default());
        unsafe { lock.eval_inited(f) }
    }

    /// For an existing `Secured` instance, fill with a set of random
    /// bytes, then return the result of calling the closure `f` with
    /// a mutable reference.
    pub fn borrow_random<F, R>(&mut self, rng: impl RngCore, f: F) -> R
    where
        F: FnOnce(SecureRef<&mut T>) -> R,
        T: FillBytes,
    {
        let mut lock = SecuredGuard::new(&mut self.0);
        lock.fill_random(rng);
        unsafe { lock.eval_inited(f) }
    }

    /// For an existing `Secured` instance, fill with the default value
    /// of `T` and call the closure `f` with a mutable reference.
    pub fn borrow_take<F, R>(&mut self, take: &mut T, f: F) -> R
    where
        F: FnOnce(SecureRef<&mut T>) -> R,
        T: DefaultIsZeroes,
    {
        let lock = SecuredGuard::new(&mut self.0);
        lock.0.write(*take);
        take.zeroize();
        unsafe { lock.eval_inited(f) }
    }

    /// For an existing `Secured` instance, call a closure `f` with a
    /// mutable reference to an uninitialized `T`.
    pub fn borrow_uninit<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(SecureRef<&mut MaybeUninit<T>>) -> R,
    {
        let mut lock = SecuredGuard::new(&mut self.0);
        lock.fill_bytes(UNINIT_ALLOC_BYTE);
        f(SecureRef::new_mut(lock.0))
    }

    /// Fill a `Secured` with the default value of `T` and return the result
    /// of calling the closure `f` with a mutable reference.
    pub fn default<F, R>(f: F) -> R
    where
        F: FnOnce(SecureRef<&mut T>) -> R,
        T: Default,
    {
        let mut slf = Secured::DEFAULT;
        slf.borrow_default(f)
    }

    /// Fill a `Secured` with a set of random bytes, then return the
    /// result of calling the closure `f` with a mutable reference.
    pub fn random<F, R>(rng: impl RngCore, f: F) -> R
    where
        F: FnOnce(SecureRef<&mut T>) -> R,
        T: FillBytes,
    {
        let mut slf = Secured::DEFAULT;
        slf.borrow_random(rng, f)
    }

    /// Fill a `Secured` by coping an existing value of type `T`,
    /// and zeroize the original copy. Return the result of calling the
    /// closure `f` with a mutable reference.
    pub fn take<F, R>(take: &mut T, f: F) -> R
    where
        F: FnOnce(SecureRef<&mut T>) -> R,
        T: DefaultIsZeroes,
    {
        let mut slf = Secured::DEFAULT;
        slf.borrow_take(take, f)
    }

    /// Call the closure `f` with a mutable reference to an uninitialized
    /// `T`.
    pub fn uninit<F, R>(f: F) -> R
    where
        F: FnOnce(SecureRef<&mut MaybeUninit<T>>) -> R,
    {
        let mut slf = Secured::DEFAULT;
        slf.borrow_uninit(f)
    }
}

impl<const N: usize> Secured<[u8; N]> {
    /// For an existing `Secured` instance, call a closure with a mutable
    /// reference to an array of bytes. The values of the bytes are
    /// initialized to a standard indicator value.
    pub fn borrow_bytes<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(SecureRef<&mut [u8; N]>) -> R,
    {
        let mut lock = SecuredGuard::new(&mut self.0);
        lock.fill_bytes(UNINIT_ALLOC_BYTE);
        unsafe { lock.eval_inited(f) }
    }

    /// Call the closure `f` with a mutable reference to an array of
    /// bytes. The values of the bytes are initialized to a standard
    /// indicator value.
    pub fn bytes<F, R>(f: F) -> R
    where
        F: FnOnce(SecureRef<&mut [u8; N]>) -> R,
    {
        let mut slf = Secured::DEFAULT;
        slf.borrow_bytes(f)
    }
}

impl<T: Copy> fmt::Debug for Secured<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("Secured<{}>", type_name::<T>()))
    }
}

impl<T: Copy> ConstDefault for Secured<T> {
    const DEFAULT: Self = Secured(MaybeUninit::<T>::uninit());
}

impl<T: Copy> Default for Secured<T> {
    fn default() -> Self {
        Self::DEFAULT
    }
}

struct SecuredGuard<'a, T>(&'a mut MaybeUninit<T>);

impl<'a, T> SecuredGuard<'a, T> {
    pub fn new(data: &'a mut MaybeUninit<T>) -> Self {
        lock_pages(data.as_mut_ptr().cast(), size_of::<T>()).expect("Error locking stack memory");
        Self(data)
    }

    #[inline]
    // SAFETY: `self.0` must be initialized prior to calling.
    pub unsafe fn eval_inited<R>(self, f: impl FnOnce(SecureRef<&mut T>) -> R) -> R {
        struct Dropper<'d, D>(&'d mut D);

        impl<D> Drop for Dropper<'_, D> {
            fn drop(&mut self) {
                unsafe {
                    ptr::drop_in_place(self.0);
                }
            }
        }

        let drop = Dropper(self.0.assume_init_mut());
        f(SecureRef::new_mut(drop.0))
    }
}

unsafe impl<T> FillBytes for SecuredGuard<'_, T> {
    fn as_bytes_mut(&mut self) -> &mut [u8] {
        let len: usize = size_of_val(self.0);
        unsafe { slice::from_raw_parts_mut(self.0 as *mut MaybeUninit<T> as *mut u8, len) }
    }
}

impl<T> Drop for SecuredGuard<'_, T> {
    fn drop(&mut self) {
        self.0.zeroize();
        match unlock_pages(self.0.as_mut_ptr().cast(), size_of::<T>()) {
            Ok(_) => (),
            Err(_) => {
                if !std::thread::panicking() {
                    panic!("Error unlocking memory");
                }
            }
        };
    }
}

#[cfg(test)]
mod tests {
    use const_default::ConstDefault;
    use rand_core::OsRng;

    use super::Secured;
    use crate::alloc::UNINIT_ALLOC_BYTE;

    #[test]
    fn secured_default() {
        let mut sec = Secured::<usize>::DEFAULT;
        #[cfg_attr(miri, allow(unused))]
        let ptr = sec.borrow_default(|mut b| {
            assert_eq!(&*b, &0);
            *b = 99usize;
            &*b as *const usize
        });
        // ensure the value is zeroized after use
        #[cfg(not(miri))]
        assert_eq!(unsafe { *ptr }, 0usize);

        Secured::<[u8; 10]>::default(|r| {
            assert_eq!(&*r, &[0; 10]);
        });
    }

    #[test]
    fn secured_random() {
        // The comparisons below could spuriously fail, with a low probability.

        Secured::<[u8; 10]>::random(OsRng, |r| {
            assert_ne!(&*r, &[0u8; 10]);
        });

        let mut sec = Secured::<[u8; 20]>::DEFAULT;
        sec.borrow_random(OsRng, |r| {
            assert_ne!(&*r, &[0u8; 20]);
        });
    }

    #[test]
    fn secured_take() {
        let mut value = 99usize;

        Secured::take(&mut value, |v| {
            assert_eq!(&*v, &99);
        });
        // ensure the value is zeroized when taken
        assert_eq!(value, 0);
    }

    #[test]
    fn secured_uninit() {
        Secured::<[u8; 10]>::bytes(|r| {
            assert_eq!(&*r, &[UNINIT_ALLOC_BYTE; 10]);
        });
        Secured::<u32>::uninit(|m| {
            let val = unsafe { m.assume_init() };
            assert_eq!(val.to_ne_bytes(), [UNINIT_ALLOC_BYTE; 4]);
        });
    }
}
