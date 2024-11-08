//! Support for values contained within allocated memory.
//!
//! Unlike `std::boxed::Box`, this Box type supports allocation within a fixed buffer.
//! Additional fallible constructors and update methods are provided for handling
//! allocation errors.
//!
//! # Usage
//!
//! ## Fixed storage
//!
//! [`Box`] instances may be allocated in fixed storage, a buffer which might be
//! stored on the stack or statically. This can help to work with dynamically
//! sized types such as `str` or `[u8]` without allocating.
//!
//! ```
//! use flex_alloc::{boxed::{Box, FixedBox}, storage::byte_storage};
//!
//! let mut buf = byte_storage::<1024>();
//! let boxed: FixedBox<[usize]> = Box::from_slice_in(&[1, 2, 3], &mut buf);
//! ```
//!
//! A fixed storage buffer may also be chained to an allocator, meaning that
//! when the capacity of the buffer is exceeded, then the allocator will be
//! used to obtain additional memory. For critical sections where the size of
//! the input is variable but may often fit on the stack, this can help to
//! eliminate costly allocations and lead to performance improvements.
//!
//! ```
//! # #[cfg(feature = "alloc")] {
//! use flex_alloc::{
//!     alloc::SpillAlloc,
//!     boxed::{Box, SpillBox},
//!     storage::byte_storage
//! };
//!
//! let mut buf = byte_storage::<1024>();
//! let boxed: SpillBox<[usize; 100]> = Box::new_in([1; 100], buf.spill_alloc());
//! # }
//! ```
//!
//! ## Unsized support
//!
//! Coercion of the [`Box`] type to a trait object such as [`Box<dyn Any>`]
//! currently requires a `nightly` Rust compiler as well as the `nightly` crate
//! feature.
//!
//! ```
//! # #[cfg(feature = "nightly")] {
//! use core::any::Any;
//! use flex_alloc::{boxed::Box};
//!
//! let boxed = Box::new(99usize) as Box<dyn Any + Send>;
//! # }
//! ```
//!
//! ## `zeroize` integration
//!
//! Integration with `zeroize` is implemented at the allocator level in order
//! to zeroize the underlying memory after the contained values have been
//! dropped, which can be supported even for types that do not implement
//! [`Zeroize`].
//!
//! ```
//! # #[cfg(all(feature = "alloc", feature = "zeroize"))] {
//! use flex_alloc::boxed::ZeroizingBox;
//!
//! let boxed = ZeroizingBox::<[usize]>::from([1, 2, 3]);
//! # }

use core::borrow;
use core::cmp::Ordering;
use core::mem::{ManuallyDrop, MaybeUninit};
use core::ops::{Deref, DerefMut};
use core::ptr::NonNull;
use core::str;
use core::{fmt, ptr};

#[cfg(feature = "zeroize")]
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::alloc::{AllocateIn, Allocator, AllocatorDefault, Fixed, Global, Spill};
use crate::storage::boxed::RawBox;
use crate::vec::config::VecConfigAlloc;
use crate::vec::Vec;
use crate::StorageError;

#[cfg(feature = "alloc")]
use crate::alloc::ConvertAlloc;

/// A box which stores its contained data in a fixed external buffer.
pub type FixedBox<'a, T> = Box<T, Fixed<'a>>;

/// A box which stores its contained data in a fixed external buffer,
/// spilling to an allocator when the capacity of the buffer is exceeded.
pub type SpillBox<'a, T, A = Global> = Box<T, Spill<'a, A>>;

#[cfg(feature = "zeroize")]
/// A vector which automatically zeroizes its buffer when dropped.
pub type ZeroizingBox<T> = Box<T, crate::alloc::ZeroizingAlloc<Global>>;

/// A pointer type that uniquely owns an allocation of type `T`.
pub struct Box<T: ?Sized, A: Allocator = Global> {
    pub(crate) handle: RawBox<T, A>,
}

