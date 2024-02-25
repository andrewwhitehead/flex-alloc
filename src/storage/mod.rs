use core::fmt;
use core::marker::PhantomData;
use core::mem::{ManuallyDrop, MaybeUninit};
use core::ptr::NonNull;

pub(crate) mod alloc;

pub(crate) mod utils;

pub use self::alloc::{
    AllocHandleParts, AllocHeader, AllocLayout, Global, RawAlloc, RawAllocIn, RawAllocNew, Thin,
};

pub trait RawBuffer: Sized {
    type RawData: ?Sized;

    fn data_ptr(&self) -> *const Self::RawData;

    fn data_ptr_mut(&mut self) -> *mut Self::RawData;
}

#[repr(C)]
pub union ByteStorage<T, const N: usize> {
    _align: [ManuallyDrop<T>; 0],
    data: [MaybeUninit<u8>; N],
}

impl<T, const N: usize> ByteStorage<T, N> {
    pub const fn new() -> Self {
        Self {
            data: unsafe { MaybeUninit::uninit().assume_init() },
        }
    }

    pub(crate) fn as_uninit_slice(&mut self) -> &mut [MaybeUninit<u8>] {
        unsafe { &mut self.data }
    }
}

impl<T, const N: usize> fmt::Debug for ByteStorage<T, N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ByteBuffer").finish_non_exhaustive()
    }
}

pub const fn array_storage<T, const N: usize>() -> [MaybeUninit<T>; N] {
    unsafe { MaybeUninit::uninit().assume_init() }
}

pub const fn byte_storage<const N: usize>() -> ByteStorage<u8, N> {
    ByteStorage::<u8, N>::new()
}

pub const fn aligned_byte_storage<T, const N: usize>() -> ByteStorage<T, N> {
    ByteStorage::<T, N>::new()
}

#[derive(Debug)]
pub struct Fixed<'a>(PhantomData<&'a mut ()>);

#[derive(Debug)]
pub struct FixedBuffer<Header, Data> {
    pub data: NonNull<Data>,
    pub header: Header,
}

impl<Header, Data> FixedBuffer<Header, Data> {
    pub const fn new(header: Header, data: NonNull<Data>) -> Self {
        Self { header, data }
    }
}

impl<Header, Data> RawBuffer for FixedBuffer<Header, Data> {
    type RawData = Data;

    #[inline]
    fn data_ptr(&self) -> *const Data {
        self.data.as_ptr()
    }

    #[inline]
    fn data_ptr_mut(&mut self) -> *mut Data {
        self.data.as_ptr()
    }
}

#[derive(Debug)]
pub struct Inline<const N: usize>;

#[derive(Debug)]
pub struct InlineBuffer<T, const N: usize> {
    pub data: [MaybeUninit<T>; N],
    pub length: usize,
}

impl<T, const N: usize> InlineBuffer<T, N> {
    pub const fn new() -> Self {
        Self {
            data: unsafe { MaybeUninit::uninit().assume_init() },
            length: 0,
        }
    }
}

impl<T, const N: usize> Default for InlineBuffer<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, T: 'a, const N: usize> RawBuffer for InlineBuffer<T, N> {
    type RawData = T;

    #[inline]
    fn data_ptr(&self) -> *const T {
        self.data.as_ptr().cast()
    }

    #[inline]
    fn data_ptr_mut(&mut self) -> *mut T {
        self.data.as_mut_ptr().cast()
    }
}
