use core::alloc::{Layout, LayoutError};
use core::fmt;
use core::marker::PhantomData;
use core::mem::{align_of, ManuallyDrop};
use core::ptr;
use core::ptr::NonNull;

use const_default::ConstDefault;

use super::RawBuffer;
use crate::alloc::{AllocateIn, Allocator, AllocatorDefault};
use crate::error::StorageError;

#[cfg(feature = "zeroize")]
use crate::alloc::AllocatorZeroizes;

/// A header type used by a buffer to determine its size.
pub trait BufferHeader<T: ?Sized>: Copy + fmt::Debug + Sized {
    /// The header value for a zero-sized buffer.
    const EMPTY: Self;

    /// Determine if this header represents a zero-sized buffer.
    fn is_empty(&self) -> bool;

    /// Calculate the layout for the associated value.
    fn layout(&self) -> Result<Layout, LayoutError>;

    /// Update the header from the result of a new allocation.
    fn update_for_alloc(&mut self, ptr: NonNull<[u8]>, exact: bool) -> NonNull<T>;
}

/// A slice allocation handle which stores the header metadata in the handle.
#[derive(Debug)]
pub struct FatBuffer<T: ?Sized, H: BufferHeader<T>, A: Allocator> {
    pub(crate) header: H,
    pub(crate) data: NonNull<T>,
    pub(crate) alloc: A,
}

impl<T, H: BufferHeader<T>, A: Allocator> FatBuffer<T, H, A> {
    #[inline]
    pub(crate) fn allocate_in<I>(header: H, alloc_in: I, exact: bool) -> Result<Self, StorageError>
    where
        I: AllocateIn<Alloc = A>,
    {
        let layout = header.layout()?;
        let (ptr, alloc) = alloc_in.allocate_in(layout)?;
        let mut header = H::EMPTY;
        let data = header.update_for_alloc(ptr, exact);
        Ok(Self {
            header,
            data,
            alloc,
        })
    }

    #[inline]
    pub(crate) fn grow(&mut self, mut new_header: H, exact: bool) -> Result<(), StorageError> {
        let new_layout = new_header.layout()?;
        let ptr = if self.is_dangling() {
            self.alloc.allocate(new_layout)?
        } else {
            let old_layout: Layout = self.header.layout()?;
            unsafe { self.alloc.grow(self.data.cast(), old_layout, new_layout) }?
        };
        self.data = new_header.update_for_alloc(ptr, exact);
        self.header = new_header;
        Ok(())
    }

    #[inline]
    pub(crate) fn shrink(&mut self, mut new_header: H) -> Result<(), StorageError> {
        if new_header.is_empty() {
            if !self.is_dangling() {
                let layout = self.header.layout()?;
                unsafe { self.alloc.deallocate(self.data.cast(), layout) };
                self.data = NonNull::dangling();
            }
        } else {
            let new_layout = new_header.layout()?;
            let ptr = if self.is_dangling() {
                self.alloc.allocate(new_layout)?
            } else {
                let old_layout: Layout = self.header.layout()?;
                unsafe { self.alloc.shrink(self.data.cast(), old_layout, new_layout) }?
            };
            self.data = new_header.update_for_alloc(ptr, true);
        }
        self.header = new_header;
        Ok(())
    }
}

impl<T, H: BufferHeader<T>, A: Allocator> FatBuffer<T, H, A> {
    #[inline]
    pub(crate) const fn dangling(alloc: A) -> Self {
        Self {
            header: H::EMPTY,
            data: NonNull::dangling(),
            alloc,
        }
    }

    #[inline]
    pub(crate) fn is_dangling(&self) -> bool {
        self.data == NonNull::dangling()
    }
}

impl<T: ?Sized, H: BufferHeader<T>, A: Allocator> FatBuffer<T, H, A> {
    #[inline]
    pub(crate) fn from_parts(header: H, data: NonNull<T>, alloc: A) -> Self {
        Self {
            header,
            data,
            alloc,
        }
    }

    #[inline]
    pub(crate) fn into_parts(self) -> (H, NonNull<T>, A) {
        let slf = ManuallyDrop::new(self);
        (slf.header, slf.data, unsafe { ptr::read(&slf.alloc) })
    }
}

impl<T, H, A> ConstDefault for FatBuffer<T, H, A>
where
    H: BufferHeader<T>,
    A: AllocatorDefault,
{
    const DEFAULT: Self = Self::dangling(A::DEFAULT);
}