impl<T, A: AllocatorDefault> Box<T, A> {
    /// Allocates in the associated allocator and then places `value` into it.
    /// This doesn’t actually allocate if `T` is zero-sized.
    pub fn new(value: T) -> Box<T, A> {
        match Self::try_new(value) {
            Ok(slf) => slf,
            Err(e) => e.panic(),
        }
    }

    /// Allocates uninitialized memory in the associated allocator.
    /// This doesn’t actually allocate if `T` is zero-sized.
    pub fn new_uninit() -> Box<MaybeUninit<T>, A> {
        match Self::try_new_uninit() {
            Ok(uninit) => uninit,
            Err(e) => e.panic(),
        }
    }

    /// Tries to allocate in the associated allocator and then places `value` into it.
    /// This doesn’t actually allocate if `T` is zero-sized.
    pub fn try_new(value: T) -> Result<Box<T, A>, StorageError> {
        RawBox::alloc().map(|boxed| Self {
            handle: boxed.write(value),
        })
    }

    /// Tries to allocate uninitialized memory in the associated allocator.
    /// This doesn’t actually allocate if `T` is zero-sized.
    pub fn try_new_uninit() -> Result<Box<MaybeUninit<T>, A>, StorageError> {
        RawBox::alloc().map(|inner| Box { handle: inner })
    }

    /// Unwraps this `Box` into its contained value.
    pub fn into_inner(boxed: Self) -> T {
        boxed.into_handle().into_inner()
    }
}

impl<T, A: AllocatorDefault> Box<[T], A> {
    /// Allocate uninitialized memory in the associated allocator.
    /// This doesn’t actually allocate if `T` is zero-sized.
    pub fn new_uninit_slice(len: usize) -> Box<[MaybeUninit<T>], A> {
        match Self::try_new_uninit_slice(len) {
            Ok(res) => res,
            Err(err) => err.panic(),
        }
    }

    /// Tries to allocate uninitialized memory in the associated allocator.
    /// This doesn’t actually allocate if `T` is zero-sized.
    pub fn try_new_uninit_slice(len: usize) -> Result<Box<[MaybeUninit<T>], A>, StorageError> {
        RawBox::alloc_slice(len, true).map(|inner| Box { handle: inner })
    }
}

impl<T: ?Sized, A: AllocatorDefault> Box<T, A> {
    /// Constructs a box from a raw pointer.
    ///
    /// After calling this function, the raw pointer is owned by the resulting `Box`.
    /// Specifically, the box destructor will call the destructor of `T` and free the
    /// allocated memory.
    ///
    /// # Safety
    /// The memory must have been allocated in accordance with the memory layout used by `Box`.
    pub unsafe fn from_raw(raw: *mut T) -> Self {
        Self::from_raw_in(raw, A::DEFAULT)
    }

    /// Consumes the `Box`, returning a wrapped raw pointer.
    ///
    /// The pointer will be properly aligned and non-null.
    ///
    /// After calling this function, the caller is responsible for the memory
    /// previously managed by the `Box`. In particular, the caller should properly
    /// destroy `T` and release the memory, taking into account the memory layout
    /// used by `Box`. The easiest way to do this is to convert the raw pointer back
    /// into a `Box` with the [`Box::from_raw`] function, allowing the `Box` destructor to
    /// perform the cleanup.
    ///
    /// Note: this is an associated function, which means that you have to call it as
    /// `Box::into_raw(b)` instead of `b.into_raw()`. This is so that there is no conflict
    /// with a method on the inner type.
    pub fn into_raw(boxed: Self) -> *mut T {
        Self::into_raw_with_allocator(boxed).0
    }
}

impl<T, A: Allocator> Box<T, A> {
    /// Allocates in the associated allocation target and then places `value` into it.
    /// This doesn’t actually allocate if `T` is zero-sized.
    pub fn new_in<I>(value: T, alloc_in: I) -> Box<T, A>
    where
        I: AllocateIn<Alloc = A>,
    {
        match Self::try_new_in(value, alloc_in) {
            Ok(slf) => slf,
            Err(e) => e.panic(),
        }
    }

