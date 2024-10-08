use core::alloc::Layout;
use core::ptr::NonNull;
use core::slice;

use zeroize::Zeroize;

use crate::error::StorageError;

use super::alloc::{RawAlloc, RawAllocDefault};
use super::{ArrayStorage, ByteStorage, WithAlloc};

/// An allocator which allocates via `A` and zeroizes all buffers when they are released.
#[derive(Debug, Default, Clone, Copy)]
pub struct ZeroizingAlloc<A>(pub A);

impl<A: RawAlloc> RawAlloc for ZeroizingAlloc<A> {
    #[inline]
    fn try_alloc(&self, layout: Layout) -> Result<NonNull<[u8]>, StorageError> {
        self.0.try_alloc(layout)
    }

    // default implementation of `try_resize`` will always allocate a new buffer and
    // release the old one, allowing it to be zeroized below.

    #[inline]
    unsafe fn release(&self, ptr: NonNull<u8>, layout: Layout) {
        if layout.size() > 0 {
            let mem = slice::from_raw_parts_mut(ptr.as_ptr(), layout.size());
            mem.zeroize();
        }
        self.0.release(ptr, layout)
    }
}

impl<A: RawAllocDefault> RawAllocDefault for ZeroizingAlloc<A> {
    const DEFAULT: Self = ZeroizingAlloc(A::DEFAULT);
}

impl<T, const N: usize> Zeroize for ArrayStorage<T, N> {
    #[inline]
    fn zeroize(&mut self) {
        self.0.zeroize()
    }
}

impl<T, const N: usize> Zeroize for ByteStorage<T, N> {
    #[inline]
    fn zeroize(&mut self) {
        self.as_uninit_slice().zeroize()
    }
}

impl<'a, Z> WithAlloc<'a> for &'a mut zeroize::Zeroizing<Z>
where
    Z: Zeroize + 'a,
    &'a mut Z: WithAlloc<'a>,
{
    type NewIn<A: 'a> = <&'a mut Z as WithAlloc<'a>>::NewIn<ZeroizingAlloc<A>>;

    #[inline]
    fn with_alloc_in<A: RawAlloc + 'a>(self, alloc: A) -> Self::NewIn<A> {
        (&mut **self).with_alloc_in(ZeroizingAlloc(alloc))
    }
}
