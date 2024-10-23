//! Secret handling for collection types.

use core::any::type_name;
use core::cell::UnsafeCell;
use core::cmp;
use core::fmt;
use core::mem::size_of_val;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicUsize, Ordering};

use zeroize::ZeroizeOnDrop;

use crate::alloc::{ProtectionMode, SecureAlloc};
use crate::boxed::SecureBox;
use crate::vec::SecureVec;

/// Convert a secure collection type into a [`ProtectedBox`].
pub trait Protect {
    /// The value type of the [`ProtectedBox`].
    type Value: ?Sized;

    /// Convert this collection into a [`ProtectedBox`].
    fn protect(self) -> ProtectedBox<Self::Value>;
}

impl<T: ?Sized> Protect for SecureBox<T> {
    type Value = T;

    fn protect(self) -> ProtectedBox<T> {
        ProtectedBox::from(self)
    }
}

impl<T> Protect for SecureVec<T> {
    type Value = [T];

    fn protect(self) -> ProtectedBox<[T]> {
        ProtectedBox::from(self)
    }
}

/// Provide read-only access to a protected value.
pub trait ReadProtected {
    /// The type of the contained value.
    type Value: ?Sized;
    /// The type of the reference to the contained value.
    type Ref<'a>: Deref<Target = Self::Value>
    where
        Self: 'a;

    /// Access the secret value for reading.
    fn read_protected(&self) -> Self::Ref<'_>;
}

/// Provide read-write access to a protected value.
pub trait WriteProtected: ReadProtected {
    /// The type of the mutable reference to the contained value.
    type RefMut<'a>: DerefMut<Target = Self::Value>
    where
        Self: 'a;

    /// Expose secret: this is the only method providing access to a secret.
    fn write_protected(&mut self) -> Self::RefMut<'_>;
}

/// A wrapper around a [`SecureBox`] which provides limited access to the
/// contained value, enforcing memory protections which not in active use.
pub struct ProtectedBox<T: ?Sized> {
    inner: UnsafeCell<SecureBox<T>>,
    refs: AtomicUsize,
}

impl<T: ?Sized> ProtectedBox<T> {
    /// Obtain a reference to the allocator.
    pub fn allocator(&self) -> &SecureAlloc {
        unsafe { &*self.inner.get() }.allocator()
    }

    #[inline]
    fn inner(&self) -> &SecureBox<T> {
        unsafe { &*self.inner.get() }
    }

    #[inline]
    fn inner_mut(&mut self) -> &mut SecureBox<T> {
        unsafe { &mut *self.inner.get() }
    }

    #[inline]
    fn set_protection_mode(&self, mode: ProtectionMode) {
        let boxed = unsafe { &mut *self.inner.get() };
        let size = size_of_val(boxed.as_ref());
        let data = boxed.as_mut_ptr().cast();
        boxed
            .allocator()
            .set_page_protection(data, size, mode)
            .expect("Error setting protection mode")
    }

    #[inline]
    fn from_inner(inner: SecureBox<T>) -> Self {
        let slf = Self {
            inner: UnsafeCell::new(inner),
            refs: AtomicUsize::new(0),
        };
        slf.set_protection_mode(ProtectionMode::NoAccess);
        slf
    }
}

impl<T: Default> Default for ProtectedBox<T> {
    fn default() -> Self {
        Self::from_inner(SecureBox::default())
    }
}

impl<T: Clone> From<&T> for ProtectedBox<T> {
    fn from(value: &T) -> Self {
        Self::from_inner(SecureBox::from(value))
    }
}

impl<T: Clone> From<&[T]> for ProtectedBox<[T]> {
    fn from(data: &[T]) -> Self {
        Self::from_inner(SecureBox::from(data))
    }
}

impl<T: ?Sized> From<SecureBox<T>> for ProtectedBox<T> {
    fn from(inner: SecureBox<T>) -> Self {
        Self::from_inner(inner)
    }
}

impl<T> From<SecureVec<T>> for ProtectedBox<[T]> {
    fn from(vec: SecureVec<T>) -> Self {
        Self::from_inner(vec.into_boxed_slice())
    }
}

impl<T: ?Sized> Drop for ProtectedBox<T> {
    fn drop(&mut self) {
        // enable usual drop behavior for the inner secure box
        self.set_protection_mode(ProtectionMode::ReadWrite);
    }
}

impl<T: ?Sized> ReadProtected for ProtectedBox<T> {
    type Value = T;
    type Ref<'a> = ProtectedBoxRef<'a, T> where T: 'a;

    fn read_protected(&self) -> Self::Ref<'_> {
        let prev = self.refs.fetch_add(2, Ordering::Acquire);
        if (prev + 2) >> (usize::BITS - 1) != 0 {
            panic!("exceeded maximum number of references");
        }
        if prev & 1 != 0 {
            // already locked
        } else if prev == 0 {
            // our responsibility to lock
            self.set_protection_mode(ProtectionMode::ReadOnly);
            self.refs.fetch_add(1, Ordering::Release);
        } else {
            // wait for other thread to lock
            loop {
                let prev = self.refs.load(Ordering::Relaxed);
                if prev & 1 == 1 {
                    break;
                }
                std::thread::yield_now();
            }
        }
        ProtectedBoxRef(self)
    }
}