    /// Allocates uninitialized memory in the associated allocation target.
    /// This doesn’t actually allocate if `T` is zero-sized.
    pub fn new_uninit_in<I>(alloc_in: I) -> Box<MaybeUninit<T>, A>
    where
        I: AllocateIn<Alloc = A>,
    {
        match Self::try_new_uninit_in(alloc_in) {
            Ok(uninit) => uninit,
            Err(e) => e.panic(),
        }
    }

    /// Tries to allocate in the associated allocation target and then places `value` into it.
    /// This doesn’t actually allocate if `T` is zero-sized.
    pub fn try_new_in<I>(value: T, alloc_in: I) -> Result<Box<T, A>, StorageError>
    where
        I: AllocateIn<Alloc = A>,
    {
        RawBox::alloc_in(alloc_in).map(|boxed| Self {
            handle: boxed.write(value),
        })
    }

    /// Tries to allocate uninitialized memory in the associated allocation target.
    /// This doesn’t actually allocate if `T` is zero-sized.
    pub fn try_new_uninit_in<I>(alloc_in: I) -> Result<Box<MaybeUninit<T>, A>, StorageError>
    where
        I: AllocateIn<Alloc = A>,
    {
        RawBox::alloc_in(alloc_in).map(|inner| Box { handle: inner })
    }
}

impl<T, A: Allocator> Box<[T], A> {
    /// Allocates uninitialized memory in the associated allocation target.
    /// This doesn’t actually allocate if `T` is zero-sized.
    pub fn new_uninit_slice_in<I>(len: usize, alloc_in: I) -> Box<[MaybeUninit<T>], A>
    where
        I: AllocateIn<Alloc = A>,
    {
        match Self::try_new_uninit_slice_in(len, alloc_in) {
            Ok(res) => res,
            Err(err) => err.panic(),
        }
    }

    /// Tries to allocates uninitialized memory in the associated allocation target.
    /// This doesn’t actually allocate if `T` is zero-sized.
    pub fn try_new_uninit_slice_in<I>(
        len: usize,
        alloc_in: I,
    ) -> Result<Box<[MaybeUninit<T>], A>, StorageError>
    where
        I: AllocateIn<Alloc = A>,
    {
        RawBox::alloc_slice_in(alloc_in, len, true).map(|inner| Box { handle: inner })
    }

    /// Convert this boxed slice into a `Vec<T, A>` without reallocating.
    pub fn into_vec(self) -> Vec<T, A> {
        let (ptr, alloc) = self.into_handle().into_parts();
        let len = ptr.len();
        // SAFETY: a boxed slice has a matching length and capacity. The pointer
        // is a valid allocation for this allocator.
        unsafe { Vec::from_parts(ptr.cast::<T>(), len, len, alloc) }
    }

    fn dangling(alloc: A) -> Box<[T], A> {
        Self {
            handle: RawBox::dangling(alloc),
        }
    }
}

impl<A: Allocator> Box<str, A> {
    /// Convert a boxed slice of bytes into a `Box<str>`.
    ///
    /// If you are sure that the byte slice is valid UTF-8, and you don’t
    /// want to incur the overhead of the validity check, there is an unsafe
    /// version of this function, [`Box::from_utf8_unchecked`], which has the
    /// same behavior but skips the check.
    pub fn from_utf8(boxed: Box<[u8], A>) -> Result<Self, str::Utf8Error> {
        let (ptr, alloc) = Box::into_raw_with_allocator(boxed);
        unsafe {
            // SAFETY: the pointer is guaranteed to be a valid and unaliased.
            let strval = str::from_utf8_mut(&mut *ptr)?;
            // SAFETY: only the type of the pointer has changed. The alignment
            // of `str` is the same as `[u8]` and the allocation size is the same.
            Ok(Self::from_raw_in(strval, alloc))
        }
    }

