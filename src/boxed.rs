use core::alloc::Layout;
use core::any::Any;
use core::borrow::{Borrow, BorrowMut};
use core::cmp::Ordering;
use core::fmt;
use core::mem::{ManuallyDrop, MaybeUninit};
use core::ops::{Deref, DerefMut};
use core::pin::Pin;
use core::ptr::{self, NonNull};
use core::str;

use crate::error::StorageError;
use crate::storage::{Global, RawAlloc, RawAllocIn, RawAllocNew};
use crate::vec::insert::Inserter;
use crate::vec::Vec;

pub struct Box<T: ?Sized, A: RawAlloc = Global> {
    data: NonNull<T>,
    alloc: A,
}

impl<T: ?Sized, A: RawAlloc> Box<T, A> {
    pub(crate) unsafe fn from_parts(data: NonNull<T>, alloc: A) -> Self {
        Self { data, alloc }
    }

    #[inline]
    pub(crate) fn into_parts(self) -> (NonNull<T>, A) {
        let boxed = ManuallyDrop::new(self);
        (boxed.data, unsafe { ptr::read(&boxed.alloc) })
    }

    #[inline]
    pub unsafe fn from_raw_in(data: *mut T, alloc: A) -> Self {
        Self::from_parts(NonNull::new_unchecked(data), alloc)
    }

    #[inline]
    pub fn into_raw(boxed: Self) -> *mut T {
        let (data, _alloc) = boxed.into_parts();
        data.as_ptr()
    }

    #[inline]
    pub fn into_raw_with_allocator(boxed: Self) -> (*mut T, A) {
        let (data, alloc) = boxed.into_parts();
        (data.as_ptr(), alloc)
    }

    #[inline]
    pub(crate) unsafe fn transmute<R>(boxed: Self) -> Box<R, A> {
        let (data, alloc) = Self::into_parts(boxed);
        Box::from_parts(data.cast(), alloc)
    }
}

impl<T, A: RawAlloc> Box<T, A> {
    #[inline]
    pub fn new_uninit_in<I>(alloc_in: I) -> Box<MaybeUninit<T>, A>
    where
        I: RawAllocIn<RawAlloc = A>,
    {
        match alloc_in.try_alloc_in(Layout::new::<MaybeUninit<T>>()) {
            Ok((data, alloc)) => unsafe { Box::from_parts(data.cast(), alloc) },
            Err(err) => err.panic(),
        }
    }

    #[inline]
    pub fn new_in<I>(value: T, alloc_in: I) -> Self
    where
        I: RawAllocIn<RawAlloc = A>,
    {
        let boxed = Self::new_uninit_in(alloc_in);
        Box::write(boxed, value)
    }

    #[inline]
    pub fn try_new_in<I>(value: T, alloc_in: I) -> Result<Self, StorageError>
    where
        I: RawAllocIn<RawAlloc = A>,
    {
        let boxed = Self::try_new_uninit_in(alloc_in)?;
        Ok(Box::write(boxed, value))
    }

    pub fn try_new_uninit_in<I>(alloc_in: I) -> Result<Box<MaybeUninit<T>, A>, StorageError>
    where
        I: RawAllocIn<RawAlloc = A>,
    {
        let (data, alloc) = alloc_in.try_alloc_in(Layout::new::<MaybeUninit<T>>())?;
        Ok(unsafe { Box::from_parts(data.cast(), alloc) })
    }

    #[inline]
    pub fn into_inner(boxed: Self) -> T {
        unsafe { Box::transmute::<MaybeUninit<T>>(boxed).as_ptr().read() }
    }

    #[inline]
    pub fn into_boxed_slice(boxed: Self) -> Box<[T], A> {
        let (data, alloc) = Self::into_parts(boxed);
        unsafe { Box::slice_from_parts(data, alloc, 1) }
    }

    #[inline]
    pub fn leak<'a>(boxed: Self) -> &'a mut T
    where
        A: 'a,
    {
        let (mut data, _alloc) = Self::into_parts(boxed);
        unsafe { data.as_mut() }
    }

    #[inline]
    pub fn into_pin(boxed: Self) -> Pin<Self> {
        unsafe { Pin::new_unchecked(boxed) }
    }

    #[inline]
    pub fn pin_in<I>(value: T, alloc_in: I) -> Pin<Self>
    where
        I: RawAllocIn<RawAlloc = A>,
    {
        Pin::from(Self::new_in(value, alloc_in))
    }
}

impl<T, A: RawAllocNew> Box<T, A> {
    #[inline]
    pub fn new(value: T) -> Self {
        Self::new_in(value, A::NEW)
    }

    #[inline]
    pub fn new_uninit() -> Box<MaybeUninit<T>, A> {
        Self::new_uninit_in(A::NEW)
    }

