use core::alloc::Layout;
use core::mem::{size_of, ManuallyDrop, MaybeUninit};
use core::ptr;
use core::ptr::NonNull;

use super::insert::Inserter;
use crate::alloc::{AllocateIn, Allocator, AllocatorDefault};
use crate::error::StorageError;

pub(crate) struct RawBox<T: ?Sized, A: Allocator> {
    ptr: NonNull<T>,
    alloc: A,
}

impl<T: ?Sized, A: Allocator> RawBox<T, A> {
    pub fn allocator(&self) -> &A {
        &self.alloc
    }

    /// Get a const pointer to the beginning of the data allocation. This may be a
    /// dangling pointer if `T` is zero sized or the current capacity is zero.
    #[inline]
    pub fn as_ptr(&self) -> *const T {
        self.ptr.as_ptr()
    }

    /// Get a mutable pointer to the beginning of the data allocation. This may be a
    /// dangling pointer if `T` is zero sized or the current capacity is zero.
    #[inline]
    pub fn as_mut_ptr(&self) -> *mut T {
        self.ptr.as_ptr()
    }

    #[inline]
    pub fn from_parts(ptr: NonNull<T>, alloc: A) -> Self {
        Self { ptr, alloc }
    }

    #[inline]
    pub fn into_parts(self) -> (NonNull<T>, A) {
        let slf = ManuallyDrop::new(self);
        // SAFETY: the pointer given to `ptr::read` is produced from a reference,
        // and so must be properly aligned and point to an initialized value
        (slf.ptr, unsafe { ptr::read(&slf.alloc) })
    }

    #[inline]
    pub fn into_inner(self) -> T
    where
        T: Sized,
    {
        // The allocation will be dropped without running the drop handler
        // for the contained value.
        // SAFETY: the value pointed at by `self.ptr` is always properly initialized.
        unsafe { self.ptr.as_ptr().read() }
    }

    #[inline]
    pub fn layout(&self) -> Layout {
        // SAFETY: the value pointed at by `self.ptr` is always properly initialized.
        // Uninitialized values would use a type of `MaybeUninit<T>`.
        Layout::for_value(unsafe { self.ptr.as_ref() })
    }

    #[inline]
    pub fn leak<'a>(self) -> &'a mut T
    where
        A: 'a,
    {
        let (mut raw, _alloc) = self.into_parts();
        // SAFETY: the value pointed at is guaranteed to be initialized and
        // properly aligned.
        unsafe { raw.as_mut() }
    }

    #[inline]
    pub unsafe fn cast<U>(self) -> RawBox<U, A> {
        let (ptr, alloc) = self.into_parts();
        RawBox::from_parts(
            // SAFETY: the pointer value is derived from a NonNull, and so
            // it must be non-null.
            unsafe { NonNull::new_unchecked(ptr.as_ptr().cast()) },
            alloc,
        )
    }
}

impl<T, A: Allocator> RawBox<T, A> {
    #[inline]
    pub fn alloc_in<I>(target: I) -> Result<RawBox<MaybeUninit<T>, A>, StorageError>
    where
        I: AllocateIn<Alloc = A>,
    {
        let layout = Layout::new::<T>();
        let (ptr, alloc) = target
            .allocate_in(layout)
            .map_err(|_| StorageError::AllocError(layout))?;
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
        let (ptr, alloc) = target
            .allocate_in(layout)
            .map_err(|_| StorageError::AllocError(layout))?;
        if !exact {
            len = ptr.len() / size_of::<T>();
        }
        Ok(RawBox {
            ptr: NonNull::slice_from_raw_parts(ptr.cast::<MaybeUninit<T>>(), len),
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
    /// Create a dangling slice pointer, ie. a pointer to an empty slice,
    /// but with proper alignment.
    pub fn dangling(alloc: A) -> Self {
        Self {
            ptr: NonNull::slice_from_raw_parts(NonNull::<T>::dangling(), 0),
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
            // SAFETY: `write` is always safe for a `MaybeUninit`. The pointer
            // itself is guaranteed to have the proper alignment and allocated
            // size.
            self.ptr.as_ptr().write(MaybeUninit::new(value));
            // SAFETY: The value has been initialized above.
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
        RawBox {
            ptr: NonNull::slice_from_raw_parts(ptr.cast::<T>(), ptr.len()),
            alloc,
        }
    }

    #[inline]
    pub fn write_slice(mut self, f: impl FnOnce(&mut Inserter<T>)) -> RawBox<[T], A> {
        let mut insert = Inserter::new(self.as_mut());
        f(&mut insert);
        let count = insert.complete();
        assert_eq!(count, self.as_ref().len());
        // SAFETY: the slice contents have been written.
        unsafe { self.assume_init() }
    }
}

impl<T: ?Sized, A: Allocator> AsRef<T> for RawBox<T, A> {
    #[inline]
    fn as_ref(&self) -> &T {
        // SAFETY: the value pointed at by `self.ptr` is guaranteed to be initialized.
        unsafe { self.ptr.as_ref() }
    }
}

impl<T: ?Sized, A: Allocator> AsMut<T> for RawBox<T, A> {
    #[inline]
    fn as_mut(&mut self) -> &mut T {
        // SAFETY: the value pointed at by `self.ptr` is guaranteed to be initialized.
        unsafe { self.ptr.as_mut() }
    }
}

#[cfg(feature = "nightly")]
impl<T: ?Sized + core::marker::Unsize<U>, U: ?Sized, A: Allocator>
    core::ops::CoerceUnsized<RawBox<U, A>> for RawBox<T, A>
{
}

impl<T: ?Sized, A: Allocator> Drop for RawBox<T, A> {
    fn drop(&mut self) {
        // SAFETY: the value pointed at by `self.ptr` is guaranteed to be
        // a valid allocation from this allocator. The layout must be smaller
        // than or equal to the original requested layout, having the same alignment.
        unsafe {
            self.alloc.deallocate(self.ptr.cast(), self.layout());
        }
    }
}
