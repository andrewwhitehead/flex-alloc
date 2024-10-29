use core::any::type_name;
use core::fmt;
use core::mem::MaybeUninit;
use core::ops;
use core::ptr;

use rand_core::RngCore;
use zeroize::{DefaultIsZeroes, Zeroize};

use crate::boxed::SecureBox;
use crate::bytes::FillBytes;

/// Access a protected value for reading or writing.
///
/// While the value is being accessed, the protections
/// offered by the container (including memory protections
/// and/or encryption) will be inactive.
pub trait ExposeProtected {
    /// The type of the referenced value.
    type Target: ?Sized;

    /// Expose the protected value for reading.
    fn expose_read<F>(&self, f: F)
    where
        F: FnOnce(SecureRef<&Self::Target>);

    /// Expose the protected value for updating.
    fn expose_write<F>(&mut self, f: F)
    where
        F: FnOnce(SecureRef<&mut Self::Target>);

    /// Unwrap the protected value.
    fn unprotect(self) -> SecureBox<Self::Target>;
}

/// Initialize a protected container type for a sized value.
pub trait ProtectedInit: ExposeProtected + From<SecureBox<Self::Target>> + Sized {
    /// For a concrete type implementing `FillBytes`, initialize with the
    /// standard indicator value and call the closure `f` with a mutable
    /// reference to the contained value before applying protections.
    fn init<F>(f: F) -> Self
    where
        F: FnOnce(SecureRef<&mut Self::Target>),
        Self::Target: Copy + FillBytes,
    {
        let mut boxed = unsafe { SecureBox::<Self::Target>::new_uninit().assume_init() };
        f(SecureRef::new_mut(boxed.as_mut()));
        boxed.into()
    }

    /// Initialize with the default value for `Self::Target`, and call the
    /// closure `f` with a mutable reference to the contained value before
    /// applying protections.
    fn init_default<F>(f: F) -> Self
    where
        F: FnOnce(SecureRef<&mut Self::Target>),
        Self::Target: Default,
    {
        let mut boxed = SecureBox::<Self::Target>::default();
        f(SecureRef::new_mut(boxed.as_mut()));
        boxed.into()
    }

    /// Initialize with a randomized value for `Self::Target`, and call the
    /// closure `f` with a mutable reference to the contained value before
    /// applying protections.
    fn init_random<F>(rng: impl RngCore, f: F) -> Self
    where
        F: FnOnce(SecureRef<&mut Self::Target>),
        Self::Target: Copy + FillBytes,
    {
        let mut boxed = SecureBox::<Self::Target>::new_uninit();
        boxed.fill_random(rng);
        let mut boxed = unsafe { boxed.assume_init() };
        f(SecureRef::new_mut(boxed.as_mut()));
        boxed.into()
    }

    /// Initialize by copying the value contained in `from` and zeroizing the
    /// existing copy. Call the closure `f` with a mutable reference to the
    /// contained value before applying protections.
    fn init_take<F>(from: &mut Self::Target, f: F) -> Self
    where
        F: FnOnce(SecureRef<&mut Self::Target>),
        Self::Target: Copy + Zeroize,
    {
        let boxed = SecureBox::new_uninit();
        let mut boxed = SecureBox::write(boxed, *from);
        from.zeroize();
        f(SecureRef::new_mut(boxed.as_mut()));
        boxed.into()
    }

    /// Initialize by calling the closure `f`, store the resulting
    /// instance of `Self::Target` and apply protections.
    #[inline(always)]
    fn init_with<F>(f: F) -> Self
    where
        F: FnOnce() -> Self::Target,
        Self::Target: Sized,
    {
        let boxed = SecureBox::new_uninit();
        SecureBox::write(boxed, f()).into()
    }

    /// Initialize by calling the fallible closure `f`, store the resulting
    /// instance of `Self::Target` and apply protections. On failure, return
    /// the error type `E`.
    #[inline(always)]
    fn try_init_with<F, E>(f: F) -> Result<Self, E>
    where
        F: FnOnce() -> Result<Self::Target, E>,
        Self::Target: Sized,
    {
        let boxed = SecureBox::new_uninit();
        Ok(SecureBox::write(boxed, f()?).into())
    }

    /// Create a new protected instance containing a random value.
    fn random(rng: impl RngCore) -> Self
    where
        Self::Target: Copy + FillBytes,
    {
        Self::init_random(rng, |_| ())
    }

    /// Create a new protected instance by copying and zeroizing an
    /// existing value.
    fn take(from: &mut Self::Target) -> Self
    where
        Self::Target: Copy + Zeroize,
    {
        Self::init_take(from, |_| ())
    }
}

impl<T, W> ProtectedInit for W where W: From<SecureBox<T>> + ExposeProtected<Target = T> {}