    #[inline]
    pub unsafe fn from_raw(data: *mut T) -> Self {
        Self::from_parts(NonNull::new_unchecked(data), A::NEW)
    }

    #[inline]
    pub fn try_new(value: T) -> Result<Self, StorageError> {
        Self::try_new_in(value, A::NEW)
    }

    #[inline]
    pub fn try_new_uninit() -> Result<Box<MaybeUninit<T>, A>, StorageError> {
        Self::try_new_uninit_in(A::NEW)
    }

    #[inline]
    pub fn pin(value: T) -> Pin<Self> {
        Pin::from(Self::new(value))
    }
}

impl<T, A: RawAlloc> Box<[T], A> {
    #[inline]
    pub(crate) unsafe fn slice_from_parts(data: NonNull<T>, alloc: A, length: usize) -> Self {
        let data = ptr::slice_from_raw_parts_mut(data.as_ptr().cast(), length);
        Self::from_parts(NonNull::new_unchecked(data), alloc)
    }

    #[inline]
    pub fn new_uninit_slice_in<I>(length: usize, alloc_in: I) -> Box<[MaybeUninit<T>], A>
    where
        I: RawAllocIn<RawAlloc = A>,
    {
        match Self::try_new_uninit_slice_in(length, alloc_in) {
            Ok(boxed) => boxed,
            Err(err) => err.panic(),
        }
    }

    pub fn try_new_uninit_slice_in<I>(
        length: usize,
        alloc_in: I,
    ) -> Result<Box<[MaybeUninit<T>], A>, StorageError>
    where
        I: RawAllocIn<RawAlloc = A>,
    {
        let layout = Layout::array::<MaybeUninit<T>>(length)?;
        let (data, alloc) = alloc_in.try_alloc_in(layout)?;
        Ok(unsafe { Box::slice_from_parts(data.cast(), alloc, length) })
    }

    #[inline]
    pub fn into_vec(self) -> Vec<T, A> {
        self.into()
    }
}

impl<T, A: RawAllocNew> Box<[T], A> {
    #[inline]
    pub fn new_uninit_slice(length: usize) -> Box<[MaybeUninit<T>], A> {
        Self::new_uninit_slice_in(length, A::NEW)
    }

    #[inline]
    pub fn try_new_uninit_slice(length: usize) -> Result<Box<[MaybeUninit<T>], A>, StorageError> {
        Self::try_new_uninit_slice_in(length, A::NEW)
    }

    #[inline]
    fn copy_slice(data: &[T]) -> Self
    where
        T: Copy,
    {
        let mut boxed = Self::new_uninit_slice(data.len());
        let buf = &mut *boxed;
        for idx in 0..data.len() {
            buf[idx].write(data[idx]);
        }
        unsafe { boxed.assume_init() }
    }
}

impl<T, A: RawAlloc, const N: usize> Box<[T; N], A> {
    #[inline]
    pub fn slice(boxed: Self) -> Box<[T], A> {
        let (data, alloc) = boxed.into_parts();
        unsafe { Box::slice_from_parts(data.cast(), alloc, N) }
    }

    #[inline]
    pub fn into_vec(self) -> Vec<T, A> {
        Box::slice(self).into()
    }
}

impl<A: RawAlloc> Box<str, A> {
    pub fn from_utf8(boxed: Box<[u8], A>) -> Result<Self, str::Utf8Error> {
        let (ptr, alloc) = Box::into_raw_with_allocator(boxed);
        unsafe {
            let strval = str::from_utf8_mut(&mut *ptr)?;
            let ptr = NonNull::new_unchecked(strval);
            Ok(Self::from_parts(ptr, alloc))
        }
    }

    pub unsafe fn from_utf8_unchecked(boxed: Box<[u8], A>) -> Self {
        let (ptr, alloc) = Box::into_raw_with_allocator(boxed);
        let strval = str::from_utf8_unchecked_mut(&mut *ptr);
        let ptr = NonNull::new_unchecked(strval);
        Self::from_parts(ptr, alloc)
    }
}

impl<T, A: RawAlloc> Box<MaybeUninit<T>, A> {
    #[inline]
    pub unsafe fn assume_init(self) -> Box<T, A> {
        Box::transmute(self)
    }

    #[inline]
    pub fn write(mut boxed: Self, value: T) -> Box<T, A> {
        (*boxed).write(value);
        unsafe { boxed.assume_init() }
    }
}

impl<T, A: RawAlloc> Box<[MaybeUninit<T>], A> {
    #[inline]
    fn write_slice(&mut self, data: &[T])
    where
        T: Clone,
    {
        let mut insert = Inserter::for_mut_slice(&mut *self);
        insert.extend_from_slice(data);
        insert.complete();
    }