    /// Convert a boxed slice of bytes into a `Box<str>`.
    ///
    /// # Safety
    /// The contained bytes must be valid UTF-8.
    pub unsafe fn from_utf8_unchecked(boxed: Box<[u8], A>) -> Self {
        let (ptr, alloc) = Box::into_raw_with_allocator(boxed);
        // SAFETY: the pointer is guaranteed to be a valid and unaliased.
        let strval = str::from_utf8_unchecked_mut(&mut *ptr);
        // SAFETY: only the type of the pointer has changed. The alignment
        // of `str` is the same as `[u8]` and the allocation size is the same.
        Self::from_raw_in(strval, alloc)
    }
}

impl<T: Clone, A: AllocatorDefault> Box<[T], A> {
    /// Create a boxed slice by cloning a slice reference.
    pub fn from_slice(data: &[T]) -> Self {
        match Self::try_from_slice(data) {
            Ok(res) => res,
            Err(err) => err.panic(),
        }
    }

    /// Try to create a boxed slice by cloning a slice reference.
    pub fn try_from_slice(data: &[T]) -> Result<Self, StorageError> {
        let len = data.len();
        let handle = RawBox::alloc_slice(len, true)?;
        Ok(Self {
            handle: handle.write_slice(|insert| {
                insert.push_slice(data);
            }),
        })
    }
}

impl<T: Clone, A: Allocator> Box<[T], A> {
    /// Create a boxed slice directly in an allocation target by cloning a slice reference.
    pub fn from_slice_in<I>(data: &[T], alloc_in: I) -> Self
    where
        I: AllocateIn<Alloc = A>,
    {
        match Self::try_from_slice_in(data, alloc_in) {
            Ok(res) => res,
            Err(err) => err.panic(),
        }
    }

    /// Try to create a boxed slice directly in an allocation target by cloning a slice reference.
    pub fn try_from_slice_in<I>(data: &[T], alloc_in: I) -> Result<Self, StorageError>
    where
        I: AllocateIn<Alloc = A>,
    {
        let len = data.len();
        let handle = RawBox::alloc_slice_in(alloc_in, len, true)?;
        Ok(Self {
            handle: handle.write_slice(|insert| {
                insert.push_slice(data);
            }),
        })
    }
}

impl<T: ?Sized, A: Allocator> Box<T, A> {
    /// Obtain a reference to the contained allocator instance.
    pub fn allocator(&self) -> &A {
        self.handle.allocator()
    }

    /// Get a read pointer to the beginning of the data allocation. This may be a
    /// dangling pointer if `T` is zero sized or the current capacity is zero.
    #[inline]
    pub fn as_ptr(&self) -> *const T {
        self.handle.as_ptr()
    }

    /// Get a mutable pointer to the beginning of the data allocation. This may be a
    /// dangling pointer if `T` is zero sized or the current capacity is zero.
    #[inline]
    pub fn as_mut_ptr(&mut self) -> *mut T {
        self.handle.as_mut_ptr()
    }

    /// Constructs a box from a raw pointer and an allocator instance.
    ///
    /// After calling this function, the raw pointer is owned by the resulting `Box`.
    /// Specifically, the box destructor will call the destructor of `T` and free the
    /// allocated memory.
    ///
    /// # Safety
    /// The memory must have been allocated in accordance with the memory layout used by `Box`.
    pub unsafe fn from_raw_in(raw: *mut T, alloc: A) -> Self {
        let ptr = NonNull::new(raw).expect("from_raw: pointer must not be null");
        Self {
            handle: RawBox::from_parts(ptr, alloc),
        }
    }

    /// Consumes the `Box`, returning a wrapped raw pointer and an allocator instance.
    ///
    /// The pointer will be properly aligned and non-null.
    ///
    /// After calling this function, the caller is responsible for the memory
    /// previously managed by the `Box`. In particular, the caller should properly
    /// destroy `T` and release the memory, taking into account the memory layout
    /// used by `Box`. The easiest way to do this is to convert the raw pointer back
    /// into a `Box` with the [`Box::from_raw_in`] function, allowing the `Box` destructor
    /// to perform the cleanup.
    ///
    /// Note: this is an associated function, which means that you have to call it as
    /// `Box::into_raw_with_allocator(b)` instead of `b.into_raw_with_allocator()`. This is
    /// so that there is no conflict with a method on the inner type.
    pub fn into_raw_with_allocator(boxed: Self) -> (*mut T, A) {
        let (ptr, alloc) = boxed.into_handle().into_parts();
        (ptr.as_ptr(), alloc)
    }

