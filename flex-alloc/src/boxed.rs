//! Support for placing values within allocated memory.

use core::alloc::Layout;
use core::any::type_name;
use core::mem::{size_of, ManuallyDrop, MaybeUninit};
use core::ops::{Deref, DerefMut};
use core::ptr::NonNull;
use core::{fmt, ptr};

#[cfg(feature = "zeroize")]
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::alloc::{AllocateIn, Allocator, AllocatorDefault, Global};
use crate::vec::{insert::Inserter, Vec};
use crate::StorageError;

#[cfg(feature = "alloc")]
use crate::alloc::ConvertAlloc;

pub(crate) struct RawBox<T: ?Sized, A: Allocator> {
    ptr: NonNull<T>,
    alloc: A,
}

impl<T: ?Sized, A: Allocator> RawBox<T, A> {
    pub fn allocator(&self) -> &A {
        &self.alloc
    }

    /// Get a read pointer to the beginning of the data allocation. This may be a
    /// dangling pointer if `T` is zero sized or the current capacity is zero.
    #[inline]
    pub fn as_ptr(&mut self) -> *mut T {
        self.ptr.as_ptr()
    }

    #[inline]
    pub fn from_parts(ptr: NonNull<T>, alloc: A) -> Self {
        Self { ptr, alloc }
    }

    #[inline]
    pub fn into_parts(self) -> (NonNull<T>, A) {
        let slf = ManuallyDrop::new(self);
        (slf.ptr, unsafe { ptr::read(&slf.alloc) })
    }

    #[inline]
    pub fn layout(&self) -> Layout {
        Layout::for_value(unsafe { self.ptr.as_ref() })
    }
}

impl<T, A: Allocator> RawBox<T, A> {
    #[inline]
    pub fn alloc_in<I>(target: I) -> Result<RawBox<MaybeUninit<T>, A>, StorageError>
    where
        I: AllocateIn<Alloc = A>,
    {
        let (ptr, alloc) = target.allocate_in(Layout::new::<T>())?;
        Ok(RawBox {
            ptr: ptr.cast(),
            alloc,
        })
    }

    #[inline]
    pub fn alloc_slice_in<I>(
        target: I,
        mut len: usize,
        exact: bool,
    ) -> Result<RawBox<[MaybeUninit<T>], A>, StorageError>
    where
        I: AllocateIn<Alloc = A>,
    {
        let layout = Layout::array::<T>(len)?;
        let (ptr, alloc) = target.allocate_in(layout)?;
        if !exact {
            len = ptr.len() / size_of::<T>();
        }
        Ok(RawBox {
            ptr: unsafe {
                NonNull::new_unchecked(ptr::slice_from_raw_parts_mut(ptr.as_ptr().cast(), len))
            },
            alloc,
        })
    }
}

impl<T, A: AllocatorDefault> RawBox<T, A> {
    #[inline]
    pub fn alloc() -> Result<RawBox<MaybeUninit<T>, A>, StorageError> {
        Self::alloc_in(A::DEFAULT)
    }

    #[inline]
    pub fn alloc_slice(
        len: usize,
        exact: bool,
    ) -> Result<RawBox<[MaybeUninit<T>], A>, StorageError> {
        Self::alloc_slice_in(A::DEFAULT, len, exact)
    }
}

impl<T, A: Allocator> RawBox<[T], A> {
    #[inline]
    pub fn dangling(alloc: A) -> Self {
        Self {
            ptr: {
                unsafe {
                    NonNull::new_unchecked(ptr::slice_from_raw_parts_mut(
                        NonNull::<T>::dangling().as_ptr(),
                        0,
                    ))
                }
            },
            alloc,
        }
    }
}

impl<T, A: Allocator> RawBox<MaybeUninit<T>, A> {
    /// # Safety
    /// The contents of the box must be initialized prior to calling, or else
    /// undefined behavior may result from the use of uninitialized memory.
    #[inline]
    pub unsafe fn assume_init(self) -> RawBox<T, A> {
        let (ptr, alloc) = self.into_parts();
        RawBox {
            ptr: ptr.cast(),
            alloc,
        }
    }

    #[inline(always)]
    pub fn write(self, value: T) -> RawBox<T, A> {
        unsafe {
            self.ptr.as_ptr().write(MaybeUninit::new(value));
            self.assume_init()
        }
    }
}

impl<T, A: Allocator> RawBox<[MaybeUninit<T>], A> {
    /// # Safety
    /// The contents of the box must be initialized prior to calling, or else
    /// undefined behavior may result from the use of uninitialized memory.
    #[inline]
    pub unsafe fn assume_init(self) -> RawBox<[T], A> {
        let (ptr, alloc) = self.into_parts();
        let len = ptr.len();
        RawBox {
            ptr: unsafe {
                NonNull::new_unchecked(ptr::slice_from_raw_parts_mut(ptr.as_ptr().cast(), len))
            },
            alloc,
        }
    }
}

impl<T: ?Sized, A: Allocator> AsRef<T> for RawBox<T, A> {
    #[inline]
    fn as_ref(&self) -> &T {
        unsafe { self.ptr.as_ref() }
    }
}

impl<T: ?Sized, A: Allocator> AsMut<T> for RawBox<T, A> {
    #[inline]
    fn as_mut(&mut self) -> &mut T {
        unsafe { self.ptr.as_mut() }
    }
}

