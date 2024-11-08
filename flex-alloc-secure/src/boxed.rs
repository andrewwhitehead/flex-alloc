//! Support for memory protection around collection types.

use core::any::type_name;
use core::cell::UnsafeCell;
use core::fmt;
use core::mem::ManuallyDrop;
use core::ptr::{addr_of, addr_of_mut};
use core::slice;
use core::sync::atomic;
use std::sync::Once;

use chacha20poly1305::{
    aead::{AeadInPlace, KeyInit},
    ChaCha8Poly1305,
};
use flex_alloc::boxed::Box;
use rand_core::{OsRng, RngCore};
use zeroize::ZeroizeOnDrop;

use crate::alloc::{ProtectionMode, SecureAlloc};
use crate::bytes::FillBytes;
use crate::protect::{ExposeProtected, SecureRef};
use crate::vec::SecureVec;

const ASSOC_DATA_SIZE: usize = 16384;

/// A [`flex-alloc Box`](flex_alloc::boxed::Box) which is backed by a
/// secured allocator and keeps its contents in physical memory. When
/// released, the allocated memory is securely zeroed.
///
/// This container should be converted into a
/// [`ProtectedBox`] or [`ShieldedBox`] to protect secret data.
///
/// This type does NOT protect against accidental output of
/// contained values using the [`Debug`] trait.
///
/// When possible, prefer initialization of the protected container
/// using the [`ProtectedInit`](`crate::ProtectedInit`) or
/// [`ProtectedInitSlice`](`crate::ProtectedInitSlice`) traits.
pub type SecureBox<T> = Box<T, SecureAlloc>;

/// A [`flex-alloc Box`](flex_alloc::boxed::Box) container type which applies
/// additional protections around the memory allocation.
///
/// - The memory is allocated using [`SecureAlloc`] and flagged to remain
///   resident in physical memory (using `mlock`/`VirtualLock`).
/// - When released, the allocated memory is securely zeroed.
/// - When not currently being accessed by the methods of the
///   [`ExposeProtected`] trait, the allocated memory pages are flagged
///   for protection from other processes (using `mprotect`/`VirtualProtect`).
pub struct ProtectedBox<T: ?Sized> {
    shared: SharedAccess<SecureBox<T>>,
}

impl<T: Default> Default for ProtectedBox<T> {
    fn default() -> Self {
        Self::from(SecureBox::default())
    }
}

impl<T: ?Sized> fmt::Debug for ProtectedBox<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("ProtectedBox<{}>", type_name::<T>()))
    }
}

impl<T: ?Sized> Drop for ProtectedBox<T> {
    fn drop(&mut self) {
        BoxData::for_boxed(self.shared.as_mut()).set_protection_mode(ProtectionMode::ReadWrite);
    }
}

impl<T: ?Sized> ExposeProtected for ProtectedBox<T> {
    type Target = T;

    fn expose_read<F>(&self, f: F)
    where
        F: FnOnce(SecureRef<&T>),
    {
        let shared = &self.shared;
        let data = shared.acquire_read(|boxed| {
            BoxData::for_boxed(boxed).set_protection_mode(ProtectionMode::ReadOnly);
        });
        let guard = OnDrop::new(|| {
            shared.release_read(|boxed| {
                BoxData::for_boxed(boxed).set_protection_mode(ProtectionMode::NoAccess);
            });
        });
        f(SecureRef::new(data));
        drop(guard);
    }

    fn expose_write<F>(&mut self, f: F)
    where
        F: FnOnce(SecureRef<&mut Self::Target>),
    {
        let boxed = self.shared.as_mut();
        let mut data = BoxData::for_boxed(boxed);
        data.set_protection_mode(ProtectionMode::ReadWrite);
        let guard = OnDrop::new(|| {
            data.set_protection_mode(ProtectionMode::NoAccess);
        });
        f(SecureRef::new_mut(boxed.as_mut()));
        drop(guard);
    }

    fn unprotect(self) -> SecureBox<Self::Target> {
        let mut shared = unsafe { addr_of!(ManuallyDrop::new(self).shared).read() };
        BoxData::for_boxed(shared.as_mut()).set_protection_mode(ProtectionMode::ReadWrite);
        shared.into_inner()
    }
}

impl<T> From<T> for ProtectedBox<T> {
    fn from(value: T) -> Self {
        Self::from(SecureBox::from(value))
    }
}