    #[inline]
    pub unsafe fn assume_init(self) -> Box<[T], A> {
        let length = self.len();
        let (data, alloc) = Self::into_parts(self);
        Box::slice_from_parts(data.cast(), alloc, length)
    }
}

impl<'a, A: RawAlloc + 'a> Box<dyn Any + 'a, A> {
    #[inline]
    pub fn downcast<T: Any>(self) -> Result<Box<T, A>, Self> {
        if self.is::<T>() {
            Ok(unsafe { Box::transmute(self) })
        } else {
            Err(self)
        }
    }
}

impl<'a, A: RawAlloc + 'a> Box<dyn Any + Send + 'a, A> {
    #[inline]
    pub fn downcast<T: Any>(self) -> Result<Box<T, A>, Self> {
        if self.is::<T>() {
            unsafe { Ok(Box::transmute(self)) }
        } else {
            Err(self)
        }
    }
}

impl<'a, A: RawAlloc + 'a> Box<dyn Any + Send + Sync + 'a, A> {
    #[inline]
    pub fn downcast<T: Any>(self) -> Result<Box<T, A>, Self> {
        if self.is::<T>() {
            unsafe { Ok(Box::transmute(self)) }
        } else {
            Err(self)
        }
    }
}

impl<T: ?Sized, A: RawAlloc> AsRef<T> for Box<T, A> {
    #[inline]
    fn as_ref(&self) -> &T {
        unsafe { self.data.as_ref() }
    }
}

impl<T: ?Sized, A: RawAlloc> AsMut<T> for Box<T, A> {
    #[inline]
    fn as_mut(&mut self) -> &mut T {
        unsafe { self.data.as_mut() }
    }
}

impl<T: ?Sized, A: RawAlloc> Borrow<T> for Box<T, A> {
    #[inline]
    fn borrow(&self) -> &T {
        unsafe { self.data.as_ref() }
    }
}

impl<T: ?Sized, A: RawAlloc> BorrowMut<T> for Box<T, A> {
    #[inline]
    fn borrow_mut(&mut self) -> &mut T {
        unsafe { self.data.as_mut() }
    }
}

impl<'b, T: Clone, A: RawAlloc + Clone> Clone for Box<T, A> {
    fn clone(&self) -> Self {
        let clone = Self::new_uninit_in(self.alloc.clone());
        Box::write(clone, T::clone(&**self))
    }

    fn clone_from(&mut self, source: &Self) {
        unsafe {
            ptr::replace(self.data.as_ptr(), (&**source).clone());
        }
    }
}

impl<'b, T: Clone, A: RawAlloc + Clone> Clone for Box<[T], A> {
    fn clone(&self) -> Self {
        let data = &**self;
        let mut boxed = Self::new_uninit_slice_in(data.len(), self.alloc.clone());
        boxed.write_slice(data);
        unsafe { boxed.assume_init() }
    }

    fn clone_from(&mut self, source: &Self) {
        if self.len() == source.len() {
            self.clone_from_slice(&source);
        } else {
            *self = source.clone();
        }
    }
}

impl<T: Default, A: RawAllocNew> Default for Box<T, A> {
    fn default() -> Box<T, A> {
        Box::new(T::default())
    }
}

impl<T, A: RawAllocNew> Default for Box<[T], A> {
    #[inline]
    fn default() -> Box<[T], A> {
        unsafe { Box::slice_from_parts(NonNull::dangling(), A::NEW, 0) }
    }
}

impl<A: RawAllocNew> Default for Box<str, A> {
    #[inline]
    fn default() -> Box<str, A> {
        unsafe { Box::from_utf8_unchecked(Box::<[u8], A>::default()) }
    }
}

impl<T: fmt::Debug + ?Sized, A: RawAlloc> fmt::Debug for Box<T, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<T: fmt::Display + ?Sized, A: RawAlloc> fmt::Display for Box<T, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

impl<T: ?Sized, A: RawAlloc> Deref for Box<T, A> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe { self.data.as_ref() }
    }
}

impl<T: ?Sized, A: RawAlloc> DerefMut for Box<T, A> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.data.as_mut() }
    }
}

impl<T: ?Sized, A: RawAlloc> Drop for Box<T, A> {
    fn drop(&mut self) {
        unsafe {
            let layout = Layout::for_value(self.data.as_ref());
            ptr::drop_in_place(self.data.as_mut());
            self.alloc.release(self.data.cast(), layout);
        }
    }
}