impl<T: ?Sized, H: BufferHeader<T>, A: Allocator> RawBuffer for FatBuffer<T, H, A> {
    type RawData = T;

    #[inline]
    fn data_ptr(&self) -> *const T {
        self.data.as_ptr()
    }

    #[inline]
    fn data_ptr_mut(&mut self) -> *mut T {
        self.data.as_ptr()
    }
}

impl<T: ?Sized, H: BufferHeader<T>, A: Allocator> Drop for FatBuffer<T, H, A> {
    fn drop(&mut self) {
        let layout = self.header.layout().expect("Layout error");
        if layout.size() > 0 {
            unsafe {
                self.alloc.deallocate(self.data.cast(), layout);
            }
        }
    }
}

#[cfg(feature = "zeroize")]
impl<T: ?Sized, H: BufferHeader<T>, A: AllocatorZeroizes> zeroize::ZeroizeOnDrop
    for FatBuffer<T, H, A>
{
}

pub(crate) struct ThinPtr<T, H: BufferHeader<T>>(NonNull<T>, PhantomData<H>);

impl<T, H: BufferHeader<T>> ThinPtr<T, H> {
    const DATA_OFFSET: usize = data_offset::<H, T>();

    #[inline]
    pub const fn dangling() -> Self {
        Self(NonNull::dangling(), PhantomData)
    }

    #[inline]
    pub fn is_dangling(&self) -> bool {
        ptr::eq(self.0.as_ptr(), NonNull::dangling().as_ptr())
    }

    #[inline]
    pub fn from_alloc(mut header: H, ptr: NonNull<[u8]>, exact: bool) -> Self {
        #[allow(clippy::len_zero)]
        if ptr.len() == 0 {
            Self::dangling()
        } else {
            let data_alloc = unsafe {
                NonNull::new_unchecked(ptr::slice_from_raw_parts_mut(
                    (ptr.as_ptr() as *mut u8).add(Self::DATA_OFFSET),
                    ptr.len() - Self::DATA_OFFSET,
                ))
            };
            let data = header.update_for_alloc(data_alloc, exact);
            unsafe { ptr.cast::<H>().as_ptr().write(header) };
            Self(data, PhantomData)
        }
    }

    #[inline]
    fn layout(header: &H) -> Result<Layout, LayoutError> {
        if header.is_empty() {
            Ok(unsafe { Layout::from_size_align_unchecked(0, align_of::<H>()) })
        } else {
            let data_layout = header.layout()?;
            match Layout::new::<H>().extend(data_layout) {
                Ok((layout, _)) => Ok(layout),
                Err(err) => Err(err),
            }
        }
    }

    #[inline]
    pub const fn to_alloc(&self) -> NonNull<u8> {
        unsafe { NonNull::new_unchecked(self.header_ptr()) }.cast()
    }

    #[inline]
    pub const fn as_ptr(&self) -> *mut T {
        self.0.as_ptr()
    }

    #[inline]
    pub const fn header_ptr(&self) -> *mut H {
        unsafe { (self.0.as_ptr() as *mut u8).sub(Self::DATA_OFFSET) as *mut _ }
    }
}

impl<T, H: BufferHeader<T>> fmt::Debug for ThinPtr<T, H> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", &self.0)
    }
}

/// A slice allocation handle which stores the header metadata in the handle.
#[derive(Debug)]
pub struct ThinBuffer<T, H: BufferHeader<T>, A: Allocator> {
    pub(crate) data: ThinPtr<T, H>,
    pub(crate) alloc: A,
}

impl<T, H: BufferHeader<T>, A: Allocator> ThinBuffer<T, H, A> {
    #[inline]
    pub(crate) fn allocate_in<I>(header: H, alloc_in: I, exact: bool) -> Result<Self, StorageError>
    where
        I: AllocateIn<Alloc = A>,
    {
        let layout = ThinPtr::layout(&header)?;
        let (ptr, alloc) = alloc_in.allocate_in(layout)?;
        let data = ThinPtr::from_alloc(header, ptr, exact);
        Ok(Self { data, alloc })
    }

    #[inline]
    pub(crate) fn grow(&mut self, new_header: H, exact: bool) -> Result<(), StorageError> {
        let old_header = self.header();
        let new_layout = ThinPtr::layout(&new_header)?;
        assert!(new_layout.size() != 0, "Cannot grow to empty buffer");
        let ptr = if old_header.is_empty() {
            self.alloc.allocate(new_layout)?
        } else {
            let old_layout: Layout = ThinPtr::<T, H>::layout(&old_header)?;
            unsafe {
                self.alloc
                    .grow(self.data.to_alloc(), old_layout, new_layout)
            }?
        };
        self.data = ThinPtr::from_alloc(new_header, ptr, exact);
        Ok(())
    }

