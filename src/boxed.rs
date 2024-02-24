use core::alloc::Layout;
use core::any::Any;
use core::borrow::{Borrow, BorrowMut};
use core::cmp::Ordering;
use core::fmt;
use core::mem::{ManuallyDrop, MaybeUninit};
use core::ops::{Deref, DerefMut};
use core::pin::Pin;
use core::ptr::{self, NonNull};
use core::slice;
use core::str;

use crate::error::StorageError;
use crate::storage::{Global, RawAlloc, RawAllocIn, RawAllocNew};

// #[cfg(feature = "zeroize")]
// use zeroize::ZeroizeOnDrop;

// #[cfg(feature = "alloc")]
// pub type AllocBox<T> = Box<T, AllocGlobal>;

// pub type FlexBox<'b, T> = Box<T, Flex<'b>>;

// pub type RefBox<'b, T> = Box<T, Fixed<'b>>;

pub struct Box<T: ?Sized, A: RawAlloc = Global> {
    data: NonNull<T>,
    alloc: A,
}

impl<T: ?Sized, A: RawAlloc> Box<T, A> {
    #[inline]
    pub unsafe fn from_parts(data: NonNull<T>, alloc: A) -> Self {
        Self { data, alloc }
    }

    #[inline]
    pub fn into_parts(boxed: Self) -> (NonNull<T>, A) {
        let boxed = ManuallyDrop::new(boxed);
        (boxed.data, unsafe { ptr::read(&boxed.alloc) })
    }

    #[inline]
    pub(crate) unsafe fn transmute<R>(boxed: Self) -> Box<R, A> {
        let (data, alloc) = Self::into_parts(boxed);
        Box::from_parts(data.cast(), alloc)
    }
}

impl<T, A: RawAlloc> Box<T, A> {
    pub fn new_uninit_in<I>(alloc_in: I) -> Box<MaybeUninit<T>, A>
    where
        I: RawAllocIn<RawAlloc = A>,
    {
        match alloc_in.try_alloc_in(Layout::new::<MaybeUninit<T>>()) {
            Ok((data, alloc)) => unsafe { Box::from_parts(data.cast(), alloc) },
            Err(err) => err.panic(),
        }
    }

    pub fn new_in<I>(value: T, alloc_in: I) -> Self
    where
        I: RawAllocIn<RawAlloc = A>,
    {
        let boxed = Self::new_uninit_in(alloc_in);
        Box::write(boxed, value)
    }

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

    pub fn into_inner(boxed: Self) -> T {
        unsafe { Box::transmute::<MaybeUninit<T>>(boxed).as_ptr().read() }
    }

    pub fn into_boxed_slice(boxed: Self) -> Box<[T], A> {
        let (data, alloc) = Self::into_parts(boxed);
        unsafe { Box::slice_from_parts(data, alloc, 1) }
    }

    pub fn leak<'a>(boxed: Self) -> &'a mut T
    where
        A: 'a,
    {
        let (mut data, _alloc) = Self::into_parts(boxed);
        unsafe { data.as_mut() }
    }

    pub fn into_pin(boxed: Self) -> Pin<Self> {
        unsafe { Pin::new_unchecked(boxed) }
    }

    pub fn pin_in<I>(value: T, alloc_in: I) -> Pin<Self>
    where
        I: RawAllocIn<RawAlloc = A>,
    {
        Pin::from(Self::new_in(value, alloc_in))
    }
}

impl<T, A: RawAllocNew> Box<T, A> {
    pub fn new(value: T) -> Self {
        Self::new_in(value, A::NEW)
    }

    pub fn new_uninit() -> Box<MaybeUninit<T>, A> {
        Self::new_uninit_in(A::NEW)
    }

    pub fn try_new(value: T) -> Result<Self, StorageError> {
        Self::try_new_in(value, A::NEW)
    }

    pub fn try_new_uninit() -> Result<Box<MaybeUninit<T>, A>, StorageError> {
        Self::try_new_uninit_in(A::NEW)
    }

    pub fn pin(value: T) -> Pin<Self> {
        Pin::from(Self::new(value))
    }
}

impl<T, A: RawAlloc> Box<[T], A> {
    #[inline]
    pub unsafe fn slice_from_parts(data: NonNull<T>, alloc: A, length: usize) -> Self {
        let data = ptr::slice_from_raw_parts_mut(data.as_ptr().cast(), length);
        Self::from_parts(NonNull::new_unchecked(data), alloc)
    }

    #[inline]
    pub fn slice_into_parts(self) -> (NonNull<T>, A, usize) {
        let length = self.len();
        let (data, alloc) = Self::into_parts(self);
        (data.cast(), alloc, length)
    }
}

impl<T, A: RawAlloc> Box<[T], A> {
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
}

impl<T, A: RawAllocNew> Box<[T], A> {
    pub fn new_uninit_slice(length: usize) -> Box<[MaybeUninit<T>], A> {
        Self::new_uninit_slice_in(length, A::NEW)
    }

    pub fn try_new_uninit_slice(length: usize) -> Result<Box<[MaybeUninit<T>], A>, StorageError> {
        Self::try_new_uninit_slice_in(length, A::NEW)
    }

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

impl<A: RawAlloc> Box<str, A> {
    pub fn from_utf8(boxed: Box<[u8], A>) -> Result<Self, str::Utf8Error> {
        let (ptr, storage, length) = boxed.slice_into_parts();
        unsafe {
            let data = slice::from_raw_parts_mut(ptr.as_ptr(), length);
            let strval = str::from_utf8_mut(data)?;
            let ptr = NonNull::new_unchecked(strval);
            Ok(Self::from_parts(ptr, storage))
        }
    }