    /// Consumes and leaks the `Box`, returning a mutable reference, `&'a mut T`.
    ///
    /// Note that the type `T` must outlive the chosen lifetime `'a`. If the type has
    /// only static references, or none at all, then this may be chosen to be `'static`.
    ///
    /// This function is mainly useful for data that lives for the remainder of the program's
    /// life. Dropping the returned reference will cause a memory leak. If this is not
    /// acceptable, the reference should first be wrapped with the `Box::from_raw` function
    /// producing a `Box`. This `Box` can then be dropped which will properly destroy `T` and
    /// release the allocated memory.
    ///
    /// Note: this is an associated function, which means that you have to call it as
    /// `Box::leak(b)` instead of `b.leak()`. This is so that there is no conflict with a
    /// method on the inner type.
    pub fn leak<'a>(boxed: Self) -> &'a mut T
    where
        A: 'a,
    {
        boxed.into_handle().leak()
    }

    #[inline]
    pub(crate) fn into_handle(self) -> RawBox<T, A> {
        // SAFETY: this simply extracts the handle without running
        // the `Drop` implementation for this `Box`. It is safe to
        // read from a pointer derived from a reference and the
        // aliasing rules are not violated.
        unsafe { ptr::read(&ManuallyDrop::new(self).handle) }
    }
}

impl<T, A: Allocator> Box<MaybeUninit<T>, A> {
    /// Converts to `Box<T, A>`.
    ///
    /// # Safety
    /// The contents of the box must be initialized prior to calling, or else
    /// undefined behavior may result from the use of uninitialized memory.
    #[inline]
    pub unsafe fn assume_init(self) -> Box<T, A> {
        Box {
            handle: self.into_handle().assume_init(),
        }
    }

    /// Writes the value and converts to `Box<T, A>`.
    ///
    /// This method converts the box similarly to `Box::assume_init` but writes value
    /// into it before conversion, thus guaranteeing safety. In some scenarios use of
    /// this method may improve performance because the compiler may be able to optimize
    /// copying from stack.
    #[inline(always)]
    pub fn write(boxed: Self, value: T) -> Box<T, A> {
        Box {
            handle: boxed.into_handle().write(value),
        }
    }
}

impl<T, A: Allocator> Box<[MaybeUninit<T>], A> {
    /// Converts to `Box<[T], A>`.
    ///
    /// # Safety
    /// The contents of the box must be initialized prior to calling, or else
    /// undefined behavior may result from the use of uninitialized memory.
    #[inline]
    pub unsafe fn assume_init(self) -> Box<[T], A> {
        Box {
            handle: self.into_handle().assume_init(),
        }
    }
}

impl<T, A: Allocator, const N: usize> Box<[T; N], A> {
    /// Converts a `Box<T, A>` into a `Box<[T], A>`.
    ///
    /// This conversion does not allocate on the heap and happens in place.
    pub fn into_boxed_slice(boxed: Self) -> Box<[T], A> {
        let (ptr, alloc) = boxed.into_handle().into_parts();
        Box {
            handle: RawBox::from_parts(NonNull::slice_from_raw_parts(ptr.cast::<T>(), N), alloc),
        }
    }
}

impl<T: ?Sized, A: Allocator> AsRef<T> for Box<T, A> {
    fn as_ref(&self) -> &T {
        self.handle.as_ref()
    }
}

impl<T: ?Sized, A: Allocator> AsMut<T> for Box<T, A> {
    fn as_mut(&mut self) -> &mut T {
        self.handle.as_mut()
    }
}

