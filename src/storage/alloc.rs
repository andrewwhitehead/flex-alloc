use core::alloc::Layout;
use core::fmt;
use core::marker::PhantomData;
use core::mem::{align_of, offset_of, ManuallyDrop};
use core::ptr::{self, NonNull};

#[cfg(feature = "alloc")]
use alloc::alloc::{alloc as raw_alloc, dealloc as raw_dealloc};
#[cfg(feature = "alloc")]
use core::mem::transmute;

use crate::error::StorageError;

use super::utils::layout_aligned_bytes;
use super::{ByteStorage, RawBuffer};

pub trait RawAlloc: fmt::Debug {
    fn try_alloc(&self, layout: Layout) -> Result<NonNull<[u8]>, StorageError>;

    unsafe fn try_resize(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, StorageError> {
        // Default implementation simply allocates and copies over the contents.
        // NB: not copying the entire previous buffer seems to defeat some automatic
        // optimization and results in much worse performance (on MacOS 14 at least).
        let new_ptr = self.try_alloc(new_layout)?;
        let cp_len = old_layout.size().min(new_ptr.len());
        if cp_len > 0 {
            ptr::copy_nonoverlapping(ptr.as_ptr(), new_ptr.as_ptr().cast(), cp_len);
        }
        self.release(ptr, old_layout);
        Ok(new_ptr)
    }

    unsafe fn release(&self, ptr: NonNull<u8>, layout: Layout);
}

pub trait RawAllocIn: Sized {
    type RawAlloc: RawAlloc;

    fn try_alloc_in(self, layout: Layout) -> Result<(NonNull<[u8]>, Self::RawAlloc), StorageError>;
}

impl<A: RawAlloc> RawAllocIn for A {
    type RawAlloc = A;

    #[inline]
    fn try_alloc_in(self, layout: Layout) -> Result<(NonNull<[u8]>, Self::RawAlloc), StorageError> {
        let data = self.try_alloc(layout)?;
        Ok((data, self))
    }
}

pub trait RawAllocNew: RawAlloc + Clone + Copy {
    const NEW: Self;
}

// FIXME use alloc::alloc::Global when allocator_api enabled
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "alloc", derive(Default, Copy))]
pub struct Global;

#[cfg(feature = "alloc")]
impl RawAlloc for Global {
    #[inline]
    fn try_alloc(&self, layout: Layout) -> Result<NonNull<[u8]>, StorageError> {
        let ptr = if layout.size() == 0 {
            // FIXME: use Layout::dangling when stabilized
            unsafe { NonNull::new_unchecked(transmute(layout.align())) }
        } else {
            let Some(ptr) = NonNull::new(unsafe { raw_alloc(layout) }) else {
                return Err(StorageError::AllocError);
            };
            ptr
        };
        Ok(NonNull::slice_from_raw_parts(ptr, layout.size()))
    }

    #[inline]
    unsafe fn release(&self, ptr: NonNull<u8>, layout: Layout) {
        if layout.size() > 0 {
            raw_dealloc(ptr.as_ptr(), layout);
        }
    }
}

#[cfg(not(feature = "alloc"))]
// Stub implementation to allow Global as the default allocator type.
// Because the type can't be created, errors will still be detected at compile time if used.
impl RawAlloc for Global {
    fn try_alloc(&self, _layout: Layout) -> Result<NonNull<[u8]>, StorageError> {
        unimplemented!();
    }

    unsafe fn release(&self, _ptr: NonNull<u8>, _layout: Layout) {
        unimplemented!();
    }
}

#[cfg(feature = "alloc")]
impl RawAllocNew for Global {
    const NEW: Self = Global;
}

pub trait AllocHeader: Copy + Clone + Sized {
    const EMPTY: Self;

    fn is_empty(&self) -> bool;
}

pub trait AllocLayout {
    type Header: AllocHeader;
    type Data;

    fn layout(header: &Self::Header) -> Result<Layout, StorageError>;

    fn update_header(header: &mut Self::Header, layout: Layout);
}

