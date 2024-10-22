//! Support for memory allocation.

use core::alloc::Layout;
#[cfg(not(feature = "allocator-api2"))]
use core::fmt;
use core::marker::PhantomData;
use core::ptr::{self, NonNull};
#[cfg(feature = "zeroize")]
use core::slice;

#[cfg(all(feature = "alloc", not(feature = "allocator-api2")))]
use core::mem::transmute;

#[cfg(all(feature = "alloc", not(feature = "allocator-api2")))]
use alloc_crate::alloc::{alloc as raw_alloc, dealloc as raw_dealloc};

#[cfg(all(feature = "alloc", feature = "allocator-api2"))]
pub use allocator_api2::alloc::Global;
#[cfg(feature = "allocator-api2")]
pub use allocator_api2::alloc::{AllocError, Allocator};

#[cfg(feature = "zeroize")]
use zeroize::Zeroize;

/// An error resulting from a failed memory allocation.
#[cfg(not(feature = "allocator-api2"))]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct AllocError;

#[cfg(not(feature = "allocator-api2"))]
impl fmt::Display for AllocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("memory allocation failed")
    }
}

#[cfg(all(feature = "std", not(feature = "allocator-api2")))]
impl std::error::Error for AllocError {}

#[cfg(not(feature = "allocator-api2"))]
pub unsafe trait Allocator {
    /// Try to allocate a slice of memory within this allocator instance,
    /// returning the new allocation.
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError>;

    /// Release an allocation produced by this allocator.
    ///
    /// # Safety
    /// The value `ptr` must represent an allocation produced by this allocator, otherwise
    /// a memory access error may occur. The value `old_layout` must correspond to the
    /// layout produced by the previous allocation.
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout);

    /// Try to allocate a slice of memory within this allocator instance,
    /// returning the new allocation. The memory will be initialized with zeroes.
    #[inline]
    fn allocate_zeroed(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        let ptr = self.allocate(layout)?;
        unsafe { ptr::write_bytes(ptr.cast::<u8>().as_ptr(), 0, ptr.len()) };
        Ok(ptr)
    }

    unsafe fn grow(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        debug_assert!(
            new_layout.size() >= old_layout.size(),
            "`new_layout.size()` must be greater than or equal to `old_layout.size()`"
        );

        // This default implementation simply allocates and copies over the contents.
        // NB: not copying the entire previous buffer seems to defeat some automatic
        // optimization and results in much worse performance (on MacOS 14 at least).
        let new_ptr = self.allocate(new_layout)?;
        let cp_len = old_layout.size().min(new_ptr.len());
        if cp_len > 0 {
            ptr::copy_nonoverlapping(ptr.as_ptr(), new_ptr.as_ptr().cast(), cp_len);
        }
        self.deallocate(ptr, old_layout);
        Ok(new_ptr)
    }

    unsafe fn grow_zeroed(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        debug_assert!(
            new_layout.size() >= old_layout.size(),
            "`new_layout.size()` must be greater than or equal to `old_layout.size()`"
        );

        // This default implementation simply allocates and copies over the contents.
        // NB: not copying the entire previous buffer seems to defeat some automatic
        // optimization and results in much worse performance (on MacOS 14 at least).
        let new_ptr = self.allocate_zeroed(new_layout)?;
        let cp_len = old_layout.size().min(new_ptr.len());
        if cp_len > 0 {
            ptr::copy_nonoverlapping(ptr.as_ptr(), new_ptr.as_ptr().cast(), cp_len);
        }
        self.deallocate(ptr, old_layout);
        Ok(new_ptr)
    }

    unsafe fn shrink(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        debug_assert!(
            new_layout.size() <= old_layout.size(),
            "`new_layout.size()` must be smaller than or equal to `old_layout.size()`"
        );

        // This default implementation simply allocates and copies over the contents.
        // NB: not copying the entire previous buffer seems to defeat some automatic
        // optimization and results in much worse performance (on MacOS 14 at least).
        let new_ptr = self.allocate(new_layout)?;
        let cp_len = old_layout.size().min(new_ptr.len());
        if cp_len > 0 {
            ptr::copy_nonoverlapping(ptr.as_ptr(), new_ptr.as_ptr().cast(), cp_len);
        }
        self.deallocate(ptr, old_layout);
        Ok(new_ptr)
    }

    #[inline(always)]
    fn by_ref(&self) -> &Self
    where
        Self: Sized,
    {
        self
    }
}

