use core::any::type_name;
use core::fmt;
use core::mem::MaybeUninit;
use core::ops::{Deref, DerefMut};
use core::ptr;
use core::slice;

use const_default::ConstDefault;
use rand_core::RngCore;
use zeroize::Zeroize;

use crate::{
    alloc::{lock_memory, unlock_memory, UNINIT_ALLOC_BYTE},
    random::FillBytes,
};

/// A stack-allocated value protected by locking its virtual memory page in physical memory.
#[repr(align(4096))]
#[cfg_attr(all(target_arch = "aarch64", target_os = "macos"), repr(align(16384)))]
pub struct Secured<T: Copy>(MaybeUninit<T>);

impl<T: Copy> Secured<T> {
    /// Create a new, empty `Secured` to store a value `T`.
    pub const fn new_uninit() -> Secured<T> {
        Self::DEFAULT
    }
}

impl<T: Copy> Secured<T> {
    pub fn borrow_with_default<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(SecuredMut<T>) -> R,
        T: Default,
    {
        let lock = SecuredLock::new(&mut self.0);
        lock.0.write(T::default());
        unsafe { lock.eval_inited(f) }
    }

    pub fn borrow_with_uninit<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(SecuredMut<MaybeUninit<T>>) -> R,
    {
        let mut lock = SecuredLock::new(&mut self.0);
        lock.fill_bytes(UNINIT_ALLOC_BYTE);
        f(SecuredMut::new(lock.0))
    }

    pub fn with_default<F, R>(f: F) -> R
    where
        F: FnOnce(SecuredMut<T>) -> R,
        T: Default,
    {
        let mut slf = Secured::new_uninit();
        slf.borrow_with_default(f)
    }

    pub fn with_uninit<F, R>(f: F) -> R
    where
        F: FnOnce(SecuredMut<MaybeUninit<T>>) -> R,
    {
        let mut slf = Secured::new_uninit();
        slf.borrow_with_uninit(f)
    }
}

impl<const N: usize> Secured<[u8; N]> {
    pub fn borrow_with_bytes<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(SecuredMut<[u8; N]>) -> R,
    {
        let mut lock = SecuredLock::new(&mut self.0);
        lock.fill_bytes(UNINIT_ALLOC_BYTE);
        unsafe { lock.eval_inited(f) }
    }

    pub fn with_bytes<F, R>(f: F) -> R
    where
        F: FnOnce(SecuredMut<[u8; N]>) -> R,
    {
        let mut slf = Secured::new_uninit();
        slf.borrow_with_bytes(f)
    }
}

impl<T: Copy + FillBytes> Secured<T> {
    pub fn borrow_with_random<F, R>(&mut self, rng: impl RngCore, f: F) -> R
    where
        F: FnOnce(SecuredMut<T>) -> R,
        T: FillBytes,
    {
        let mut lock = SecuredLock::new(&mut self.0);
        lock.fill_random(rng);
        unsafe { lock.eval_inited(f) }
    }

    pub fn with_random<F, R>(rng: impl RngCore, f: F) -> R
    where
        F: FnOnce(SecuredMut<T>) -> R,
        T: FillBytes,
    {
        let mut slf = Secured::new_uninit();
        slf.borrow_with_random(rng, f)
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

struct SecuredLock<'a, T>(&'a mut MaybeUninit<T>);

impl<'a, T> SecuredLock<'a, T> {
    pub fn new(data: &'a mut MaybeUninit<T>) -> Self {
        lock_memory(data.as_mut_ptr().cast(), size_of::<T>()).expect("Error locking stack memory");
        Self(data)
    }

    #[inline]
    // SAFETY: `self.0` must be initialized prior to calling.
    pub unsafe fn eval_inited<R>(self, f: impl FnOnce(SecuredMut<T>) -> R) -> R {
        struct Dropper<'d, D>(&'d mut D);

        impl<D> Drop for Dropper<'_, D> {
            fn drop(&mut self) {
                unsafe {
                    ptr::drop_in_place(self.0);
                }
            }
        }

        let drop = Dropper(self.0.assume_init_mut());
        f(SecuredMut::new(drop.0))
    }
}

unsafe impl<T> FillBytes for SecuredLock<'_, T> {
    fn as_bytes_mut(&mut self) -> &mut [u8] {
        let len: usize = size_of_val(self.0);
        unsafe { slice::from_raw_parts_mut(self.0 as *mut MaybeUninit<T> as *mut u8, len) }
    }
}

impl<T> Drop for SecuredLock<'_, T> {
    fn drop(&mut self) {
        self.0.zeroize();
        match unlock_memory(self.0.as_mut_ptr().cast(), size_of::<T>()) {
            Ok(_) => (),
            Err(_) => {
                if !std::thread::panicking() {
                    panic!("Error unlocking memory");
                }
            }
        };
    }
}

/// Temporary mutable access to a `Secured` value.
pub struct SecuredMut<'m, T: ?Sized>(&'m mut T);

impl<'a, T: ?Sized> SecuredMut<'a, T> {
    #[inline]
    fn new(inner: &'a mut T) -> Self {
        Self(inner)
    }
}

impl<T: ?Sized> AsRef<T> for SecuredMut<'_, T> {
    #[inline]
    fn as_ref(&self) -> &T {
        self.0
    }
}

impl<T: ?Sized> AsMut<T> for SecuredMut<'_, T> {
    #[inline]
    fn as_mut(&mut self) -> &mut T {
        self.0
    }
}

impl<T: ?Sized> Deref for SecuredMut<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl<T: ?Sized> DerefMut for SecuredMut<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0
    }
}

impl<T: ?Sized> fmt::Debug for SecuredMut<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("SecuredMut<{}>", type_name::<T>()))
    }
}

#[cfg(test)]
mod tests {
    use const_default::ConstDefault;

    use super::Secured;
    use crate::alloc::UNINIT_ALLOC_BYTE;

    #[test]
    fn rand_secured() {
        use rand_core::OsRng;

        Secured::<[u8; 10]>::with_random(OsRng, |r| {
            assert!(r.as_ref() != &[0u8; 10]);
        });

        Secured::<[u8; 10]>::with_bytes(|r| {
            assert!(r[0] == UNINIT_ALLOC_BYTE);
        });

        let mut sec = Secured::<usize>::DEFAULT;
        sec.borrow_with_default(|b| {
            assert!(*b == 0);
        });
        sec.borrow_with_random(OsRng, |b| {
            assert!(*b != 0);
        });
    }
}
