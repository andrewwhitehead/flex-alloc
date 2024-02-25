use core::mem::MaybeUninit;
use core::ptr;
use core::slice;

use crate::index::Index;

use super::buffer::VecBuffer;

pub struct Inserter<'a, T> {
    buf: &'a mut [MaybeUninit<T>],
    start: usize,
    end: usize,
    tail: usize,
}

impl<'a, T> Inserter<'a, T> {
    #[inline]
    pub unsafe fn new(data: *mut T, cap: usize, len: usize, tail: usize) -> Self {
        Self {
            buf: unsafe { slice::from_raw_parts_mut(data.cast::<MaybeUninit<T>>(), cap) },
            start: len,
            end: len,
            tail,
        }
    }

    #[inline]
    pub fn for_buffer<B>(buf: &'a mut B) -> Self
    where
        B: VecBuffer<Data = T>,
    {
        let cap = buf.capacity().to_usize();
        let len = buf.length().to_usize();
        unsafe { Self::new(buf.data_ptr_mut(), cap, len, 0) }
    }

    #[inline]
    pub fn for_buffer_with_range<B>(buf: &'a mut B, start: usize, count: usize, tail: usize) -> Self
    where
        B: VecBuffer<Data = T>,
    {
        let cap = start + count;
        assert!(cap + tail <= buf.capacity().to_usize());
        unsafe { Self::new(buf.data_ptr_mut(), cap, start, tail) }
    }

    #[inline]
    pub fn for_mut_slice(buf: &'a mut [MaybeUninit<T>]) -> Self {
        let cap = buf.len();
        unsafe { Self::new(buf.as_mut_ptr().cast(), cap, 0, 0) }
    }

    #[inline]
    pub fn extend_from_slice(&mut self, data: &[T])
    where
        T: Clone,
    {
        for item in data {
            self.push_clone(item);
        }
    }

    #[inline]
    pub fn push(&mut self, val: T) {
        self.buf[self.end].write(val);
        self.end += 1;
    }

    #[inline]
    pub fn push_clone(&mut self, val: &T)
    where
        T: Clone,
    {
        self.buf[self.end].write(val.clone());
        self.end += 1;
    }

    #[inline]
    pub const fn full(&self) -> bool {
        self.end == self.buf.len()
    }

    #[inline]
    pub fn complete(mut self) -> (usize, usize) {
        let count = self.end - self.start;
        self.start = self.end;
        (count, self.end)
    }
}

impl<T> Drop for Inserter<'_, T> {
    #[inline]
    fn drop(&mut self) {
        if self.start != self.end {
            unsafe {
                ptr::drop_in_place(
                    &mut self.buf[self.start..self.end] as *mut [MaybeUninit<T>] as *mut [T],
                )
            };
        }
        if self.tail > 0 {
            let range = self.buf.as_mut_ptr_range();
            unsafe {
                ptr::copy(range.end, range.start, self.tail);
            }
        }
    }
}