pub trait AllocHandle: RawBuffer<RawData = <Self::Meta as AllocLayout>::Data> {
    type Alloc: RawAlloc;
    type Meta: AllocLayout;

    fn allocator(&self) -> &Self::Alloc;

    fn is_empty_handle(&self) -> bool;

    /// SAFETY: is_empty_buffer must return false
    unsafe fn header(&self) -> &<Self::Meta as AllocLayout>::Header;

    /// SAFETY: is_empty_buffer must return false
    unsafe fn header_mut(&mut self) -> &mut <Self::Meta as AllocLayout>::Header;

    fn alloc_handle_in<A>(
        alloc_in: A,
        header: <Self::Meta as AllocLayout>::Header,
        exact: bool,
    ) -> Result<Self, StorageError>
    where
        A: RawAllocIn<RawAlloc = Self::Alloc>;

    fn resize_handle(
        &mut self,
        new_header: <Self::Meta as AllocLayout>::Header,
        exact: bool,
    ) -> Result<(), StorageError>;

    #[inline]
    fn spawn_handle(
        &self,
        header: <Self::Meta as AllocLayout>::Header,
        exact: bool,
    ) -> Result<Self, StorageError>
    where
        Self::Alloc: Clone,
    {
        Self::alloc_handle_in(self.allocator().clone(), header, exact)
    }
}

pub trait AllocHandleNew: AllocHandle {
    const NEW: Self;
    const NEW_ALLOC: Self::Alloc;
}

pub trait AllocHandleParts: AllocHandle {
    fn handle_from_parts(
        header: <Self::Meta as AllocLayout>::Header,
        data: NonNull<<Self::Meta as AllocLayout>::Data>,
        alloc: Self::Alloc,
    ) -> Self;

    fn handle_into_parts(
        self,
    ) -> (
        <Self::Meta as AllocLayout>::Header,
        NonNull<<Self::Meta as AllocLayout>::Data>,
        Self::Alloc,
    );
}

#[derive(Debug)]
pub struct FatAllocHandle<Meta: AllocLayout, Alloc: RawAlloc> {
    header: Meta::Header,
    data: NonNull<Meta::Data>,
    alloc: Alloc,
}

impl<Meta: AllocLayout, Alloc: RawAlloc> FatAllocHandle<Meta, Alloc> {
    #[inline]
    const fn new(header: Meta::Header, data: NonNull<u8>, alloc: Alloc) -> Self {
        Self {
            header,
            data: data.cast(),
            alloc,
        }
    }

    #[inline]
    pub const fn dangling(header: Meta::Header, alloc: Alloc) -> Self {
        Self::new(header, NonNull::<Meta::Data>::dangling().cast(), alloc)
    }

    #[inline]
    fn is_dangling(&self) -> bool {
        ptr::eq(self.data.as_ptr(), NonNull::dangling().as_ptr())
    }

    #[inline]
    pub fn into_raw_parts(self) -> (Meta::Header, NonNull<u8>, Alloc) {
        let parts = ManuallyDrop::new(self);
        let header = unsafe { ptr::read(&parts.header) };
        let alloc = unsafe { ptr::read(&parts.alloc) };
        let data = parts.data.cast();
        (header, data, alloc)
    }
}

impl<Meta: AllocLayout, Alloc: RawAlloc> RawBuffer for FatAllocHandle<Meta, Alloc> {
    type RawData = Meta::Data;

    #[inline]
    fn data_ptr(&self) -> *const Self::RawData {
        self.data.as_ptr()
    }

    #[inline]
    fn data_ptr_mut(&mut self) -> *mut Self::RawData {
        self.data.as_ptr()
    }
}

impl<Meta: AllocLayout, Alloc: RawAlloc> AllocHandle for FatAllocHandle<Meta, Alloc> {
    type Alloc = Alloc;
    type Meta = Meta;

    #[inline]
    fn allocator(&self) -> &Self::Alloc {
        &self.alloc
    }

    #[inline]
    fn is_empty_handle(&self) -> bool {
        self.header.is_empty()
    }