impl<T: ?Sized, A: Allocator> borrow::Borrow<T> for Box<T, A> {
    fn borrow(&self) -> &T {
        self.handle.as_ref()
    }
}

impl<T: ?Sized, A: Allocator> borrow::BorrowMut<T> for Box<T, A> {
    fn borrow_mut(&mut self) -> &mut T {
        self.handle.as_mut()
    }
}

impl<T: Clone, A: Allocator + Clone> Clone for Box<T, A> {
    fn clone(&self) -> Self {
        let boxed = Self::new_uninit_in(self.allocator().clone());
        Box::write(boxed, self.as_ref().clone())
    }
}

impl<T: Clone, A: Allocator + Clone> Clone for Box<[T], A> {
    fn clone(&self) -> Self {
        Self::from_slice_in(self.as_ref(), self.allocator().clone())
    }
}

impl<A: Allocator + Clone> Clone for Box<str, A> {
    fn clone(&self) -> Self {
        let boxed = Box::<[u8], A>::from_slice_in(self.as_bytes(), self.allocator().clone());
        // SAFETY: the Box contents are guaranteed to be valid UTF-8 data.
        unsafe { Box::from_utf8_unchecked(boxed) }
    }
}

#[cfg(feature = "nightly")]
impl<T: ?Sized + core::marker::Unsize<U>, U: ?Sized, A: Allocator>
    core::ops::CoerceUnsized<Box<U, A>> for Box<T, A>
{
}

impl<T: ?Sized + fmt::Debug, A: Allocator> fmt::Debug for Box<T, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl<T: Default, A: AllocatorDefault> Default for Box<T, A> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T, A: AllocatorDefault> Default for Box<[T], A> {
    fn default() -> Self {
        Self::dangling(A::DEFAULT)
    }
}

impl<A: AllocatorDefault> Default for Box<str, A> {
    fn default() -> Self {
        // SAFETY: an empty (dangling) Box is valid UTF-8.
        unsafe { Box::from_utf8_unchecked(Box::dangling(A::DEFAULT)) }
    }
}

impl<T: ?Sized, A: Allocator> Deref for Box<T, A> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.handle.as_ref()
    }
}

impl<T: ?Sized, A: Allocator> DerefMut for Box<T, A> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.handle.as_mut()
    }
}

impl<T: ?Sized, A: Allocator> Drop for Box<T, A> {
    fn drop(&mut self) {
        unsafe {
            ptr::drop_in_place(self.handle.as_mut());
        }
    }
}

impl<T, A: AllocatorDefault> From<T> for Box<T, A> {
    #[inline]
    fn from(value: T) -> Self {
        Box::write(Self::new_uninit(), value)
    }
}

impl<T: Clone, A: AllocatorDefault> From<&[T]> for Box<[T], A> {
    fn from(data: &[T]) -> Self {
        Self::from_slice(data)
    }
}

impl<T, A: AllocatorDefault, const N: usize> From<[T; N]> for Box<[T], A> {
    fn from(data: [T; N]) -> Self {
        Box::into_boxed_slice(Box::new(data))
    }
}

impl<A: AllocatorDefault> From<&str> for Box<str, A> {
    fn from(data: &str) -> Self {
        let boxed = Box::from_slice(data.as_bytes());
        // SAFETY: the Box contents are guaranteed to be valid UTF-8 data.
        unsafe { Self::from_utf8_unchecked(boxed) }
    }
}

impl<T, C> From<Vec<T, C>> for Box<[T], C::Alloc>
where
    C: VecConfigAlloc<T>,
{
    fn from(vec: Vec<T, C>) -> Self {
        vec.into_boxed_slice()
    }
}

impl<T, A: AllocatorDefault> FromIterator<T> for Box<[T], A> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Vec::<T, A>::from_iter(iter).into_boxed_slice()
    }
}

impl<T, A: Allocator, const N: usize> TryFrom<Box<[T], A>> for Box<[T; N], A> {
    type Error = Box<[T], A>;

