use core::alloc::Layout;
use core::mem::MaybeUninit;
use core::ptr::NonNull;

use crate::error::StorageError;

#[inline]
pub fn layout_aligned_bytes(
    buf: &mut [MaybeUninit<u8>],
    layout: Layout,
) -> Result<NonNull<[u8]>, StorageError> {
    let buf_len = buf.len();
    let start = NonNull::from(buf).cast::<u8>();
    let offset = start.align_offset(layout.align());
    let max_cap = buf_len.saturating_sub(offset);
    if max_cap < layout.size() || offset > buf_len {
        Err(StorageError::CapacityLimit)
    } else {
        let head = unsafe { start.add(offset) };
        Ok(NonNull::slice_from_raw_parts(head, max_cap))
    }
}

pub const fn min_non_zero_cap<T>() -> usize {
    if size_of::<T>() == 1 {
        8
    } else if size_of::<T>() <= 1024 {
        4
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_min_cap() {
        assert_eq!(min_non_zero_cap::<u8>(), 8);
        assert_eq!(min_non_zero_cap::<usize>(), 4);
        assert_eq!(min_non_zero_cap::<[u8; 1025]>(), 1);
    }
}