impl<T: Clone> From<&[T]> for ProtectedBox<[T]> {
    fn from(data: &[T]) -> Self {
        Self::from(SecureBox::from(data))
    }
}

impl<T, const N: usize> From<[T; N]> for ProtectedBox<[T]> {
    fn from(data: [T; N]) -> Self {
        Self::from(SecureBox::from(data))
    }
}

impl From<&str> for ProtectedBox<str> {
    fn from(data: &str) -> Self {
        Self::from(SecureBox::from(data))
    }
}

impl<T: ?Sized> From<SecureBox<T>> for ProtectedBox<T> {
    fn from(boxed: SecureBox<T>) -> Self {
        let mut wrapper = Self {
            shared: SharedAccess::new(boxed),
        };
        BoxData::for_boxed(wrapper.shared.as_mut()).set_protection_mode(ProtectionMode::NoAccess);
        wrapper
    }
}

impl<T> From<SecureVec<T>> for ProtectedBox<[T]> {
    fn from(vec: SecureVec<T>) -> Self {
        Self::from(vec.into_boxed_slice())
    }
}

unsafe impl<T: Send + ?Sized> Send for ProtectedBox<T> {}
unsafe impl<T: Sync + ?Sized> Sync for ProtectedBox<T> {}

impl<T: ?Sized> ZeroizeOnDrop for ProtectedBox<T> {}

/// A [`flex-alloc Box`](flex_alloc::boxed::Box) container type which applies
/// additional protections around the allocated memory, and is encrypted when
/// not currently being accessed.
///
/// - The memory is allocated using [`SecureAlloc`] and flagged to remain
///   resident in physical memory (using `mlock`/`VirtualLock`).
/// - When released, the allocated memory is securely zeroed.
/// - When not currently being accessed by the methods of the
///   [`ExposeProtected`] trait, the allocated memory pages are flagged
///   for protection from other processes using (`mprotect`/`VirtualProtect`).
/// - When not currently being accessed, the allocated memory is
///   encrypted using the ChaCha8 encryption cipher. A large (16Kb)
///   buffer of randomized bytes is used as associated data during the
///   encryption and decryption process.
pub struct ShieldedBox<T: ?Sized> {
    shared: SharedAccess<SecureBox<T>>,
    key: chacha20poly1305::Key,
    tag: chacha20poly1305::Tag,
}

impl<T: Default> Default for ShieldedBox<T> {
    fn default() -> Self {
        Self::from(SecureBox::default())
    }
}

impl<T: ?Sized> fmt::Debug for ShieldedBox<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("ShieldedBox<{}>", type_name::<T>()))
    }
}

impl<T: ?Sized> Drop for ShieldedBox<T> {
    fn drop(&mut self) {
        BoxData::for_boxed(self.shared.as_mut()).set_protection_mode(ProtectionMode::ReadWrite);
    }
}

impl<T: ?Sized> ExposeProtected for ShieldedBox<T> {
    type Target = T;

    fn expose_read<F>(&self, f: F)
    where
        F: FnOnce(SecureRef<&T>),
    {
        let shared = &self.shared;
        let expose = shared.acquire_read(|boxed| {
            let mut data = BoxData::for_boxed(boxed);
            data.set_protection_mode(ProtectionMode::ReadWrite);
            data.decrypt(&self.key, self.tag);
        });
        let guard = OnDrop::new(|| {
            shared.release_read(|boxed| {
                let mut data = BoxData::for_boxed(boxed);
                data.set_protection_mode(ProtectionMode::ReadWrite);
                let tag = data.encrypt(&self.key);
                if self.tag != tag {
                    panic!("Unshielded box was modified while read-only");
                }
                data.set_protection_mode(ProtectionMode::NoAccess);
            });
        });
        f(SecureRef::new(expose));
        drop(guard);
    }

    fn expose_write<F>(&mut self, f: F)
    where
        F: FnOnce(SecureRef<&mut Self::Target>),
    {
        let boxed = self.shared.as_mut();
        let mut data = BoxData::for_boxed(boxed);
        data.set_protection_mode(ProtectionMode::ReadWrite);
        data.decrypt(&self.key, self.tag);
        let guard = OnDrop::new(|| {
            self.tag = data.encrypt(&self.key);
            data.set_protection_mode(ProtectionMode::NoAccess);
        });
        f(SecureRef::new_mut(boxed.as_mut()));
        drop(guard);
    }