/// For all types which are an allocator or reference an allocator, enable their
/// usage as a target for allocation.
pub trait AllocateIn: Sized {
    /// The type of the allocator instance
    type Alloc: Allocator;

    /// Try to allocate a slice of a memory corresponding to `layout`, returning
    /// the new allocation and the allocator instance
    fn allocate_in(self, layout: Layout) -> Result<(NonNull<[u8]>, Self::Alloc), AllocError>;

    /// Try to allocate a slice of a memory corresponding to `layout`, returning
    /// the new allocation and the allocator instance. The memory will be initialized
    /// with zeroes.
    #[inline]
    fn allocate_zeroed_in(
        self,
        layout: Layout,
    ) -> Result<(NonNull<[u8]>, Self::Alloc), AllocError> {
        let (ptr, alloc) = self.allocate_in(layout)?;
        unsafe { ptr::write_bytes(ptr.cast::<u8>().as_ptr(), 0, ptr.len()) };
        Ok((ptr, alloc))
    }
}

impl<A: Allocator> AllocateIn for A {
    type Alloc = A;

    #[inline]
    fn allocate_in(self, layout: Layout) -> Result<(NonNull<[u8]>, Self::Alloc), AllocError> {
        let data = self.allocate(layout)?;
        Ok((data, self))
    }

    #[inline]
    fn allocate_zeroed_in(
        self,
        layout: Layout,
    ) -> Result<(NonNull<[u8]>, Self::Alloc), AllocError> {
        let data = self.allocate_zeroed(layout)?;
        Ok((data, self))
    }
}

/// A trait implemented by allocators supporting a constant initializer.
/// This cannot use ConstDefault as it is not implemented for the external
/// `Global` allocator.
pub trait AllocatorDefault: Allocator + Clone + Default {
    /// The constant initializer for this allocator.
    const DEFAULT: Self;
}

/// A marker trait for allocators which zeroize on deallocation.
pub trait AllocatorZeroizes: Allocator {}

/// Attach an allocator to a fixed allocation buffer. Once the initial
/// buffer is exhausted, additional buffer(s) may be requested from the
/// new allocator instance.
pub trait WithAlloc<'a>: Sized {
    /// The concrete type of resulting allocation target.
    type NewIn<A: 'a>;

    /// Consume the allocator instance, returning a new allocator
    /// which spills into the Global allocator.
    #[inline]
    fn with_alloc(self) -> Self::NewIn<Global> {
        Self::with_alloc_in(self, Global)
    }

    /// Consume the allocator instance, returning a new allocator
    /// which spills into the provided allocator instance `alloc`.
    fn with_alloc_in<A: Allocator + 'a>(self, alloc: A) -> Self::NewIn<A>;
}

/// The standard heap allocator. When the `alloc` feature is not enabled,
/// usage will result in a panic.
#[cfg(any(not(feature = "alloc"), not(feature = "allocator-api2")))]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "alloc", derive(Default, Copy))]
pub struct Global;

#[cfg(all(feature = "alloc", not(feature = "allocator-api2")))]
unsafe impl Allocator for Global {
    #[inline]
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        let ptr = if layout.size() == 0 {
            // FIXME: use Layout::dangling when stabilized
            #[allow(clippy::useless_transmute)]
            unsafe {
                NonNull::new_unchecked(transmute(layout.align()))
            }
        } else {
            let Some(ptr) = NonNull::new(unsafe { raw_alloc(layout) }) else {
                return Err(AllocError);
            };
            ptr
        };
        Ok(NonNull::slice_from_raw_parts(ptr, layout.size()))
    }

    #[inline]
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        if layout.size() > 0 {
            raw_dealloc(ptr.as_ptr(), layout);
        }
    }
}

#[cfg(not(feature = "alloc"))]
// Stub implementation to allow Global as the default allocator type
// even when the `alloc` feature is not enabled.
unsafe impl Allocator for Global {
    fn allocate(&self, _layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        unimplemented!();
    }

    unsafe fn deallocate(&self, _ptr: NonNull<u8>, _layout: Layout) {
        unimplemented!();
    }
}

#[cfg(feature = "alloc")]
impl AllocatorDefault for Global {
    const DEFAULT: Self = Global;
}

/// An allocator backed by a fixed storage buffer.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct FixedAlloc<'a>(PhantomData<&'a mut ()>);

