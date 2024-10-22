use core::fmt;
use core::fmt::Debug;
use core::iter::FusedIterator;
use core::ops::Range;
use core::ptr;
use core::slice;

use crate::index::Index;

use super::buffer::VecBuffer;
use super::index_panic;

/// A struct used for draining items from a Vec as an iterator.
pub struct Drain<'d, B: VecBuffer> {
    pub(super) range: Range<usize>,
    pub(super) remain: Range<usize>,
    pub(super) tail_length: usize,
    pub(super) buf: &'d mut B,
}

impl<'d, B: VecBuffer> Drain<'d, B> {
    pub(super) fn new(buf: &'d mut B, range: Range<usize>) -> Self {
        let len = buf.length().to_usize();
        if range.end < range.start || range.end > len {
            index_panic();
        }
        if len > 0 {
            // SAFETY: buffer capacity is established as > 0
            unsafe { buf.set_length(B::Index::from_usize(range.start)) };
        }
        let tail_length = len - range.end;
        Self {
            range: range.clone(),
            remain: range,
            tail_length,
            buf,
        }
    }

    /// Access the remaining items as a slice reference.
    pub fn as_slice(&self) -> &[B::Item] {
        unsafe {
            slice::from_raw_parts(
                self.buf.data_ptr().add(self.remain.start),
                self.remain.len(),
            )
        }
    }

    /// Access the remaining items as a mutable slice reference.
    pub fn as_mut_slice(&mut self) -> &mut [B::Item] {
        unsafe {
            slice::from_raw_parts_mut(
                self.buf.data_ptr_mut().add(self.remain.start),
                self.remain.len(),
            )
        }
    }

    /// Check if there are remaining items in the iterator.
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get the number of remaining items in the iterator.
    pub const fn len(&self) -> usize {
        self.remain.end - self.remain.start
    }

    pub(super) fn clear_remain(&mut self) {
        let remain_len = self.len();
        if remain_len > 0 {
            unsafe {
                ptr::drop_in_place(self.as_mut_slice().as_mut_ptr());
            }
            self.remain.start = self.remain.end;
        }
    }

    /// Abort the drain operation, leaving the remaining items contained in the `Vec` instance.
    pub fn keep_rest(mut self) {
        let len = self.len();
        let shift = self.remain.start - self.range.start;
        if len > 0 && shift > 0 {
            unsafe {
                let head = self.buf.data_ptr_mut().add(self.range.start);
                ptr::copy(head.add(shift), head, len);
            }
        }
        self.range.start += len;
        self.remain = Range {
            start: self.range.start,
            end: self.range.start,
        };
    }
}

impl<'d, B: VecBuffer> AsRef<[B::Item]> for Drain<'d, B> {
    fn as_ref(&self) -> &[B::Item] {
        self.as_slice()
    }
}

impl<'d, B: VecBuffer> fmt::Debug for Drain<'d, B>
where
    B::Item: Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Drain").field(&self.as_slice()).finish()
    }
}

impl<'d, B: VecBuffer> Iterator for Drain<'d, B> {
    type Item = B::Item;

    fn next(&mut self) -> Option<Self::Item> {
        let index = self.remain.start;
        if index != self.remain.end {
            self.remain.start = index + 1;
            unsafe {
                let read = self.buf.data_ptr().add(index);
                Some(ptr::read(read))
            }
        } else {
            None
        }
    }

    #[inline]
    fn count(self) -> usize
    where
        Self: Sized,
    {
        self.len()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}

impl<'d, B: VecBuffer> DoubleEndedIterator for Drain<'d, B> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let mut index = self.remain.end;
        if index != self.remain.start {
            index -= 1;
            self.remain.end = index;
            unsafe {
                let read = self.buf.data_ptr().add(index);
                Some(ptr::read(read))
            }
        } else {
            None
        }
    }
}

impl<'d, B: VecBuffer> ExactSizeIterator for Drain<'d, B> {}

impl<'d, B: VecBuffer> FusedIterator for Drain<'d, B> {}

impl<'d, B: VecBuffer> Drop for Drain<'d, B> {
    fn drop(&mut self) {
        self.clear_remain();
        if self.tail_length > 0 {
            unsafe {
                let head = self.buf.data_ptr_mut().add(self.range.start);
                ptr::copy(head.add(self.range.len()), head, self.tail_length);
            }
        }
        let len = self.range.start + self.tail_length;
        if len > 0 {
            // SAFETY: capacity is established as > 0
            unsafe {
                self.buf
                    .set_length(B::Index::from_usize(self.range.start + self.tail_length))
            }
        }
    }
}