impl<T: ?Sized, A: Allocator> Drop for RawBox<T, A> {
    fn drop(&mut self) {
        unsafe {
            self.alloc.deallocate(self.ptr.cast(), self.layout());
        }
    }
}

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
        let handle = boxed.into_handle();
        unsafe { handle.ptr.as_ptr().read() }
    }
}

impl<T, A: AllocatorDefault> Box<[T], A> {
    /// Allocate uninitialized memory in the associated allocator.
    /// This doesn’t actually allocate if `T` is zero-sized.
    pub fn new_uninit_slice(len: usize) -> Box<[MaybeUninit<T>], A> {
        Self::try_new_uninit_slice(len).expect("Allocation error")
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
        Self::try_new_uninit_slice_in(len, alloc_in).expect("Allocation error")
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
        unsafe { Vec::from_parts(ptr.cast(), ptr.len(), ptr.len(), alloc) }
    }

    fn dangling(alloc: A) -> Box<[T], A> {
        Self {
            handle: RawBox::dangling(alloc),
        }
    }
}

impl<T: Clone, A: AllocatorDefault> Box<[T], A> {
    /// Create a boxed slice by cloning a slice reference.
    pub fn from_slice(data: &[T]) -> Self {
        Self::try_from_slice(data).expect("Allocation error")
    }

    /// Try to create a boxed slice by cloning a slice reference.
    pub fn try_from_slice(data: &[T]) -> Result<Self, StorageError> {
        let len = data.len();
        let mut buf = Self::try_new_uninit_slice(len)?;
        let mut insert = Inserter::for_uninit_slice(buf.handle.as_mut());
        for value in data {
            insert.push_clone(value);
        }
        insert.complete();
        Ok(unsafe { buf.assume_init() })
    }
}

impl<T: Clone, A: Allocator> Box<[T], A> {
    /// Create a boxed slice directly in an allocation target by cloning a slice reference.
    pub fn from_slice_in<I>(data: &[T], alloc_in: I) -> Self
    where
        I: AllocateIn<Alloc = A>,
    {
        Self::try_from_slice_in(data, alloc_in).expect("Allocation error")
    }

    /// Try to create a boxed slice directly in an allocation target by cloning a slice reference.
    pub fn try_from_slice_in<I>(data: &[T], alloc_in: I) -> Result<Self, StorageError>
    where
        I: AllocateIn<Alloc = A>,
    {
        let len = data.len();
        let mut buf = Self::try_new_uninit_slice_in(len, alloc_in)?;
        let mut insert = Inserter::for_uninit_slice(buf.handle.as_mut());
        for value in data {
            insert.push_clone(value);
        }
        insert.complete();
        Ok(unsafe { buf.assume_init() })
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
    pub fn as_ptr(&mut self) -> *const T {
        self.handle.as_ptr() as *const T
    }

    /// Get a mutable pointer to the beginning of the data allocation. This may be a
    /// dangling pointer if `T` is zero sized or the current capacity is zero.
    #[inline]
    pub fn as_mut_ptr(&mut self) -> *mut T {
        self.handle.as_ptr()
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
        let (ptr, _alloc) = Self::into_raw_with_allocator(boxed);
        unsafe { &mut *ptr }
    }

    #[inline]
    fn into_handle(self) -> RawBox<T, A> {
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
    pub fn write(self, value: T) -> Box<T, A> {
        Box {
            handle: self.into_handle().write(value),
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

impl<T: Clone, A: Allocator + Clone> Clone for Box<T, A> {
    fn clone(&self) -> Self {
        let boxed = Self::new_uninit_in(self.allocator().clone());
        boxed.write(self.as_ref().clone())
    }
}

impl<T: Clone, A: Allocator + Clone> Clone for Box<[T], A> {
    fn clone(&self) -> Self {
        Self::from_slice_in(self.as_ref(), self.allocator().clone())
    }
}

impl<T: ?Sized, A: Allocator> fmt::Debug for Box<T, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("SecureBox<{}>", type_name::<T>()))
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

impl<T: Clone, A: AllocatorDefault> From<&T> for Box<T, A> {
    fn from(value: &T) -> Self {
        Self::new_uninit().write(value.clone())
    }
}

impl<T: Clone, A: AllocatorDefault> From<&[T]> for Box<[T], A> {
    fn from(data: &[T]) -> Self {
        Self::from_slice(data)
    }
}

impl<T, A: Allocator> From<Vec<T, A>> for Box<[T], A> {
    fn from(vec: Vec<T, A>) -> Self {
        vec.into_boxed_slice()
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

impl<T: ?Sized, A: Allocator> Drop for Box<T, A> {
    fn drop(&mut self) {
        unsafe {
            ptr::drop_in_place(self.handle.as_mut());
        }
    }
}

unsafe impl<T: Send + ?Sized, A: Allocator + Send> Send for Box<T, A> {}
unsafe impl<T: Sync + ?Sized, A: Allocator + Sync> Sync for Box<T, A> {}

#[cfg(feature = "zeroize")]
impl<T: ?Sized + Zeroize, A: Allocator> Zeroize for Box<T, A> {
    fn zeroize(&mut self) {
        self.as_mut().zeroize()
    }
}

#[cfg(feature = "zeroize")]
impl<T: ?Sized, A: crate::alloc::AllocatorZeroizes> ZeroizeOnDrop for Box<T, A> {}
