use core::alloc::Layout;
use core::fmt;
use core::mem::{offset_of, transmute, ManuallyDrop};
use core::ptr::{self, NonNull};

#[cfg(feature = "alloc")]
use alloc::alloc::{alloc as raw_alloc, dealloc as raw_dealloc};

use crate::error::StorageError;

use super::RawBuffer;

pub trait RawAlloc: Clone {
    fn try_alloc(&self, layout: Layout) -> Result<NonNull<u8>, StorageError>;

    unsafe fn try_resize(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<u8>, StorageError> {
        // Default implementation simply allocates and copies over the contents.
        // NB: not copying the entire previous buffer seems to defeat some automatic
        // optimization and results in much worse performance (on MacOS 14 at least).
        let new_ptr = self.try_alloc(new_layout)?;
        let cp_len = old_layout.size().min(new_layout.size());
        if cp_len > 0 {
            ptr::copy_nonoverlapping(ptr.as_ptr(), new_ptr.as_ptr(), cp_len);
        }
        self.release(ptr, old_layout);
        Ok(new_ptr)
    }

    unsafe fn release(&self, ptr: NonNull<u8>, layout: Layout);
}

pub trait RawAllocIn {
    type Alloc: RawAlloc;

    fn try_alloc_in(self, layout: Layout) -> Result<(NonNull<u8>, Self::Alloc), StorageError>;
}

pub trait RawAllocNew: RawAlloc {
    const NEW: Self;
}

impl<A: RawAlloc> RawAllocIn for A {
    type Alloc = A;

    #[inline]
    fn try_alloc_in(self, layout: Layout) -> Result<(NonNull<u8>, Self::Alloc), StorageError> {
        let data = self.try_alloc(layout)?;
        Ok((data, self))
    }
}

// FIXME use alloc::alloc::Global when allocator_api enabled
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Global;

#[cfg(feature = "alloc")]
impl RawAlloc for Global {
    #[inline]
    fn try_alloc(&self, layout: Layout) -> Result<NonNull<u8>, StorageError> {
        if layout.size() == 0 {
            // FIXME: use Layout::dangling when stabilized
            let ptr = unsafe { NonNull::new_unchecked(transmute(layout.align())) };
            return Ok(ptr);
        }
        match NonNull::new(unsafe { raw_alloc(layout) }) {
            Some(ptr) => Ok(ptr),
            None => Err(StorageError::AllocError),
        }
    }

    #[inline]
    unsafe fn release(&self, ptr: NonNull<u8>, layout: Layout) {
        if layout.size() > 0 {
            raw_dealloc(ptr.as_ptr(), layout);
        }
    }
}

impl RawAllocNew for Global {
    const NEW: Self = Global;
}

pub trait AllocHeader: Copy + Clone + Sized {
    const EMPTY: Self;

    fn is_empty(&self) -> bool;
}

impl AllocHeader for () {
    const EMPTY: Self = ();

    #[inline]
    fn is_empty(&self) -> bool {
        false
    }
}

pub trait AllocLayout {
    type Header: AllocHeader;
    type Data;

    fn layout(header: &Self::Header) -> Result<Layout, StorageError>;
}

pub trait AllocBuffer: RawBuffer<RawData = <Self::Meta as AllocLayout>::Data> {
    type Alloc: RawAlloc;
    type Meta: AllocLayout;

    fn allocator(&self) -> &Self::Alloc;

    fn is_empty_buffer(&self) -> bool;

    /// SAFETY: is_empty_buffer must return false
    unsafe fn header(&self) -> &<Self::Meta as AllocLayout>::Header;

    /// SAFETY: is_empty_buffer must return false
    unsafe fn header_mut(&mut self) -> &mut <Self::Meta as AllocLayout>::Header;

    fn new_buffer(alloc: Self::Alloc) -> Self;

    fn alloc_buffer(
        alloc: Self::Alloc,
        header: <Self::Meta as AllocLayout>::Header,
    ) -> Result<Self, StorageError>;

    fn resize_buffer(
        &mut self,
        new_header: <Self::Meta as AllocLayout>::Header,
    ) -> Result<(), StorageError>;