impl<T: ?Sized> WriteProtected for ProtectedBox<T> {
    type RefMut<'a> = ProtectedBoxMut<'a, T> where T: 'a;

    fn write_protected(&mut self) -> Self::RefMut<'_> {
        self.set_protection_mode(ProtectionMode::ReadWrite);
        ProtectedBoxMut(self)
    }
}

unsafe impl<T: Send + ?Sized> Send for ProtectedBox<T> {}
unsafe impl<T: Sync + ?Sized> Sync for ProtectedBox<T> {}

impl<T: ?Sized> ZeroizeOnDrop for ProtectedBox<T> {}

/// A managed reference to the value of a [`ProtectedBox`].
pub struct ProtectedBoxRef<'a, T: ?Sized>(&'a ProtectedBox<T>);

impl<T: ?Sized> AsRef<T> for ProtectedBoxRef<'_, T> {
    #[inline]
    fn as_ref(&self) -> &T {
        self.0.inner().as_ref()
    }
}

impl<T: ?Sized> fmt::Debug for ProtectedBoxRef<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("ProtectedBoxRef<{}>", type_name::<T>()))
    }
}

impl<T: ?Sized> Deref for ProtectedBoxRef<'_, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.0.inner().as_ref()
    }
}

impl<T: ?Sized> Drop for ProtectedBoxRef<'_, T> {
    fn drop(&mut self) {
        loop {
            let state =
                match self
                    .0
                    .refs
                    .compare_exchange_weak(3, 2, Ordering::Acquire, Ordering::Relaxed)
                {
                    Ok(_) => {
                        self.0.set_protection_mode(ProtectionMode::NoAccess);
                        match self.0.refs.compare_exchange(
                            2,
                            0,
                            Ordering::Release,
                            Ordering::Relaxed,
                        ) {
                            Ok(_) => break,
                            Err(_) => {
                                // another thread will be waiting for the lock
                                self.0.set_protection_mode(ProtectionMode::ReadOnly);
                                let prev = self.0.refs.fetch_add(1, Ordering::Release);
                                prev - 1
                            }
                        }
                    }
                    Err(state) => state,
                };

            match state.cmp(&3) {
                cmp::Ordering::Less => {
                    panic!("Reference count error");
                }
                cmp::Ordering::Equal => {
                    // try again
                }
                cmp::Ordering::Greater => {
                    if self
                        .0
                        .refs
                        .compare_exchange_weak(
                            state,
                            state - 2,
                            Ordering::Release,
                            Ordering::Acquire,
                        )
                        .is_ok()
                    {
                        break;
                    }
                }
            }
        }
    }
}

/// A managed mutable reference to the value of a [`ProtectedBox`].
pub struct ProtectedBoxMut<'a, T: ?Sized>(&'a mut ProtectedBox<T>);

impl<T: ?Sized> AsRef<T> for ProtectedBoxMut<'_, T> {
    #[inline]
    fn as_ref(&self) -> &T {
        self.0.inner().as_ref()
    }
}

impl<T: ?Sized> AsMut<T> for ProtectedBoxMut<'_, T> {
    #[inline]
    fn as_mut(&mut self) -> &mut T {
        self.0.inner_mut().as_mut()
    }
}

impl<T: ?Sized> fmt::Debug for ProtectedBoxMut<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("ProtectedBoxMut<{}>", type_name::<T>()))
    }
}

impl<T: ?Sized> Deref for ProtectedBoxMut<'_, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.0.inner().as_ref()
    }
}

impl<T: ?Sized> DerefMut for ProtectedBoxMut<'_, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.inner_mut().as_mut()
    }
}

impl<T: ?Sized> Drop for ProtectedBoxMut<'_, T> {
    fn drop(&mut self) {
        self.0.set_protection_mode(ProtectionMode::NoAccess);
    }
}

#[cfg(test)]
mod tests {
    use core::sync::atomic::Ordering;

    use crate::vec::SecureVec;

    use super::{ProtectedBox, ReadProtected};

    #[test]
    fn protected_ref_count() {
        let prot = ProtectedBox::<usize>::default();
        let r1 = prot.read_protected();
        assert_eq!(prot.refs.load(Ordering::Relaxed), 3);
        let r2 = prot.read_protected();
        assert_eq!(prot.refs.load(Ordering::Relaxed), 5);
        drop(r1);
        assert_eq!(prot.refs.load(Ordering::Relaxed), 3);
        drop(r2);
        assert_eq!(prot.refs.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn sec_vec() {
        let mut vec = SecureVec::new();
        for _ in 0..100 {
            vec.push(1usize);
        }
    }
}
