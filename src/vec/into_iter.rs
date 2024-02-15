use core::iter::FusedIterator;
use core::ops::Range;
use core::ptr;
use core::slice;

use crate::index::Index;

use super::buffer::VecBuffer;

#[derive(Debug)]
pub struct IntoIter<B: VecBuffer> {
    remain: Range<usize>,
    buf: B,
}

impl<B: VecBuffer> IntoIter<B> {
    pub(super) fn new(mut buf: B) -> Self {
        let end = buf.length().to_usize();
        if end > 0 {
            // SAFETY: buffer capacity is established as > 0
            unsafe { buf.set_length(B::Index::ZERO) };
        }
        Self {
            remain: Range { start: 0, end },
            buf,
        }
    }

    pub fn as_slice(&self) -> &[B::Data] {
        unsafe {
            slice::from_raw_parts(
                self.buf.data_ptr().add(self.remain.start),
                self.remain.len(),
            )
        }
    }

    pub fn as_mut_slice(&mut self) -> &mut [B::Data] {
        unsafe {
            slice::from_raw_parts_mut(
                self.buf.data_ptr_mut().add(self.remain.start),
                self.remain.len(),
            )
        }
    }

    pub const fn len(&self) -> usize {
        self.remain.end - self.remain.start
    }

    fn clear(&mut self) {
        let remain_len = self.len();
        if remain_len > 0 {
            unsafe {
                ptr::drop_in_place(self.as_mut_slice().as_mut_ptr());
            }
            self.remain.start = self.remain.end;
        }
    }
}

impl<B: VecBuffer> AsRef<[B::Data]> for IntoIter<B> {
    fn as_ref(&self) -> &[B::Data] {
        self.as_slice()
    }
}

impl<B: VecBuffer> AsMut<[B::Data]> for IntoIter<B> {
    fn as_mut(&mut self) -> &mut [B::Data] {
        self.as_mut_slice()
    }
}

impl<B: VecBuffer> Iterator for IntoIter<B> {
    type Item = B::Data;

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

impl<B: VecBuffer> DoubleEndedIterator for IntoIter<B> {
    #[inline]
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

impl<B: VecBuffer> ExactSizeIterator for IntoIter<B> {}

impl<B: VecBuffer> FusedIterator for IntoIter<B> {}

impl<B: VecBuffer> Drop for IntoIter<B> {
    fn drop(&mut self) {
        self.clear();
    }
}
