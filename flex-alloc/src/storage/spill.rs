use core::alloc::Layout;
use core::marker::PhantomData;
use core::ptr::{self, NonNull};

use crate::alloc::{AllocError, AllocateIn, Allocator, AllocatorDefault, FixedAlloc, SpillAlloc};

/// An allocator which consumes the provided fixed storage before deferring to the
/// contained `A` instance allocator for further allocations
#[derive(Debug, Default, Clone)]
pub struct SpillStorage<'a, I: 'a, A> {
    pub(crate) buffer: I,
    pub(crate) alloc: A,
    _pd: PhantomData<&'a mut ()>,
}

impl<I, A: Allocator> SpillStorage<'_, I, A> {
    #[inline]
    pub(crate) fn new_in(buffer: I, alloc: A) -> Self {
        Self {
            buffer,
            alloc,
            _pd: PhantomData,
        }
    }
}

impl<'a, I, A> AllocateIn for SpillStorage<'a, I, A>
where
    I: AllocateIn<Alloc = FixedAlloc<'a>>,
    A: Allocator,
{
    type Alloc = SpillAlloc<'a, A>;

    #[inline]
    fn allocate_in(self, layout: Layout) -> Result<(NonNull<[u8]>, Self::Alloc), AllocError> {
        match self.buffer.allocate_in(layout) {
            Ok((ptr, fixed)) => {
                let alloc = SpillAlloc::new(self.alloc, ptr.as_ptr().cast(), fixed);
                Ok((ptr, alloc))
            }
            Err(_) => {
                let ptr = self.alloc.allocate(layout)?;
                let alloc = SpillAlloc::new(self.alloc, ptr::null(), FixedAlloc::DEFAULT);
                Ok((ptr, alloc))
            }
        }
    }
}