    fn unprotect(self) -> SecureBox<Self::Target> {
        let (key, tag) = (self.key, self.tag);
        let mut shared = unsafe { addr_of!(ManuallyDrop::new(self).shared).read() };
        let mut data = BoxData::for_boxed(shared.as_mut());
        data.set_protection_mode(ProtectionMode::ReadWrite);
        data.decrypt(&key, tag);
        shared.into_inner()
    }
}

impl<T> From<T> for ShieldedBox<T> {
    fn from(value: T) -> Self {
        Self::from(SecureBox::from(value))
    }
}

impl<T: Clone> From<&[T]> for ShieldedBox<[T]> {
    fn from(data: &[T]) -> Self {
        Self::from(SecureBox::from(data))
    }
}

impl<T, const N: usize> From<[T; N]> for ShieldedBox<[T]> {
    fn from(data: [T; N]) -> Self {
        Self::from(SecureBox::from(data))
    }
}

impl From<&str> for ShieldedBox<str> {
    fn from(data: &str) -> Self {
        Self::from(SecureBox::from(data))
    }
}

impl<T: ?Sized> From<SecureBox<T>> for ShieldedBox<T> {
    fn from(boxed: SecureBox<T>) -> Self {
        let mut wrapper = Self {
            shared: SharedAccess::new(boxed),
            key: Default::default(),
            tag: Default::default(),
        };
        wrapper.key.fill_random(OsRng);
        let mut data = BoxData::for_boxed(wrapper.shared.as_mut());
        wrapper.tag = data.encrypt(&wrapper.key);
        data.set_protection_mode(ProtectionMode::NoAccess);
        wrapper
    }
}

impl<T> From<SecureVec<T>> for ShieldedBox<[T]> {
    fn from(vec: SecureVec<T>) -> Self {
        Self::from(vec.into_boxed_slice())
    }
}

unsafe impl<T: Send + ?Sized> Send for ShieldedBox<T> {}
unsafe impl<T: Sync + ?Sized> Sync for ShieldedBox<T> {}

impl<T: ?Sized> ZeroizeOnDrop for ShieldedBox<T> {}

struct OnDrop<F: FnOnce()>(Option<F>);

impl<F: FnOnce()> OnDrop<F> {
    pub fn new(f: F) -> Self {
        Self(Some(f))
    }
}

impl<F: FnOnce()> Drop for OnDrop<F> {
    #[inline]
    fn drop(&mut self) {
        if let Some(f) = self.0.take() {
            f()
        }
    }
}

struct SharedAccess<T> {
    data: UnsafeCell<T>,
    refs: atomic::AtomicUsize,
}

impl<T> SharedAccess<T> {
    const fn new(data: T) -> Self {
        Self {
            data: UnsafeCell::new(data),
            refs: atomic::AtomicUsize::new(0),
        }
    }

    fn acquire_read(&self, acquire: impl FnOnce(&mut T)) -> &T {
        let mut rounds = 0;
        loop {
            let prev = self.refs.fetch_or(1, atomic::Ordering::Acquire);
            if prev == 0 {
                // our responsibility to acquire
                let data = unsafe { &mut *self.data.get() };
                acquire(data);
                // any other readers are queued
                self.refs.store(2, atomic::Ordering::Release);
                break;
            } else if prev & 1 == 0 {
                if (prev + 2) >> (usize::BITS - 1) != 0 {
                    panic!("exceeded maximum number of references");
                }
                // other readers could leave while lock is held
                self.refs.fetch_add(1, atomic::Ordering::Release);
                break;
            } else {
                // busy loop
                rounds += 1;
                if rounds >= 100 {
                    std::thread::yield_now();
                    rounds = 0;
                }
            }
        }
        unsafe { &*self.data.get() }
    }

    fn release_read(&self, release: impl FnOnce(&mut T)) {
        let prev = self.refs.fetch_or(1, atomic::Ordering::Acquire);
        if prev == 2 {
            // our responsibility to release
            let data = unsafe { &mut *self.data.get() };
            release(data);
            self.refs.store(0, atomic::Ordering::Release);
        } else if prev & 1 == 0 {
            // acquired lock, but not our responsibility
            self.refs.fetch_sub(3, atomic::Ordering::Release);
        } else {
            // did not acquire lock, but we can leave
            self.refs.fetch_sub(2, atomic::Ordering::Release);
        }
    }

    pub fn as_mut(&mut self) -> &mut T {
        self.data.get_mut()
    }