    #[inline]
    fn spawn_buffer(
        &self,
        header: <Self::Meta as AllocLayout>::Header,
    ) -> Result<Self, StorageError> {
        Self::alloc_buffer(self.allocator().clone(), header)
    }
}

pub trait AllocBufferNew: AllocBuffer {
    const NEW: Self;
}

pub trait AllocBufferParts: AllocBuffer {
    fn buffer_from_parts(
        header: <Self::Meta as AllocLayout>::Header,
        data: NonNull<<Self::Meta as AllocLayout>::Data>,
        alloc: Self::Alloc,
    ) -> Self;

    fn buffer_into_parts(
        self,
    ) -> (
        <Self::Meta as AllocLayout>::Header,
        NonNull<<Self::Meta as AllocLayout>::Data>,
        Self::Alloc,
    );
}

pub trait AllocMethod {
    type Alloc: RawAlloc;
    type Buffer<Meta: AllocLayout>: AllocBuffer<Alloc = Self::Alloc, Meta = Meta>;
}

impl<A: RawAlloc> AllocMethod for A {
    type Alloc = A;
    type Buffer<Meta: AllocLayout> = FatHandle<Meta, Self::Alloc>;
}

#[derive(Debug)]
pub struct FatHandle<Meta: AllocLayout, Alloc: RawAlloc> {
    header: Meta::Header,
    data: NonNull<Meta::Data>,
    alloc: Alloc,
}

impl<Meta: AllocLayout, Alloc: RawAlloc> FatHandle<Meta, Alloc> {
    const fn new(header: Meta::Header, data: NonNull<u8>, alloc: Alloc) -> Self {
        Self {
            header,
            data: data.cast(),
            alloc,
        }
    }

    pub const fn dangling(header: Meta::Header, alloc: Alloc) -> Self {
        Self::new(header, NonNull::<Meta::Data>::dangling().cast(), alloc)
    }

    #[inline]
    fn is_dangling(&self) -> bool {
        ptr::eq(self.data.as_ptr(), NonNull::dangling().as_ptr())
    }

    pub fn into_raw_parts(self) -> (Meta::Header, NonNull<u8>, Alloc) {
        let parts = ManuallyDrop::new(self);
        let header = unsafe { ptr::read(&parts.header) };
        let alloc = unsafe { ptr::read(&parts.alloc) };
        let data = parts.data.cast();
        (header, data, alloc)
    }
}

impl<Meta: AllocLayout, Alloc: RawAlloc> RawBuffer for FatHandle<Meta, Alloc> {
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

impl<Meta: AllocLayout, Alloc: RawAlloc> AllocBuffer for FatHandle<Meta, Alloc> {
    type Alloc = Alloc;
    type Meta = Meta;

    #[inline]
    fn allocator(&self) -> &Self::Alloc {
        &self.alloc
    }

