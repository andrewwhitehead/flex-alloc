use core::alloc::Layout;
use core::mem::MaybeUninit;
use core::ptr::NonNull;

use crate::error::StorageError;

#[inline]
pub fn array_layout<T>(count: usize) -> Result<Layout, StorageError> {
    Layout::array::<T>(count).map_err(StorageError::LayoutError)
}

#[inline]
pub fn layout_aligned_bytes(
    buf: &mut [MaybeUninit<u8>],
    layout: Layout,
) -> Result<NonNull<[u8]>, StorageError> {
    let start = buf.as_mut_ptr();
    let offset = start.align_offset(layout.align());
    let max_cap = buf.len().saturating_sub(offset);
    if max_cap < layout.size() || offset > buf.len() {
        Err(StorageError::CapacityLimit)
    } else {
        Ok(NonNull::slice_from_raw_parts(
            unsafe { NonNull::new_unchecked((start as *mut u8).add(offset)) },
            max_cap,
        ))
    }
}

pub const fn min_non_zero_cap<T>() -> usize {
    if core::mem::size_of::<T>() == 1 {
        8
    } else if core::mem::size_of::<T>() <= 1024 {
        4
    } else {
        1
    }
}
