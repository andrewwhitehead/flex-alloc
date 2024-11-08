use core::mem::MaybeUninit;
use core::ptr;

pub struct Inserter<'a, T> {
    buf: &'a mut [MaybeUninit<T>],
    pos: usize,
    cap: usize,
}

impl<'a, T> Inserter<'a, T> {
    #[inline]
    pub fn new(data: &'a mut [MaybeUninit<T>]) -> Self {
        Self::new_with_tail(data, 0)
    }

    #[inline]
    pub fn new_with_tail(data: &'a mut [MaybeUninit<T>], tail_count: usize) -> Self {
        let cap = data.len() - tail_count;
        Self {
            buf: data,
            pos: 0,
            cap,
        }
    }

    #[inline]
    pub fn push(&mut self, val: T) {
        assert!(self.pos < self.cap);
        self.buf[self.pos].write(val);
        self.pos += 1;
    }

    #[inline]
    pub unsafe fn push_unchecked(&mut self, val: T) {
        self.buf[self.pos].write(val);
        self.pos += 1;
    }

    #[inline]
    pub fn push_iter(&mut self, iter: &mut impl Iterator<Item = T>) {
        while self.pos < self.cap {
            if let Some(item) = iter.next() {
                self.buf[self.pos].write(item);
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    #[inline]
    pub fn push_repeat(&mut self, val: &T, len: usize)
    where
        T: Clone,
    {
        assert!(self.cap - self.pos >= len);
        for _ in 0..len {
            self.buf[self.pos].write(val.clone());
            self.pos += 1;
        }
    }

    #[inline]
    pub fn push_slice(&mut self, data: &[T])
    where
        T: Clone,
    {
        assert!(self.cap - self.pos >= data.len());
        for item in data {
            self.buf[self.pos].write(item.clone());
            self.pos += 1;
        }
    }

    // Successfully complete the insertion. Returns the number of
    // inserted entries plus the number of tail entries (equal to
    // the number of initialized slots in the buffer).
    #[inline]
    pub fn complete(mut self) -> usize {
        let count = self.pos;
        let tail_count = self.buf.len() - self.cap;
        if count < self.cap && tail_count > 0 {
            // shift tail entries
            let range = self.buf[count..self.cap].as_mut_ptr_range();
            unsafe {
                ptr::copy(range.end, range.start, tail_count);
            }
        }
        self.buf = &mut [];
        count + tail_count
    }
}

impl<T> Drop for Inserter<'_, T> {
    #[inline]
    fn drop(&mut self) {
        if !self.buf.is_empty() {
            // Drop the inserted items
            unsafe {
                ptr::drop_in_place(&mut self.buf[..self.pos] as *mut [MaybeUninit<T>] as *mut [T])
            };
            if self.cap < self.buf.len() {
                // Drop the tail items
                unsafe {
                    ptr::drop_in_place(
                        &mut self.buf[self.cap..] as *mut [MaybeUninit<T>] as *mut [T],
                    )
                };
            }
        }
    }
}