    #[inline]
    fn is_empty_buffer(&self) -> bool {
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
    fn new_buffer(alloc: Self::Alloc) -> Self {
        FatHandle::dangling(Meta::Header::EMPTY, alloc)
    }

    #[inline]
    fn alloc_buffer(alloc: Self::Alloc, header: Meta::Header) -> Result<Self, StorageError> {
        if header.is_empty() {
            Ok(FatHandle::dangling(header, alloc))
        } else {
            let layout = Meta::layout(&header)?;
            let data = alloc.try_alloc(layout)?;
            Ok(FatHandle::new(header, data, alloc))
        }
    }

    #[inline]
    fn resize_buffer(&mut self, new_header: Meta::Header) -> Result<(), StorageError> {
        if new_header.is_empty() {
            if !self.is_dangling() {
                let layout = Meta::layout(&self.header)?;
                unsafe { self.alloc.release(self.data.cast(), layout) };
                self.data = NonNull::dangling();
            }
        } else if self.is_dangling() {
            let new_layout = Meta::layout(&new_header)?;
            self.data = self.alloc.try_alloc(new_layout)?.cast();
        } else {
            let old_layout = Meta::layout(&self.header)?;
            let new_layout = Meta::layout(&new_header)?;
            self.data = unsafe {
                self.alloc
                    .try_resize(self.data.cast(), old_layout, new_layout)?
            }
            .cast();
        }
        self.header = new_header;
        Ok(())
    }
}

impl<Meta: AllocLayout> AllocBufferNew for FatHandle<Meta, Global> {
    const NEW: Self = Self::dangling(Meta::Header::EMPTY, Global);
}

impl<Meta: AllocLayout, Alloc: RawAlloc> AllocBufferParts for FatHandle<Meta, Alloc> {
    #[inline]
    fn buffer_from_parts(
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
    fn buffer_into_parts(
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

impl<Meta: AllocLayout, Alloc: RawAlloc> Drop for FatHandle<Meta, Alloc> {
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
    pub const fn from_alloc(ptr: NonNull<u8>) -> Self {
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
pub struct ThinHandle<Meta: AllocLayout, Alloc: RawAlloc> {
    data: ThinPtr<Meta>,
    alloc: Alloc,
}

impl<Meta: AllocLayout, Alloc: RawAlloc> ThinHandle<Meta, Alloc> {
    const fn new(data: ThinPtr<Meta>, alloc: Alloc) -> Self {
        ThinHandle { data, alloc }
    }

    pub const fn dangling(alloc: Alloc) -> Self {
        Self::new(ThinPtr::dangling(), alloc)
    }

    #[inline]
    fn new_alloc(alloc: &Alloc, header: Meta::Header) -> Result<ThinPtr<Meta>, StorageError> {
        let layout = Meta::layout(&header).and_then(Self::combined_layout)?;
        let data = ThinPtr::from_alloc(alloc.try_alloc(layout)?);
        let header_mut: *mut Meta::Header = data.header_ptr();
        unsafe { header_mut.write(header) };
        Ok(data)
    }

    #[inline]
    fn combined_layout(data_layout: Layout) -> Result<Layout, StorageError> {
        match Layout::new::<Meta::Header>().extend(data_layout) {
            Ok(res) => Ok(res.0),
            Err(err) => Err(StorageError::LayoutError(err)),
        }
    }
}

impl<Meta: AllocLayout, Alloc: RawAlloc> RawBuffer for ThinHandle<Meta, Alloc> {
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

impl<Meta: AllocLayout, Alloc: RawAlloc> AllocBuffer for ThinHandle<Meta, Alloc> {
    type Alloc = Alloc;
    type Meta = Meta;

    #[inline]
    fn allocator(&self) -> &Self::Alloc {
        &self.alloc
    }

    #[inline]
    fn is_empty_buffer(&self) -> bool {
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
    fn new_buffer(alloc: Self::Alloc) -> Self {
        ThinHandle::dangling(alloc)
    }

    #[inline]
    fn alloc_buffer(alloc: Self::Alloc, header: Meta::Header) -> Result<Self, StorageError> {
        if header.is_empty() {
            Ok(ThinHandle::dangling(alloc))
        } else {
            let data = Self::new_alloc(&alloc, header)?;
            Ok(Self::new(data, alloc))
        }
    }

    #[inline]
    fn resize_buffer(&mut self, new_header: Meta::Header) -> Result<(), StorageError> {
        if new_header.is_empty() {
            if !self.data.is_dangling() {
                let layout =
                    Meta::layout(unsafe { self.header() }).and_then(Self::combined_layout)?;
                unsafe { self.alloc.release(self.data.to_alloc(), layout) };
                self.data = ThinPtr::dangling();
            }
        } else if self.data.is_dangling() {
            self.data = Self::new_alloc(&self.alloc, new_header)?;
        } else {
            let old_layout =
                Meta::layout(unsafe { self.header() }).and_then(Self::combined_layout)?;
            let new_layout = Meta::layout(&new_header).and_then(Self::combined_layout)?;
            self.data = ThinPtr::from_alloc(unsafe {
                self.alloc
                    .try_resize(self.data.to_alloc(), old_layout, new_layout)?
            });
            *unsafe { self.header_mut() } = new_header;
        }
        Ok(())
    }
}

impl<Meta: AllocLayout> AllocBufferNew for ThinHandle<Meta, Global> {
    const NEW: Self = Self::dangling(Global);
}

impl<Meta: AllocLayout, Alloc: RawAlloc> Drop for ThinHandle<Meta, Alloc> {
    fn drop(&mut self) {
        if !self.data.is_dangling() {
            let layout = Meta::layout(unsafe { self.header() })
                .and_then(Self::combined_layout)
                .expect("error calculating layout");
            unsafe {
                self.alloc.release(self.data.to_alloc(), layout);
            }
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Thin;

impl AllocMethod for Thin {
    type Alloc = Global;
    type Buffer<Meta: AllocLayout> = ThinHandle<Meta, Self::Alloc>;
}
