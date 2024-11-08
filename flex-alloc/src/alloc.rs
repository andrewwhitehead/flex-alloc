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

#[cfg(all(not(test), feature = "alloc"))]
pub use alloc_crate::alloc::handle_alloc_error;

#[cfg(any(test, not(feature = "alloc")))]
/// Custom allocation error handler.
pub fn handle_alloc_error(layout: Layout) -> ! {
    panic!("memory allocation of {} bytes failed", layout.size());
}

#[cfg(all(feature = "alloc", not(feature = "allocator-api2")))]
#[inline]
pub(crate) fn layout_dangling(layout: Layout) -> NonNull<u8> {
    // FIXME: use Layout::dangling when stabilized
    // SAFETY: layout alignments are guaranteed to be non-zero.
    #[allow(clippy::useless_transmute)]
    unsafe {
        NonNull::new_unchecked(transmute(layout.align()))
    }
}

/// The AllocError error indicates an allocation failure that may be due to
/// resource exhaustion or to something wrong when combining the given input
/// arguments with this allocator.
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

/// An implementation of Allocator can allocate, grow, shrink, and deallocate
/// arbitrary blocks of data described via `Layout`.
///
/// Allocator is designed to be implemented on ZSTs, references, or smart
/// pointers because having an allocator like `MyAlloc([u8; N])` cannot be
/// moved, without updating the pointers to the allocated memory.
///
/// Unlike `GlobalAlloc`, zero-sized allocations are allowed in `Allocator`.
/// If an underlying allocator does not support this (like `jemalloc`) or
/// return a null pointer (such as `libc::malloc`), this must be caught by
/// the implementation.
///
/// # Currently allocated memory
/// Some of the methods require that a memory block be currently allocated via
/// an allocator. This means that:
/// - The starting address for that memory block was previously returned by
///   `allocate`, `grow`, or `shrink`, and
/// - The memory block has not been subsequently deallocated, where blocks are
///   either deallocated directly by being passed to deallocate or were change
///   by being passed to `grow` or `shrink` that returns `Ok`. If `grow` or
///   `shrink` have returned `Err`, the passed pointer remains valid.
///
/// # Memory fitting
/// Some of the methods require that a layout fit a memory block. What it means
/// for a layout to "fit" a memory block means (or equivalently, for a memory
/// block to "fit" a layout) is that the following conditions must hold:
/// - The block must be allocated with the same alignment as `layout.align()`, and
/// - The provided `layout.size()` must fall in the range `min..=max`, where:
///   - `min` is the size of the layout most recently used to allocate the block, and
///   - `max` is the latest actual size returned from `allocate`, `grow`, or `shrink`.
///
/// #Safety
/// - Memory blocks returned from an allocator must point to valid memory and retain
/// their validity until the instance and all of its clones are dropped,
/// - Cloning or moving the allocator must not invalidate memory blocks returned from
/// this allocator. A cloned allocator must behave like the same allocator, and
/// - Any pointer to a memory block which is currently allocated may be passed to any
/// other method of the allocator.
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
        // SAFETY: the result of `allocate` must be properly aligned
        unsafe { ptr::write_bytes(ptr.cast::<u8>().as_ptr(), 0, ptr.len()) };
        Ok(ptr)
    }

    /// Try to extend the size of an allocation to accomodate a new, larger layout.
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

    /// Try to extend the size of an allocation to accomodate a new, larger layout.
    /// Fill the extra capacity with zeros.
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

    /// Try to reduce the size of an allocation to accomodate a new, smaller layout.
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

    /// Obtain a reference to this allocator type.
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
        // SAFETY: the result of `allocate` must be properly aligned
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
pub trait SpillAlloc<'a>: Sized {
    /// The concrete type of resulting allocation target.
    type NewIn<A: 'a>;

    /// Consume the allocator instance, returning a new allocator
    /// which spills into the Global allocator.
    #[inline]
    fn spill_alloc(self) -> Self::NewIn<Global> {
        Self::spill_alloc_in(self, Global)
    }

    /// Consume the allocator instance, returning a new allocator
    /// which spills into the provided allocator instance `alloc`.
    fn spill_alloc_in<A: Allocator + 'a>(self, alloc: A) -> Self::NewIn<A>;
}

/// The global memory allocator.
///
/// When the `alloc` feature is enabled, this type implements the `Allocator`
/// trait by forwarding calls to the allocator registered with the
/// `#[global_allocator]` attribute if there is one, or the `std` crate's default.
#[cfg(any(not(feature = "alloc"), not(feature = "allocator-api2")))]
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "alloc", derive(Default, Copy))]
pub struct Global;

#[cfg(all(feature = "alloc", not(feature = "allocator-api2")))]
unsafe impl Allocator for Global {
    #[inline]
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        let ptr = if layout.size() == 0 {
            layout_dangling(layout)
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
// even when the `alloc` feature is not enabled. Any usage as an allocator
// will result in a panic.
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
pub struct Fixed<'a>(PhantomData<&'a mut ()>);

unsafe impl Allocator for Fixed<'_> {
    #[inline(always)]
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        if layout.size() == 0 {
            Ok(NonNull::slice_from_raw_parts(NonNull::dangling(), 0))
        } else {
            Err(AllocError)
        }
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

impl<'a> AllocatorDefault for Fixed<'a> {
    const DEFAULT: Self = Self(PhantomData);
}

impl Clone for Fixed<'_> {
    fn clone(&self) -> Self {
        Fixed::DEFAULT
    }
}

/// An allocator which may represent either a fixed allocation or a dynamic
/// allocation with an allocator instance `A`.
#[derive(Debug)]
pub struct Spill<'a, A> {
    alloc: A,
    initial: *const u8,
    _fixed: Fixed<'a>,
}

impl<'a, A> Spill<'a, A> {
    pub(crate) const fn new(alloc: A, initial: *const u8, fixed: Fixed<'a>) -> Self {
        Self {
            alloc,
            initial,
            _fixed: fixed,
        }
    }
}

impl<A: Default + Allocator> Default for Spill<'_, A> {
    #[inline]
    fn default() -> Self {
        Self::new(A::default(), ptr::null(), Fixed::DEFAULT)
    }
}

unsafe impl<A: Allocator> Allocator for Spill<'_, A> {
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

impl<'a, A: Default + Allocator> Clone for Spill<'a, A> {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl<'a, A: AllocatorDefault> AllocatorDefault for Spill<'a, A> {
    const DEFAULT: Self = Self::new(A::DEFAULT, ptr::null(), Fixed::DEFAULT);
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
impl<'a, Z> SpillAlloc<'a> for &'a mut zeroize::Zeroizing<Z>
where
    Z: Zeroize + 'a,
    &'a mut Z: SpillAlloc<'a>,
{
    type NewIn<A: 'a> = <&'a mut Z as SpillAlloc<'a>>::NewIn<ZeroizingAlloc<A>>;

    #[inline]
    fn spill_alloc_in<A: Allocator + 'a>(self, alloc: A) -> Self::NewIn<A> {
        (&mut **self).spill_alloc_in(ZeroizingAlloc(alloc))
    }
}

#[cfg(feature = "zeroize")]
impl<A: Allocator> AllocatorZeroizes for ZeroizingAlloc<A> {}

/// Convert between types in this crate and standard containers.
pub trait ConvertAlloc<Target> {
    /// Convert directly into the target type, ideally without reallocating.
    fn convert(self) -> Target;
}