    #[inline]
    unsafe fn header(&self) -> &Meta::Header {
        &self.header
    }

    #[inline]
    unsafe fn header_mut(&mut self) -> &mut Meta::Header {
        &mut self.header
    }

    #[inline]
    fn alloc_handle_in<A>(
        alloc_in: A,
        mut header: <Self::Meta as AllocLayout>::Header,
        exact: bool,
    ) -> Result<Self, StorageError>
    where
        A: RawAllocIn<RawAlloc = Self::Alloc>,
    {
        let mut layout = Meta::layout(&header)?;
        let (ptr, alloc) = alloc_in.try_alloc_in(layout)?;
        if !exact && layout.size() != ptr.len() {
            layout = unsafe { Layout::from_size_align_unchecked(ptr.len(), layout.align()) };
            Meta::update_header(&mut header, layout);
        }
        Ok(Self::new(header, ptr.cast(), alloc))
    }

    #[inline]
    fn resize_handle(
        &mut self,
        mut new_header: Meta::Header,
        exact: bool,
    ) -> Result<(), StorageError> {
        if new_header.is_empty() {
            if !self.is_dangling() {
                let layout = Meta::layout(&self.header)?;
                unsafe { self.alloc.release(self.data.cast(), layout) };
                self.data = NonNull::dangling();
            }
        } else {
            let new_layout = Meta::layout(&new_header)?;
            let ptr = if self.is_dangling() {
                self.alloc.try_alloc(new_layout)?
            } else {
                let old_layout: Layout = Meta::layout(&self.header)?;
                unsafe {
                    self.alloc
                        .try_resize(self.data.cast(), old_layout, new_layout)
                }?
            };
            if !exact && new_layout.size() != ptr.len() {
                let layout =
                    unsafe { Layout::from_size_align_unchecked(ptr.len(), new_layout.align()) };
                Meta::update_header(&mut new_header, layout);
            }
            self.data = ptr.cast();
        }
        self.header = new_header;
        Ok(())
    }
}

impl<Meta: AllocLayout, Alloc: RawAllocNew> AllocHandleNew for FatAllocHandle<Meta, Alloc> {
    const NEW: Self = Self::dangling(Meta::Header::EMPTY, Self::NEW_ALLOC);
    const NEW_ALLOC: Self::Alloc = Alloc::NEW;
}

impl<Meta: AllocLayout, Alloc: RawAlloc> AllocHandleParts for FatAllocHandle<Meta, Alloc> {
    #[inline]
    fn handle_from_parts(
        header: <Self::Meta as AllocLayout>::Header,
        data: NonNull<<Self::Meta as AllocLayout>::Data>,
        alloc: Self::Alloc,
    ) -> Self {
        Self {
            header,
            data,
            alloc,
        }
    }

    #[inline]
    fn handle_into_parts(
        self,
    ) -> (
        <Self::Meta as AllocLayout>::Header,
        NonNull<<Self::Meta as AllocLayout>::Data>,
        Self::Alloc,
    ) {
        let slf = ManuallyDrop::new(self);
        (unsafe { ptr::read(&slf.header) }, slf.data, unsafe {
            ptr::read(&slf.alloc)
        })
    }
}

impl<Meta: AllocLayout, Alloc: RawAlloc> Drop for FatAllocHandle<Meta, Alloc> {
    fn drop(&mut self) {
        if !self.is_dangling() {
            let layout = Meta::layout(&self.header).expect("error calculating layout");
            unsafe {
                self.alloc.release(self.data.cast(), layout);
            }
        }
    }
}

struct ThinPtr<Meta: AllocLayout>(NonNull<Meta::Data>);

impl<Meta: AllocLayout> ThinPtr<Meta> {
    const DATA_OFFSET: usize = offset_of!((Meta::Header, Meta::Data), 1);

    #[inline]
    pub const fn dangling() -> Self {
        Self(NonNull::dangling())
    }

    #[inline]
    pub fn is_dangling(&self) -> bool {
        ptr::eq(self.0.as_ptr(), NonNull::dangling().as_ptr())
    }