    #[inline]
    pub(crate) fn shrink(&mut self, new_header: H) -> Result<(), StorageError> {
        let old_header = self.header();
        let old_layout = if old_header.is_empty() {
            None
        } else {
            Some(ThinPtr::<T, H>::layout(&old_header)?)
        };
        if new_header.is_empty() {
            if let Some(old_layout) = old_layout {
                unsafe { self.alloc.deallocate(self.data.to_alloc(), old_layout) };
                self.data = ThinPtr::dangling();
            }
        } else {
            let new_layout = ThinPtr::<T, H>::layout(&new_header)?;
            let ptr = if let Some(old_layout) = old_layout {
                unsafe {
                    self.alloc
                        .shrink(self.data.to_alloc(), old_layout, new_layout)
                }?
            } else {
                self.alloc.allocate(new_layout)?
            };
            self.data = ThinPtr::from_alloc(new_header, ptr, true);
        }
        Ok(())
    }
}

impl<T, H: BufferHeader<T>, A: Allocator> ThinBuffer<T, H, A> {
    #[inline]
    pub(crate) const fn dangling(alloc: A) -> Self {
        Self {
            data: ThinPtr::dangling(),
            alloc,
        }
    }

    #[inline]
    pub(crate) fn is_dangling(&self) -> bool {
        self.data.is_dangling()
    }

    #[inline]
    pub(crate) fn header(&self) -> H {
        if self.is_dangling() {
            H::EMPTY
        } else {
            unsafe { ptr::read(self.data.header_ptr()) }
        }
    }

    #[inline]
    pub(crate) unsafe fn set_header(&mut self, header: H) {
        self.data.header_ptr().write(header)
    }
}

impl<T, H: BufferHeader<T>, A: Allocator> ThinBuffer<T, H, A> {
    #[inline]
    pub(crate) fn from_parts(_header: H, data: NonNull<T>, alloc: A) -> Self {
        // FIXME assert headers match?
        Self {
            data: ThinPtr(data, PhantomData),
            alloc,
        }
    }

    #[inline]
    pub(crate) fn into_parts(self) -> (H, NonNull<T>, A) {
        let slf = ManuallyDrop::new(self);
        let header = if slf.is_dangling() {
            H::EMPTY
        } else {
            unsafe { ptr::read(slf.data.header_ptr()) }
        };
        (header, slf.data.0, unsafe { ptr::read(&slf.alloc) })
    }
}

impl<T, H, A> ConstDefault for ThinBuffer<T, H, A>
where
    H: BufferHeader<T>,
    A: AllocatorDefault,
{
    const DEFAULT: Self = Self::dangling(A::DEFAULT);
}

impl<T, H: BufferHeader<T>, A: Allocator> RawBuffer for ThinBuffer<T, H, A> {
    type RawData = T;

    #[inline]
    fn data_ptr(&self) -> *const T {
        self.data.as_ptr()
    }

    #[inline]
    fn data_ptr_mut(&mut self) -> *mut T {
        self.data.as_ptr()
    }
}

#[cfg(feature = "zeroize")]
impl<T, H: BufferHeader<T>, A: AllocatorZeroizes> zeroize::ZeroizeOnDrop for ThinBuffer<T, H, A> {}

impl<T, H: BufferHeader<T>, A: Allocator> Drop for ThinBuffer<T, H, A> {
    fn drop(&mut self) {
        let header = self.header();
        if !header.is_empty() {
            let layout = ThinPtr::<T, H>::layout(&header).expect("Layout error");
            unsafe {
                self.alloc.deallocate(self.data.to_alloc(), layout);
            }
        }
    }
}

/// Calculate the byte offset of Data when following Header. This should
/// be equivalent to offset_of!((Meta::Header, Meta::Data), 1)
/// although repr(C) would need to be used to guarantee consistency.
/// See `Layout::padding_needed_for` (currently unstable) for reference.
const fn data_offset<Header, Data>() -> usize {
    let header = Layout::new::<Header>();
    let data_align = align_of::<Data>();
    header.size().wrapping_add(data_align).wrapping_sub(1) & !data_align.wrapping_sub(1)
}
