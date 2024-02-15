use core::alloc::Layout;
use core::mem::{size_of, MaybeUninit};
use core::ptr::NonNull;

use crate::error::StorageError;

#[inline]
pub fn array_layout<T>(count: usize) -> Result<Layout, StorageError> {
    Layout::array::<T>(count).map_err(StorageError::LayoutError)
}

#[inline]
pub fn aligned_byte_slice<T>(buf: &mut [MaybeUninit<u8>]) -> (NonNull<T>, usize) {
    if size_of::<T>() == 0 {
        (NonNull::dangling(), usize::MAX)
    } else {
        let aligned: &mut [MaybeUninit<T>] = unsafe { buf.align_to_mut() }.1;
        let len = aligned.len();
        if len == 0 {
            (NonNull::dangling(), 0)
        } else {
            (
                unsafe { NonNull::new_unchecked(aligned.as_mut_ptr()).cast() },
                len,
            )
        }
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