    #[inline]
    pub const fn from_alloc(ptr: NonNull<[u8]>) -> Self {
        Self(unsafe { NonNull::new_unchecked(ptr.as_ptr().byte_add(Self::DATA_OFFSET)) }.cast())
    }

    #[inline]
    pub const fn to_alloc(&self) -> NonNull<u8> {
        unsafe { NonNull::new_unchecked(self.header_ptr()) }.cast()
    }

    #[inline]
    pub const fn as_ptr(&self) -> *mut Meta::Data {
        self.0.as_ptr()
    }

    #[inline]
    pub const fn header_ptr(&self) -> *mut Meta::Header {
        unsafe { self.0.as_ptr().byte_sub(Self::DATA_OFFSET) }.cast()
    }
}

impl<Meta: AllocLayout> fmt::Debug for ThinPtr<Meta> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", &self.0)
    }
}

#[derive(Debug)]
pub struct ThinAllocHandle<Meta: AllocLayout, Alloc: RawAlloc> {
    data: ThinPtr<Meta>,
    alloc: Alloc,
}

impl<Meta: AllocLayout, Alloc: RawAlloc> ThinAllocHandle<Meta, Alloc> {
    #[inline]
    const fn new(data: ThinPtr<Meta>, alloc: Alloc) -> Self {
        ThinAllocHandle { data, alloc }
    }

    #[inline]
    pub const fn dangling(alloc: Alloc) -> Self {
        Self::new(ThinPtr::dangling(), alloc)
    }

    #[inline]
    fn combined_layout(data_layout: Layout, is_empty: bool) -> Result<Layout, StorageError> {
        if data_layout.size() == 0 && is_empty {
            Ok(unsafe { Layout::from_size_align_unchecked(0, align_of::<Meta::Header>()) })
        } else {
            match Layout::new::<Meta::Header>().extend(data_layout) {
                Ok((layout, _)) => Ok(layout),
                Err(err) => Err(StorageError::LayoutError(err)),
            }
        }
    }

    #[inline]
    fn update_header(ptr: NonNull<[u8]>, header: &mut Meta::Header, data_layout: Layout) {
        let data_len = ptr.len() - ThinPtr::<Meta>::DATA_OFFSET;
        let layout = unsafe { Layout::from_size_align_unchecked(data_len, data_layout.align()) };
        Meta::update_header(header, layout);
    }
}

impl<Meta: AllocLayout, Alloc: RawAlloc> RawBuffer for ThinAllocHandle<Meta, Alloc> {
    type RawData = Meta::Data;

    #[inline]
    fn data_ptr(&self) -> *const Self::RawData {
        self.data.as_ptr()
    }

    #[inline]
    fn data_ptr_mut(&mut self) -> *mut Self::RawData {
        self.data.as_ptr()
    }
}

impl<Meta: AllocLayout, Alloc: RawAlloc> AllocHandle for ThinAllocHandle<Meta, Alloc> {
    type Alloc = Alloc;
    type Meta = Meta;

    #[inline]
    fn allocator(&self) -> &Self::Alloc {
        &self.alloc
    }

    #[inline]
    fn is_empty_handle(&self) -> bool {
        // no header exists for a dangling data pointer
        self.data.is_dangling()
    }

    #[inline]
    unsafe fn header(&self) -> &<Self::Meta as AllocLayout>::Header {
        &*self.data.header_ptr()
    }

    #[inline]
    unsafe fn header_mut(&mut self) -> &mut <Self::Meta as AllocLayout>::Header {
        &mut *self.data.header_ptr()
    }