    pub unsafe fn from_utf8_unchecked(boxed: Box<[u8], A>) -> Self {
        let (ptr, storage, length) = boxed.slice_into_parts();
        let data = slice::from_raw_parts_mut(ptr.as_ptr(), length);
        let strval = str::from_utf8_unchecked_mut(data);
        let ptr = NonNull::new_unchecked(strval);
        Self::from_parts(ptr, storage)
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
    pub unsafe fn assume_init(self) -> Box<[T], A> {
        let (ptr, storage, length) = self.slice_into_parts();
        Box::slice_from_parts(ptr.cast(), storage, length)
    }
}

impl<'a, A: RawAlloc + 'a> Box<dyn Any + 'a, A> {
    pub fn downcast<T: Any>(self) -> Result<Box<T, A>, Self> {
        if self.is::<T>() {
            Ok(unsafe { Box::transmute(self) })
        } else {
            Err(self)
        }
    }
}

impl<'a, A: RawAlloc + 'a> Box<dyn Any + Send + 'a, A> {
    pub fn downcast<T: Any>(self) -> Result<Box<T, A>, Self> {
        if self.is::<T>() {
            unsafe { Ok(Box::transmute(self)) }
        } else {
            Err(self)
        }
    }
}

impl<'a, A: RawAlloc + 'a> Box<dyn Any + Send + Sync + 'a, A> {
    pub fn downcast<T: Any>(self) -> Result<Box<T, A>, Self> {
        if self.is::<T>() {
            unsafe { Ok(Box::transmute(self)) }
        } else {
            Err(self)
        }
    }
}

impl<T: ?Sized, A: RawAlloc> AsRef<T> for Box<T, A> {
    fn as_ref(&self) -> &T {
        unsafe { self.data.as_ref() }
    }
}

impl<T: ?Sized, A: RawAlloc> AsMut<T> for Box<T, A> {
    fn as_mut(&mut self) -> &mut T {
        unsafe { self.data.as_mut() }
    }
}

impl<T: ?Sized, A: RawAlloc> Borrow<T> for Box<T, A> {
    fn borrow(&self) -> &T {
        unsafe { self.data.as_ref() }
    }
}

impl<T: ?Sized, A: RawAlloc> BorrowMut<T> for Box<T, A> {
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

// impl<'b, T: Clone, S: StorageSpawn> Clone for Box<[T], S> {
//     fn clone(&self) -> Self {
//         let mut vec = Vec::with_capacity_in(self.len(), Self::storage(self));
//         vec.extend_from_slice(&*self);
//         vec.into_boxed_slice()
//     }

//     fn clone_from(&mut self, source: &Self) {
//         if self.len() == source.len() {
//             self.clone_from_slice(&source);
//         } else {
//             *self = source.clone();
//         }
//     }
// }

impl<T: Default, A: RawAllocNew> Default for Box<T, A> {
    fn default() -> Box<T, A> {
        Box::new(T::default())
    }
}

impl<T, A: RawAllocNew> Default for Box<[T], A> {
    fn default() -> Box<[T], A> {
        unsafe { Box::slice_from_parts(NonNull::dangling(), A::NEW, 0) }
    }
}

impl<A: RawAllocNew> Default for Box<str, A> {
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

    fn deref(&self) -> &Self::Target {
        unsafe { self.data.as_ref() }
    }
}

impl<T: ?Sized, A: RawAlloc> DerefMut for Box<T, A> {
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
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

// impl<T: Clone, S: StorageNew> From<&[T]> for Box<[T], S> {
//     fn from(data: &[T]) -> Self {
//         Vec::<T, S>::from_slice(data).into_boxed_slice()
//     }
// }

impl<A: RawAllocNew> From<&str> for Box<str, A> {
    fn from(data: &str) -> Self {
        let boxed = Box::copy_slice(data.as_bytes());
        unsafe { Box::from_utf8_unchecked(boxed) }
    }
}

// impl<T, S: StorageSpawn> From<Vec<T, S>> for Box<[T], S> {
//     fn from(vec: Vec<T, S>) -> Self {
//         vec.into_boxed_slice()
//     }
// }

impl<T: ?Sized, A: RawAlloc> From<Box<T, A>> for Pin<Box<T, A>> {
    fn from(boxed: Box<T, A>) -> Self {
        unsafe { Pin::new_unchecked(boxed) }
    }
}

#[cfg(feature = "alloc")]
impl<T: ?Sized> From<alloc::boxed::Box<T>> for Box<T, Global> {
    fn from(boxed: alloc::boxed::Box<T>) -> Self {
        let ptr = alloc::boxed::Box::into_raw(boxed);
        unsafe { Box::from_parts(NonNull::new_unchecked(ptr), Global) }
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

// impl<T, S: StorageNew> FromIterator<T> for Box<[T], S> {
//     fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
//         Vec::<T, S>::from_iter(iter).into_boxed_slice()
//     }
// }

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

// #[cfg(feature = "zeroize")]
// impl<T: ?Sized, S: Storage + ZeroizeOnDrop> ZeroizeOnDrop for Box<T, ZeroizingStorage<S>> {}

// from_raw?
// from_raw_in?
// into_raw?
// into_raw_with_storage?
// new_zeroed
// new_zeroed_in
// new_zeroed_slice
// new_zeroed_slice_in
// try_new_zeroed
// try_new_zeroed_in
// try_new_zeroed_slice
// misc traits