    pub fn into_inner(self) -> T {
        self.data.into_inner()
    }
}

struct BoxData {
    ptr: *mut u8,
    len: usize,
}

impl BoxData {
    #[inline]
    pub fn for_boxed<T: ?Sized>(boxed: &mut SecureBox<T>) -> Self {
        let len = size_of_val(boxed.as_ref());
        let ptr = boxed.as_mut_ptr() as *mut u8;
        Self { ptr, len }
    }

    pub fn set_protection_mode(&mut self, mode: ProtectionMode) {
        SecureAlloc
            .set_page_protection(self.ptr, self.len, mode)
            .expect("Error setting page protection");
    }

    #[must_use]
    fn encrypt(&mut self, key: &chacha20poly1305::Key) -> chacha20poly1305::Tag {
        let buffer = unsafe { slice::from_raw_parts_mut(self.ptr, self.len) };
        let engine = ChaCha8Poly1305::new(key);
        let nonce = Default::default();
        engine
            .encrypt_in_place_detached(&nonce, encryption_assoc_data(), buffer)
            .expect("Shielded box encryption error")
    }

    pub fn decrypt(&mut self, key: &chacha20poly1305::Key, tag: chacha20poly1305::Tag) {
        let buffer = unsafe { slice::from_raw_parts_mut(self.ptr, self.len) };
        let engine = ChaCha8Poly1305::new(key);
        let nonce = Default::default();
        engine
            .decrypt_in_place_detached(&nonce, encryption_assoc_data(), buffer, &tag)
            .expect("Shielded box decryption error")
    }
}

fn encryption_assoc_data() -> &'static [u8; ASSOC_DATA_SIZE] {
    static mut DATA: [u8; ASSOC_DATA_SIZE] = [0u8; ASSOC_DATA_SIZE];
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        OsRng.fill_bytes(unsafe { &mut *addr_of_mut!(DATA) });
    });
    unsafe { &*addr_of!(DATA) }
}

#[cfg(test)]
mod tests {

    use super::{
        encryption_assoc_data, ExposeProtected, ProtectedBox, ShieldedBox, ASSOC_DATA_SIZE,
    };
    use crate::vec::SecureVec;

    #[test]
    fn enc_assoc_data_init() {
        let data = encryption_assoc_data();
        assert_ne!(data, &[0u8; ASSOC_DATA_SIZE]);
    }

    #[test]
    fn protected_mut() {
        let mut prot = ProtectedBox::<usize>::default();
        prot.expose_read(|r| {
            assert_eq!(r.as_ref(), &0);
        });
        prot.expose_write(|mut w| {
            *w = 10;
        });
        prot.expose_read(|r| {
            assert_eq!(r.as_ref(), &10);
        });
    }

    #[test]
    fn shielded_mut() {
        let mut prot = ShieldedBox::<usize>::default();
        prot.expose_read(|r| {
            assert_eq!(r.as_ref(), &0);
        });
        prot.expose_write(|mut w| {
            *w = 10;
        });
        prot.expose_read(|r| {
            assert_eq!(r.as_ref(), &10);
        });
    }

    // #[test]
    // fn protected_ref_count() {
    //     let prot = ProtectedBox::<usize>::default();
    //     prot.expose_read(|r1| {
    //         assert_eq!(prot.refs.load(atomic::Ordering::Relaxed), 3);
    //         prot.expose_read(|r2| {
    //             assert_eq!(prot.refs.load(atomic::Ordering::Relaxed), 5);
    //         });
    //         assert_eq!(prot.refs.load(atomic::Ordering::Relaxed), 3);
    //     });
    //     assert_eq!(prot.refs.load(atomic::Ordering::Relaxed), 0);
    // }

    #[test]
    fn protected_vec() {
        let mut vec = SecureVec::new();
        vec.resize(100, 1usize);
        let boxed = ProtectedBox::<[usize]>::from(vec);
        boxed.expose_read(|r| {
            assert_eq!(r.len(), 100);
        });
    }

    // #[test]
    // fn protected_check_protection_crash() {
    //     use crate::boxed::SecureBox;
    //     let boxed = SecureBox::<usize>::default();
    //     let ptr = boxed.as_ptr();
    //     let prot = boxed.protect();
    //     // let read = prot.read_protected(); // would change protection mode
    //     println!("inner: {}", unsafe { &*ptr });
    // }
}