impl<T, A: RawAllocNew> From<T> for Box<T, A> {
    #[inline]
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl<T: Clone, A: RawAllocNew> From<&[T]> for Box<[T], A> {
    #[inline]
    fn from(data: &[T]) -> Self {
        let mut boxed = Self::new_uninit_slice(data.len());
        boxed.write_slice(data);
        unsafe { boxed.assume_init() }
    }
}

impl<A: RawAllocNew> From<&str> for Box<str, A> {
    #[inline]
    fn from(data: &str) -> Self {
        let boxed = Box::copy_slice(data.as_bytes());
        unsafe { Box::from_utf8_unchecked(boxed) }
    }
}

impl<T, A: RawAlloc> From<Vec<T, A>> for Box<[T], A> {
    #[inline]
    fn from(vec: Vec<T, A>) -> Self {
        vec.into_boxed_slice()
    }
}

impl<T: ?Sized, A: RawAlloc> From<Box<T, A>> for Pin<Box<T, A>> {
    #[inline]
    fn from(boxed: Box<T, A>) -> Self {
        unsafe { Pin::new_unchecked(boxed) }
    }
}

#[cfg(feature = "alloc")]
impl<T: ?Sized> From<alloc::boxed::Box<T>> for Box<T, Global> {
    #[inline]
    fn from(boxed: alloc::boxed::Box<T>) -> Self {
        let ptr = alloc::boxed::Box::into_raw(boxed);
        unsafe { Box::from_parts(NonNull::new_unchecked(ptr), Global) }
    }
}

// not allowed to be implemented for Box<T> due to orphan rules
// #[cfg(feature = "alloc")]
// impl<T: ?Sized> From<Box<T, Global>> for alloc::boxed::Box<T> {
//     #[inline]
//     fn from(boxed: Box<T, Global>) -> Self {
//         let (ptr, alloc) = boxed.into_parts();
//         unsafe { alloc::boxed::Box::from_raw(ptr) }
//     }
// }

#[cfg(feature = "allocator-api2")]
impl<T: ?Sized, A> From<allocator_api2::boxed::Box<T, A>> for Box<T, A>
where
    A: allocator_api2::alloc::Allocator,
{
    #[inline]
    fn from(boxed: allocator_api2::boxed::Box<T, A>) -> Self {
        let (ptr, alloc) = allocator_api2::boxed::Box::into_raw_with_allocator(boxed);
        unsafe { Box::from_parts(NonNull::new_unchecked(ptr), alloc) }
    }
}

#[cfg(feature = "allocator-api2")]
impl<T: ?Sized, A> From<Box<T, A>> for allocator_api2::boxed::Box<T, A>
where
    A: allocator_api2::alloc::Allocator,
{
    #[inline]
    fn from(boxed: Box<T, A>) -> Self {
        let (ptr, alloc) = boxed.into_parts();
        unsafe { allocator_api2::boxed::Box::from_raw_in(ptr.as_ptr(), alloc) }
    }
}

// #[cfg(feature = "alloc")]
// impl<T, S> From<alloc::vec::Vec<T>> for Box<[T], S>
// where
//     S: StorageNew + StorageWithAlloc,
// {
//     fn from(vec: alloc::vec::Vec<T>) -> Self {
//         Vec::from(vec).into_boxed_slice()
//     }
// }

impl<T, A: RawAlloc + Default> FromIterator<T> for Box<[T], A> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut vec = Vec::new_in(A::default());
        vec.extend(iter);
        vec.into_boxed_slice()
    }
}

impl<T: ?Sized + PartialEq, A: RawAlloc> PartialEq for Box<T, A> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        PartialEq::eq(&**self, &**other)
    }
}

impl<T: ?Sized + Eq, A: RawAlloc> Eq for Box<T, A> {}

impl<T: ?Sized + PartialOrd, A: RawAlloc> PartialOrd for Box<T, A> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        PartialOrd::partial_cmp(&**self, &**other)
    }
}

impl<T: ?Sized + Ord, A: RawAlloc> Ord for Box<T, A> {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        Ord::cmp(&**self, &**other)
    }
}

impl<T: ?Sized, A: RawAlloc> Unpin for Box<T, A> {}

#[cfg(feature = "zeroize")]
impl<T: zeroize::Zeroize, C: RawAlloc> zeroize::Zeroize for Box<T, C> {
    #[inline]
    fn zeroize(&mut self) {
        self.deref_mut().zeroize()
    }
}

#[cfg(feature = "zeroize")]
impl<T, C: RawAlloc> zeroize::ZeroizeOnDrop for Box<T, crate::storage::ZeroizingAlloc<C>> {}

#[cfg(feature = "zeroize")]
pub type ZeroizingBox<T> = Box<T, crate::storage::ZeroizingAlloc<Global>>;

// new_zeroed
// new_zeroed_in
// new_zeroed_slice
// new_zeroed_slice_in
// try_new_zeroed
// try_new_zeroed_in
// try_new_zeroed_slice
// misc traits
