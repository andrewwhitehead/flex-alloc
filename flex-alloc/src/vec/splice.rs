use core::iter::{FusedIterator, Peekable};
use core::marker::PhantomData;
use core::ops::Range;
use core::ptr;

use super::buffer::VecBuffer;
use super::drain::Drain;
use super::index_panic;
use crate::capacity::{Grow, Index};

/// A struct used to manage an active `splice` operation for a `Vec` instance
pub struct Splice<'s, I, B, G>
where
    I: Iterator,
    B: VecBuffer<Item = I::Item>,
    G: Grow,
{
    drain: Drain<'s, B>,
    extend: Peekable<I>,
    _pd: PhantomData<G>,
}

impl<'s, I, B, G> Splice<'s, I, B, G>
where
    I: Iterator,
    B: VecBuffer<Item = I::Item>,
    G: Grow,
{
    pub(super) fn new(vec: &'s mut B, extend: I, range: Range<usize>) -> Self {
        let drain = Drain::new(vec, range);
        Self {
            drain,
            extend: extend.peekable(),
            _pd: PhantomData,
        }
    }

    /// Check if the segment to be removed has remaining items
    pub const fn is_empty(&self) -> bool {
        self.drain.is_empty()
    }

    /// Get the number of remaining items in the segment to be removed
    pub const fn len(&self) -> usize {
        self.drain.len()
    }
}

impl<'s, I, B, G> Iterator for Splice<'s, I, B, G>
where
    I: Iterator,
    B: VecBuffer<Item = I::Item>,
    G: Grow,
{
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        self.drain.next()
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
        self.drain.size_hint()
    }
}

impl<'s, I, B, G> DoubleEndedIterator for Splice<'s, I, B, G>
where
    I: Iterator,
    B: VecBuffer<Item = I::Item>,
    G: Grow,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        self.drain.next_back()
    }
}

impl<'s, I, B, G> ExactSizeIterator for Splice<'s, I, B, G>
where
    I: Iterator,
    B: VecBuffer<Item = I::Item>,
    G: Grow,
{
}

impl<'s, I, B, G> FusedIterator for Splice<'s, I, B, G>
where
    I: Iterator,
    B: VecBuffer<Item = I::Item>,
    G: Grow,
{
}

impl<'s, I, B, G> Drop for Splice<'s, I, B, G>
where
    I: Iterator,
    B: VecBuffer<Item = I::Item>,
    G: Grow,
{
    fn drop(&mut self) {
        self.drain.clear_remain();

        while self.extend.peek().is_some() {
            for index in self.drain.range.clone() {
                if let Some(item) = self.extend.next() {
                    unsafe { self.drain.buf.uninit_index(index) }.write(item);
                    self.drain.range.start = index + 1;
                } else {
                    // iterator exhausted, Drain will move tail if necessary and set length
                    return;
                }
            }

            let mut buf_cap = self.drain.buf.capacity();
            let (min_remain, max_remain) = self.extend.size_hint();
            let cap_remain = buf_cap.to_usize() - self.drain.range.end - self.drain.tail_length;
            if min_remain > cap_remain {
                let new_cap =
                    B::Index::try_from_usize(buf_cap.to_usize() + min_remain - cap_remain)
                        .expect("exceeded range of capacity");
                let new_cap = G::next_capacity::<B::Item, _>(buf_cap, new_cap);
                match self.drain.buf.grow_buffer(new_cap, false) {
                    Ok(_) => (),
                    Err(err) => err.panic(),
                }
                buf_cap = new_cap;
            }

            // FIXME some values of size_hint could lead to more tail shifts than necessary,
            // unless we proactively move the tail further?
            if self.drain.tail_length > 0 {
                let new_tail = self
                    .drain
                    .range
                    .end
                    .saturating_add(max_remain.unwrap_or_default().max(min_remain))
                    .min(buf_cap.to_usize());
                let ins_count = new_tail - self.drain.range.end;
                if ins_count < min_remain {
                    index_panic();
                }
                unsafe {
                    let head = self.drain.buf.data_ptr_mut().add(self.drain.range.end);
                    ptr::copy(head, head.add(ins_count), self.drain.tail_length);
                }
                self.drain.range.end += ins_count;
            }
        }
    }
}