unsafe impl Allocator for FixedAlloc<'_> {
    #[inline(always)]
    fn allocate(&self, _layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        Err(AllocError)
    }

    #[inline(always)]
    unsafe fn grow(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        if old_layout.align() != new_layout.align() || new_layout.size() > old_layout.size() {
            Err(AllocError)
        } else {
            Ok(NonNull::slice_from_raw_parts(ptr, old_layout.size()))
        }
    }

    #[inline(always)]
    unsafe fn shrink(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        if old_layout.align() != new_layout.align() || new_layout.size() > old_layout.size() {
            Err(AllocError)
        } else {
            Ok(NonNull::slice_from_raw_parts(ptr, old_layout.size()))
        }
    }

    #[inline]
    unsafe fn deallocate(&self, _ptr: NonNull<u8>, _layout: Layout) {}
}

impl<'a> AllocatorDefault for FixedAlloc<'a> {
    const DEFAULT: Self = Self(PhantomData);
}

impl Clone for FixedAlloc<'_> {
    fn clone(&self) -> Self {
        FixedAlloc::DEFAULT
    }
}

/// An allocator which may represent either a fixed allocation or a dynamic
/// allocation with an allocator instance `A`.
#[derive(Debug)]
pub struct SpillAlloc<'a, A> {
    alloc: A,
    initial: *const u8,
    _fixed: FixedAlloc<'a>,
}

impl<'a, A> SpillAlloc<'a, A> {
    pub(crate) const fn new(alloc: A, initial: *const u8, fixed: FixedAlloc<'a>) -> Self {
        Self {
            alloc,
            initial,
            _fixed: fixed,
        }
    }
}

impl<A: Default + Allocator> Default for SpillAlloc<'_, A> {
    #[inline]
    fn default() -> Self {
        Self::new(A::default(), ptr::null(), FixedAlloc::DEFAULT)
    }
}

unsafe impl<A: Allocator> Allocator for SpillAlloc<'_, A> {
    #[inline]
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        self.alloc.allocate(layout)
    }

    #[inline]
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        if !ptr::eq(self.initial, ptr.as_ptr()) {
            self.alloc.deallocate(ptr, layout)
        }
    }
}

impl<'a, A: Default + Allocator> Clone for SpillAlloc<'a, A> {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl<'a, A: AllocatorDefault> AllocatorDefault for SpillAlloc<'a, A> {
    const DEFAULT: Self = Self::new(A::DEFAULT, ptr::null(), FixedAlloc::DEFAULT);
}

#[cfg(feature = "zeroize")]
/// An allocator which allocates via `A` and zeroizes all buffers when they are released.
#[derive(Debug, Default, Clone, Copy)]
pub struct ZeroizingAlloc<A>(pub A);

#[cfg(feature = "zeroize")]
unsafe impl<A: Allocator> Allocator for ZeroizingAlloc<A> {
    #[inline]
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        self.0.allocate(layout)
    }

    // The default implementation of `try_resize`` will always allocate a new buffer
    // and release the old one, allowing it to be zeroized below.

    #[inline]
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        if layout.size() > 0 {
            let mem = slice::from_raw_parts_mut(ptr.as_ptr(), layout.size());
            mem.zeroize();
        }
        self.0.deallocate(ptr, layout)
    }
}

#[cfg(feature = "zeroize")]
impl<A: AllocatorDefault> AllocatorDefault for ZeroizingAlloc<A> {
    const DEFAULT: Self = ZeroizingAlloc(A::DEFAULT);
}

#[cfg(feature = "zeroize")]
impl<'a, Z> WithAlloc<'a> for &'a mut zeroize::Zeroizing<Z>
where
    Z: Zeroize + 'a,
    &'a mut Z: WithAlloc<'a>,
{
    type NewIn<A: 'a> = <&'a mut Z as WithAlloc<'a>>::NewIn<ZeroizingAlloc<A>>;

    #[inline]
    fn with_alloc_in<A: Allocator + 'a>(self, alloc: A) -> Self::NewIn<A> {
        (&mut **self).with_alloc_in(ZeroizingAlloc(alloc))
    }
}

#[cfg(feature = "zeroize")]
impl<A: Allocator> AllocatorZeroizes for ZeroizingAlloc<A> {}

/// Convert between types in this crate and standard containers.
pub trait ConvertAlloc<Target> {
    /// Convert directly into the target type, ideally without reallocating.
    fn convert(self) -> Target;
}