    #[inline]
    fn alloc_handle_in<A>(
        alloc_in: A,
        mut header: <Self::Meta as AllocLayout>::Header,
        exact: bool,
    ) -> Result<Self, StorageError>
    where
        A: RawAllocIn<RawAlloc = Self::Alloc>,
    {
        let data_layout = Meta::layout(&header)?;
        let alloc_layout = Self::combined_layout(data_layout, header.is_empty())?;
        let (ptr, alloc) = alloc_in.try_alloc_in(alloc_layout)?;
        if ptr.len() < ThinPtr::<Meta>::DATA_OFFSET {
            unsafe { alloc.release(ptr.cast(), alloc_layout) };
            return if ptr.len() == 0 && data_layout.size() == 0 {
                Ok(ThinAllocHandle::dangling(alloc))
            } else {
                Err(StorageError::CapacityLimit)
            };
        }
        if !exact && alloc_layout.size() != ptr.len() {
            Self::update_header(ptr, &mut header, data_layout);
        }
        let data = ThinPtr::<Meta>::from_alloc(ptr);
        unsafe { data.header_ptr().write(header) };
        Ok(Self::new(data, alloc))
    }

    #[inline]
    fn resize_handle(
        &mut self,
        mut new_header: Meta::Header,
        exact: bool,
    ) -> Result<(), StorageError> {
        let data_layout = Meta::layout(&new_header)?;
        let alloc_layout = Self::combined_layout(data_layout, new_header.is_empty())?;
        let ptr = if self.data.is_dangling() {
            self.alloc.try_alloc(alloc_layout)?
        } else {
            let old_layout = Self::combined_layout(Meta::layout(unsafe { self.header() })?, false)?;
            unsafe {
                self.alloc
                    .try_resize(self.data.to_alloc(), old_layout, alloc_layout)
            }?
        };
        if ptr.len() < ThinPtr::<Meta>::DATA_OFFSET {
            unsafe { self.alloc.release(ptr.cast(), alloc_layout) };
            return if ptr.len() == 0 && data_layout.size() == 0 {
                self.data = ThinPtr::dangling();
                Ok(())
            } else {
                Err(StorageError::CapacityLimit)
            };
        }
        if !exact && alloc_layout.size() != ptr.len() {
            Self::update_header(ptr, &mut new_header, data_layout);
        }
        let data = ThinPtr::<Meta>::from_alloc(ptr);
        unsafe { data.header_ptr().write(new_header) };
        self.data = data;
        Ok(())
    }
}

impl<Meta: AllocLayout, Alloc: RawAllocNew> AllocHandleNew for ThinAllocHandle<Meta, Alloc> {
    const NEW: Self = Self::dangling(Self::NEW_ALLOC);
    const NEW_ALLOC: Self::Alloc = Alloc::NEW;
}

impl<Meta: AllocLayout, Alloc: RawAlloc> Drop for ThinAllocHandle<Meta, Alloc> {
    fn drop(&mut self) {
        if !self.data.is_dangling() {
            let layout = Meta::layout(unsafe { self.header() })
                .and_then(|layout| Self::combined_layout(layout, false))
                .expect("error calculating layout");
            unsafe {
                self.alloc.release(self.data.to_alloc(), layout);
            }
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Thin;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct FixedAlloc<'a>(PhantomData<&'a mut ()>);

impl RawAlloc for FixedAlloc<'_> {
    #[inline]
    fn try_alloc(&self, _layout: Layout) -> Result<NonNull<[u8]>, StorageError> {
        Err(StorageError::CapacityLimit)
    }

    #[inline]
    unsafe fn try_resize(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, StorageError> {
        if old_layout.align() != new_layout.align() || new_layout.size() > old_layout.size() {
            Err(StorageError::CapacityLimit)
        } else {
            Ok(NonNull::slice_from_raw_parts(ptr, old_layout.size()))
        }
    }

    #[inline]
    unsafe fn release(&self, _ptr: NonNull<u8>, _layout: Layout) {}
}

impl<'a, T, const N: usize> RawAllocIn for &'a mut ByteStorage<T, N> {
    type RawAlloc = FixedAlloc<'a>;

    #[inline]
    fn try_alloc_in(self, layout: Layout) -> Result<(NonNull<[u8]>, Self::RawAlloc), StorageError> {
        let ptr = layout_aligned_bytes(self.as_uninit_slice(), layout)?;
        let alloc = FixedAlloc::default();
        Ok((ptr, alloc))
    }
}