    fn try_from(boxed: Box<[T], A>) -> Result<Self, Self::Error> {
        if boxed.len() == N {
            Ok(Self {
                handle: unsafe { boxed.into_handle().cast() },
            })
        } else {
            Err(boxed)
        }
    }
}

impl<T, A: Allocator, const N: usize> TryFrom<Vec<T, A>> for Box<[T; N], A> {
    type Error = Vec<T, A>;

    fn try_from(vec: Vec<T, A>) -> Result<Self, Self::Error> {
        if vec.len() == N {
            let boxed = vec.into_boxed_slice();
            Ok(Self {
                handle: unsafe { boxed.into_handle().cast() },
            })
        } else {
            Err(vec)
        }
    }
}

#[cfg(all(feature = "alloc", not(feature = "nightly")))]
impl<T: ?Sized> ConvertAlloc<Box<T, Global>> for alloc_crate::boxed::Box<T> {
    fn convert(self) -> Box<T, Global> {
        let raw = alloc_crate::boxed::Box::into_raw(self);
        unsafe { Box::from_raw(raw) }
    }
}

#[cfg(all(feature = "alloc", feature = "nightly"))]
impl<T: ?Sized, A: Allocator> ConvertAlloc<Box<T, A>> for alloc_crate::boxed::Box<T, A> {
    fn convert(self) -> Box<T, A> {
        let (raw, alloc) = alloc_crate::boxed::Box::into_raw_with_allocator(self);
        unsafe { Box::from_raw_in(raw, alloc) }
    }
}

#[cfg(all(feature = "alloc", not(feature = "nightly")))]
impl<T: ?Sized> ConvertAlloc<alloc_crate::boxed::Box<T>> for Box<T, Global> {
    fn convert(self) -> alloc_crate::boxed::Box<T> {
        let raw = Box::into_raw(self);
        unsafe { alloc_crate::boxed::Box::from_raw(raw) }
    }
}

#[cfg(all(feature = "alloc", feature = "nightly"))]
impl<T: ?Sized, A: Allocator> ConvertAlloc<alloc_crate::boxed::Box<T, A>> for Box<T, A> {
    fn convert(self) -> alloc_crate::boxed::Box<T, A> {
        let (raw, alloc) = Box::into_raw_with_allocator(self);
        unsafe { alloc_crate::boxed::Box::from_raw_in(raw, alloc) }
    }
}

impl<T: ?Sized + PartialEq, A: Allocator> PartialEq for Box<T, A> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        PartialEq::eq(self.as_ref(), other.as_ref())
    }
}

impl<T: ?Sized + Eq, A: Allocator> Eq for Box<T, A> {}

impl<T: ?Sized + PartialOrd, A: Allocator> PartialOrd for Box<T, A> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        PartialOrd::partial_cmp(&**self, &**other)
    }
}

impl<T: ?Sized + Ord, A: Allocator> Ord for Box<T, A> {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        Ord::cmp(&**self, &**other)
    }
}

unsafe impl<T: Send + ?Sized, A: Allocator + Send> Send for Box<T, A> {}
unsafe impl<T: Sync + ?Sized, A: Allocator + Sync> Sync for Box<T, A> {}

impl<T: ?Sized, A: Allocator> Unpin for Box<T, A> {}

#[cfg(feature = "zeroize")]
impl<T: ?Sized + Zeroize, A: Allocator> Zeroize for Box<T, A> {
    fn zeroize(&mut self) {
        self.as_mut().zeroize()
    }
}

#[cfg(feature = "zeroize")]
impl<T: ?Sized, A: crate::alloc::AllocatorZeroizes> ZeroizeOnDrop for Box<T, A> {}

#[cfg(test)]
mod tests {
    #[cfg(all(feature = "alloc", feature = "nightly"))]
    #[test]
    fn box_unsized() {
        use core::any::Any;

        use super::Box;

        let _ = Box::new(10usize) as Box<dyn Any>;
    }
}