/// Initialize a protected container type for a slice of elements.
pub trait ProtectedInitSlice:
    ExposeProtected<Target = [Self::Item]> + From<SecureBox<[Self::Item]>> + Sized
{
    /// The type of the elements contained in the slice.
    type Item;

    /// For a concrete type implementing `FillBytes`, initialize a slice
    /// of length `len` with the standard indicator value and call the closure
    /// `f` with a mutable reference to the slice before applying protections.
    fn init_slice<F>(len: usize, f: F) -> Self
    where
        F: FnOnce(SecureRef<&mut [Self::Item]>),
        Self::Item: Copy + FillBytes,
    {
        let mut boxed = unsafe { SecureBox::<[Self::Item]>::new_uninit_slice(len).assume_init() };
        f(SecureRef::new_mut(boxed.as_mut()));
        boxed.into()
    }

    /// Initialize with a slice of length `len` containing the default value for
    /// `Self::Item`, and call the closure `f` with a mutable reference to the
    /// slice before applying protections.
    fn init_default_slice<F>(len: usize, f: F) -> Self
    where
        F: FnOnce(SecureRef<&mut [Self::Item]>),
        Self::Item: Default,
    {
        let mut boxed = SecureBox::<[Self::Item]>::new_uninit_slice(len);
        boxed
            .as_mut()
            .fill_with(|| MaybeUninit::new(Default::default()));
        let mut boxed = unsafe { boxed.assume_init() };
        f(SecureRef::new_mut(boxed.as_mut()));
        boxed.into()
    }

    /// Initialize with a randomized slice of length `len`, and call the
    /// closure `f` with a mutable reference to the slice before
    /// applying protections.
    fn init_random_slice<F>(len: usize, rng: impl RngCore, f: F) -> Self
    where
        F: FnOnce(SecureRef<&mut [Self::Item]>),
        Self::Item: Copy + FillBytes,
    {
        let mut boxed = SecureBox::<[Self::Item]>::new_uninit_slice(len);
        boxed.fill_random(rng);
        let mut boxed = unsafe { boxed.assume_init() };
        f(SecureRef::new_mut(boxed.as_mut()));
        boxed.into()
    }

    /// Initialize by copying the slice `from` and zeroizing the existing
    /// copy. Call the closure `f` with a mutable reference to the contained
    /// slice before applying protections.
    fn init_take_slice<F>(from: &mut [Self::Item], f: F) -> Self
    where
        F: FnOnce(SecureRef<&mut [Self::Item]>),
        Self::Item: DefaultIsZeroes,
    {
        let len = from.len();
        let mut boxed = SecureBox::<[Self::Item]>::new_uninit_slice(len);
        unsafe {
            ptr::copy_nonoverlapping(
                from.as_ptr(),
                boxed.as_mut().as_mut_ptr() as *mut Self::Item,
                len,
            )
        };
        from.zeroize();
        let mut boxed = unsafe { boxed.assume_init() };
        f(SecureRef::new_mut(boxed.as_mut()));
        boxed.into()
    }

    /// Create a new protected instance containing a random slice of length `len`.
    fn random_slice(len: usize, rng: impl RngCore) -> Self
    where
        Self::Item: Copy + FillBytes,
    {
        Self::init_random_slice(len, rng, |_| ())
    }

    /// Create a new protected slice instance by copying and zeroizing an
    /// existing slice.
    fn take_slice(from: &mut [Self::Item]) -> Self
    where
        Self::Item: DefaultIsZeroes,
    {
        Self::init_take_slice(from, |_| ())
    }
}

impl<T, W> ProtectedInitSlice for W
where
    W: From<SecureBox<[T]>> + ExposeProtected<Target = [T]>,
{
    type Item = T;
}

/// A managed reference to the value of a [`ProtectedBox`].
pub struct SecureRef<T: ?Sized>(T);

impl<'a, T: ?Sized> SecureRef<&'a T> {
    pub(crate) fn new(inner: &'a T) -> Self {
        Self(inner)
    }
}

impl<'a, T: ?Sized> SecureRef<&'a mut T> {
    pub(crate) fn new_mut(inner: &'a mut T) -> Self {
        Self(inner)
    }
}

impl<'a, T> SecureRef<&'a mut MaybeUninit<T>> {
    /// Convert this reference into an initialized state.
    /// # Safety
    /// If the inner value is not properly initialized, then
    /// undetermined behavior may result.
    #[inline]
    pub unsafe fn assume_init(self) -> SecureRef<&'a mut T> {
        SecureRef(self.0.assume_init_mut())
    }

    /// Write a value to the uninitialized reference and
    /// safely initialize it.
    #[inline(always)]
    pub fn write(slf: Self, value: T) -> SecureRef<&'a mut T> {
        slf.0.write(value);
        SecureRef(unsafe { slf.0.assume_init_mut() })
    }
}

impl<T: ?Sized> AsRef<T> for SecureRef<&'_ T> {
    #[inline]
    fn as_ref(&self) -> &T {
        self.0
    }
}

impl<T: ?Sized> AsRef<T> for SecureRef<&'_ mut T> {
    #[inline]
    fn as_ref(&self) -> &T {
        self.0
    }
}

impl<T: ?Sized> AsMut<T> for SecureRef<&'_ mut T> {
    #[inline]
    fn as_mut(&mut self) -> &mut T {
        self.0
    }
}

impl<T: ?Sized> fmt::Debug for SecureRef<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("ProtectedRef<{}>", type_name::<T>()))
    }
}

impl<T: ?Sized> ops::Deref for SecureRef<&'_ T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        self.0
    }
}

impl<T: ?Sized> ops::Deref for SecureRef<&'_ mut T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl<T: ?Sized> ops::DerefMut for SecureRef<&'_ mut T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0
    }
}
