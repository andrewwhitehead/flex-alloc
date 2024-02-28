use core::borrow::{Borrow, BorrowMut};
use core::cmp::Ordering;
use core::mem::{self, ManuallyDrop, MaybeUninit};
use core::ops::{Bound, Deref, DerefMut, Range, RangeBounds};
use core::ptr::{self, NonNull};
use core::slice;

use crate::boxed::Box;
use crate::error::{InsertionError, StorageError};
use crate::index::{Grow, Index};
use crate::storage::{Global, RawBuffer};

use self::buffer::VecBuffer;
use self::config::{
    VecConfig, VecConfigAlloc, VecConfigAllocParts, VecConfigNew, VecConfigSpawn, VecNewIn,
};
use self::insert::Inserter;

pub use self::{drain::Drain, into_iter::IntoIter, splice::Splice};

pub mod buffer;
pub mod config;

mod drain;
pub(crate) mod insert;
mod into_iter;
mod splice;

#[cold]
#[inline(never)]
pub(super) fn index_panic() -> ! {
    panic!("Invalid element index");
}

#[inline]
fn bounds<I: Index>(range: impl RangeBounds<I>, length: I) -> Range<usize> {
    let start = match range.start_bound() {
        Bound::Unbounded => 0,
        Bound::Included(i) => i.to_usize(),
        Bound::Excluded(i) => i.to_usize() + 1,
    };
    let end = match range.end_bound() {
        Bound::Unbounded => length.to_usize(),
        Bound::Included(i) => i.to_usize() + 1,
        Bound::Excluded(i) => i.to_usize(),
    };
    Range { start, end }
}

#[derive(Debug)]
#[repr(transparent)]
pub struct Vec<T, C: VecConfig = Global> {
    buffer: C::Buffer<T>,
}

impl<T, C: VecConfigNew<T>> Vec<T, C> {
    pub const fn new() -> Self {
        Self { buffer: C::NEW }
    }

    pub fn try_with_capacity(capacity: C::Index) -> Result<Self, StorageError> {
        let buffer = C::vec_try_new(capacity, false)?;
        Ok(Self { buffer })
    }

    pub fn with_capacity(capacity: C::Index) -> Self {
        match Self::try_with_capacity(capacity) {
            Ok(res) => res,
            Err(error) => error.panic(),
        }
    }

    pub fn from_slice(data: &[T]) -> Self
    where
        T: Clone,
    {
        let Some(len) = C::Index::try_from_usize(data.len()) else {
            index_panic();
        };
        let mut vec = Self::with_capacity(len);
        vec.extend_from_slice(data);
        vec
    }

    pub fn try_from_slice(data: &[T]) -> Result<Self, StorageError>
    where
        T: Clone,
    {
        let Some(len) = C::Index::try_from_usize(data.len()) else {
            return Err(StorageError::CapacityLimit);
        };
        let mut vec = Self::try_with_capacity(len)?;
        vec.extend_from_slice(data);
        Ok(vec)
    }
}

impl<T, C: VecConfig> Vec<T, C> {
    pub fn new_in<A>(alloc_in: A) -> Self
    where
        A: VecNewIn<T, Config = C>,
    {
        match A::vec_try_new_in(alloc_in, C::Index::ZERO, false) {
            Ok(buffer) => Self { buffer },
            Err(err) => err.panic(),
        }
    }

    pub fn try_new_in<A>(alloc_in: A) -> Result<Self, StorageError>
    where
        A: VecNewIn<T, Config = C>,
    {
        Ok(Self {
            buffer: A::vec_try_new_in(alloc_in, C::Index::ZERO, false)?,
        })
    }

    pub fn with_capacity_in<A>(capacity: C::Index, alloc_in: A) -> Self
    where
        A: VecNewIn<T, Config = C>,
    {
        match Self::try_with_capacity_in(capacity, alloc_in) {
            Ok(res) => res,
            Err(error) => error.panic(),
        }
    }

    pub fn try_with_capacity_in<A>(capacity: C::Index, alloc_in: A) -> Result<Self, StorageError>
    where
        A: VecNewIn<T, Config = C>,
    {
        Ok(Self {
            buffer: A::vec_try_new_in(alloc_in, capacity, false)?,
        })
    }
}

impl<T, C: VecConfig> Vec<T, C> {
    #[inline]
    fn into_inner(self) -> C::Buffer<T> {
        let me = ManuallyDrop::new(self);
        unsafe { ptr::read(&me.buffer) }
    }
}

impl<T, C: VecConfigAlloc<T>> Vec<T, C> {
    pub fn allocator(&self) -> &C::Alloc {
        C::allocator(&self.buffer)
    }
}

impl<T, C: VecConfigAllocParts<T>> Vec<T, C> {
    pub fn into_boxed_slice(mut self) -> Box<[T], C::Alloc> {
        self.shrink_to_fit();
        let (data, length, capacity, alloc) = self.into_parts();
        assert_eq!(capacity, length);
        unsafe { Box::slice_from_parts(data, alloc, length.to_usize()) }
    }

    pub fn try_into_boxed_slice(mut self) -> Result<Box<[T], C::Alloc>, StorageError> {
        self.try_shrink_to_fit()?;
        let (data, length, capacity, alloc) = self.into_parts();
        assert_eq!(capacity, length);
        Ok(unsafe { Box::slice_from_parts(data, alloc, length.to_usize()) })
    }

    #[inline]
    pub(crate) fn into_parts(self) -> (NonNull<T>, C::Index, C::Index, C::Alloc) {
        C::vec_into_parts(self.into_inner())
    }

    #[inline]
    pub(crate) unsafe fn from_parts(
        data: NonNull<T>,
        length: C::Index,
        capacity: C::Index,
        alloc: C::Alloc,
    ) -> Self {
        Self {
            buffer: C::vec_from_parts(data, length, capacity, alloc),
        }
    }
}

impl<T, C: VecConfig> Vec<T, C> {
    #[inline]
    pub fn as_ptr(&mut self) -> *const T {
        self.buffer.data_ptr()
    }

    #[inline]
    pub fn as_mut_ptr(&mut self) -> *mut T {
        self.buffer.data_ptr_mut()
    }

    #[inline]
    pub fn as_slice(&self) -> &[T] {
        self.buffer.as_slice()
    }

    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        self.buffer.as_mut_slice()
    }

    #[inline]
    pub fn capacity(&self) -> C::Index {
        self.buffer.capacity()
    }

    #[inline]
    pub fn clear(&mut self) {
        self.truncate(C::Index::ZERO);
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == C::Index::ZERO
    }

    #[inline]
    pub fn len(&self) -> C::Index {
        self.buffer.length()
    }

    #[inline]
    pub unsafe fn set_len(&mut self, length: C::Index) {
        self.buffer.set_length(length)
    }

    #[inline]
    pub fn reserve(&mut self, reserve: C::Index) {
        match self.try_reserve(reserve) {
            Ok(_) => (),
            Err(error) => error.panic(),
        }
    }

    #[inline]
    pub fn try_reserve(&mut self, reserve: C::Index) -> Result<(), StorageError> {
        self._try_reserve(reserve.into(), false)
    }

    fn _try_reserve(&mut self, reserve: usize, exact: bool) -> Result<(), StorageError> {
        let buf_cap: usize = self.buffer.capacity().to_usize();
        let Some(buf_needed) = self.buffer.length().to_usize().checked_add(reserve) else {
            return Err(StorageError::CapacityLimit);
        };
        if buf_cap >= buf_needed {
            return Ok(());
        }
        let Some(mut capacity) = C::Index::try_from_usize(buf_needed) else {
            return Err(StorageError::CapacityLimit);
        };
        if !exact {
            capacity = C::Grow::next_capacity::<T, _>(self.buffer.capacity(), capacity);
        }
        self.buffer.vec_try_resize(capacity, false)?;
        Ok(())
    }

    #[inline]
    pub fn reserve_exact(&mut self, reserve: usize) {
        match self.try_reserve_exact(reserve) {
            Ok(_) => (),
            Err(error) => error.panic(),
        }
    }

    #[inline]
    pub fn try_reserve_exact(&mut self, reserve: usize) -> Result<(), StorageError> {
        self._try_reserve(reserve.into(), true)
    }

    pub fn append(&mut self, other: &mut Self) {
        if self.is_empty() {
            mem::swap(&mut self.buffer, &mut other.buffer);
        } else if !other.is_empty() {
            let cur_len = self.buffer.length().to_usize();
            let cp_len = other.len();
            self.reserve(cp_len);
            unsafe {
                ptr::copy_nonoverlapping(
                    other.buffer.data_ptr(),
                    self.buffer.data_ptr_mut().add(cur_len),
                    cp_len.to_usize(),
                );
            }
            // SAFETY: capacity of both buffers has been established as > 0
            unsafe { other.buffer.set_length(C::Index::ZERO) };
            unsafe {
                self.buffer
                    .set_length(C::Index::from_usize(cur_len + cp_len.to_usize()))
            };
        }
    }

    #[inline]
    pub fn dedup(&mut self)
    where
        T: Eq,
    {
        self.dedup_by(|a, b| a == b)
    }

    pub fn dedup_by<F>(&mut self, mut cmp: F)
    where
        F: FnMut(&mut T, &mut T) -> bool,
    {
        let orig_len = self.buffer.length().to_usize();
        if orig_len < 2 {
            return;
        }
        let mut new_len = 1;
        let mut head = self.as_mut_ptr();
        let mut tail = unsafe { head.add(1) };
        // FIXME on panic, move tail to head
        for _ in 1..orig_len {
            if !cmp(unsafe { &mut *tail }, unsafe { &mut *head }) {
                head = unsafe { head.add(1) };
                if head != tail {
                    unsafe { ptr::copy_nonoverlapping(tail, head, 1) };
                }
                new_len += 1;
            } else {
                unsafe {
                    ptr::drop_in_place(tail);
                }
            }
            tail = unsafe { tail.add(1) };
        }
        // SAFETY: capacity of the buffer has been established as > 0
        unsafe { self.buffer.set_length(C::Index::from_usize(new_len)) }
    }

    #[inline]
    pub fn dedup_by_key<F, K>(&mut self, mut key_f: F)
    where
        F: FnMut(&mut T) -> K,
        K: PartialEq,
    {
        self.dedup_by(|a, b| key_f(a) == key_f(b))
    }

    #[inline]
    pub fn drain<R>(&mut self, range: R) -> Drain<'_, C::Buffer<T>>
    where
        R: RangeBounds<C::Index>,
    {
        let range = bounds(range, self.buffer.length());
        Drain::new(&mut self.buffer, range)
    }

    pub fn extend_from_slice(&mut self, items: &[T])
    where
        T: Clone,
    {
        match self._try_reserve(items.len(), false) {
            Ok(_) => (),
            Err(error) => error.panic(),
        }
        unsafe {
            self.extend_unchecked(items);
        }
    }

    pub fn try_extend_from_slice(&mut self, items: &[T]) -> Result<(), StorageError>
    where
        T: Clone,
    {
        self._try_reserve(items.len(), false)?;
        unsafe {
            self.extend_unchecked(items);
        }
        Ok(())
    }

    unsafe fn extend_unchecked(&mut self, items: &[T])
    where
        T: Clone,
    {
        let mut insert = Inserter::for_buffer(&mut self.buffer);
        for item in items.iter() {
            insert.push_clone(item);
        }
        let (added, new_len) = insert.complete();
        if added > 0 {
            unsafe { self.buffer.set_length(C::Index::from_usize(new_len)) };
        }
    }

    fn try_extend(&mut self, iter: &mut impl Iterator<Item = T>) -> Result<(), InsertionError<T>> {
        loop {
            let mut insert = Inserter::for_buffer(&mut self.buffer);
            let mut full;
            loop {
                full = insert.full();
                if full {
                    break;
                }
                let Some(item) = iter.next() else { break };
                insert.push(item);
            }
            let (added, new_len) = insert.complete();
            if added > 0 {
                unsafe { self.buffer.set_length(C::Index::from_usize(new_len)) };
            }
            if !full {
                // ran out of items to insert
                break;
            }
            if let Some(item) = iter.next() {
                let min_reserve = iter.size_hint().0.saturating_add(1);
                match self._try_reserve(min_reserve, false) {
                    Ok(_) => {
                        unsafe { self.buffer.uninit_index(new_len) }.write(item);
                        unsafe { self.buffer.set_length(C::Index::from_usize(new_len + 1)) };
                    }
                    Err(err) => return Err(InsertionError::new(err, item)),
                }
            } else {
                break;
            }
        }
        Ok(())
    }

    pub fn insert(&mut self, index: C::Index, value: T) {
        match self.try_insert(index, value) {
            Ok(_) => (),
            Err(error) => error.panic(),
        }
    }

    pub fn try_insert(&mut self, index: C::Index, value: T) -> Result<(), InsertionError<T>> {
        let prev_len = self.buffer.length();
        if index > prev_len {
            index_panic();
        }
        let index = index.to_usize();
        let tail_count = prev_len.to_usize() - index;
        match self._try_reserve(1, false) {
            Ok(_) => (),
            Err(error) => return Err(InsertionError::new(error, value)),
        };
        unsafe {
            let head = self.buffer.data_ptr_mut().add(index);
            if tail_count > 0 {
                ptr::copy(head, head.add(1), tail_count);
            }
            head.write(value);
        }
        // SAFETY: capacity of the buffer has been established as > 0 by try_reserve
        unsafe { self.buffer.set_length(prev_len.saturating_add(1)) };
        Ok(())
    }

    pub fn insert_slice(&mut self, index: C::Index, values: &[T])
    where
        T: Clone,
    {
        match self.try_insert_slice(index, values) {
            Ok(_) => (),
            Err(error) => error.panic(),
        }
    }

    pub fn try_insert_slice(&mut self, index: C::Index, values: &[T]) -> Result<(), StorageError>
    where
        T: Clone,
    {
        let prev_len = self.buffer.length().to_usize();
        let index = index.to_usize();
        if index > prev_len {
            index_panic();
        }
        let ins_count = values.len();
        if ins_count == 0 {
            return Ok(());
        }
        self._try_reserve(ins_count, false)?;
        let tail_count = prev_len - index;
        let head = unsafe { self.buffer.data_ptr_mut().add(index) };
        if tail_count > 0 {
            unsafe { ptr::copy(head, head.add(index + ins_count), tail_count) };
        }
        let mut insert =
            Inserter::for_buffer_with_range(&mut self.buffer, index, ins_count, tail_count);
        for item in values {
            insert.push_clone(item);
        }
        insert.complete();
        // SAFETY: capacity of the buffer has been established as > 0 by try_reserve
        unsafe {
            self.buffer
                .set_length(C::Index::from_usize(prev_len + ins_count));
        }
        Ok(())
    }

    pub fn pop(&mut self) -> Option<T> {
        let mut tail = self.buffer.length().to_usize();
        if tail > 0 {
            tail -= 1;
            unsafe { self.buffer.set_length(C::Index::from_usize(tail)) };
            Some(unsafe { self.buffer.uninit_index(tail).assume_init_read() })
        } else {
            None
        }
    }

    pub fn push(&mut self, item: T) {
        match self._try_reserve(1, false) {
            Ok(_) => (),
            Err(error) => error.panic(),
        }
        unsafe {
            self.push_unchecked(item);
        }
    }

    pub fn try_push(&mut self, item: T) -> Result<(), InsertionError<T>> {
        if let Err(error) = self._try_reserve(1, false) {
            return Err(InsertionError::new(error, item));
        }
        unsafe {
            self.push_unchecked(item);
        }
        Ok(())
    }

    #[inline]
    pub unsafe fn push_unchecked(&mut self, item: T) {
        let length = self.buffer.length().to_usize();
        self.buffer.uninit_index(length).write(item);
        self.buffer.set_length(C::Index::from_usize(length + 1));
    }

    pub fn remove(&mut self, index: C::Index) -> T {
        let len = self.buffer.length().to_usize();
        let index = index.to_usize();
        if index >= len {
            index_panic();
        }
        let copy_count = len - index - 1;
        unsafe {
            let result = self.buffer.uninit_index(index).assume_init_read();
            if copy_count > 0 {
                let head = self.as_mut_ptr().add(index);
                ptr::copy(head.add(1), head, copy_count);
            }
            self.buffer.set_length(C::Index::from_usize(len - 1));
            result
        }
    }

    #[inline]
    pub fn resize(&mut self, new_len: C::Index, value: T)
    where
        T: Clone,
    {
        match self.try_resize(new_len, value) {
            Ok(_) => (),
            Err(err) => err.panic(),
        }
    }

    pub fn try_resize(&mut self, new_len: C::Index, value: T) -> Result<(), StorageError>
    where
        T: Clone,
    {
        let len = self.buffer.length();
        match new_len.cmp(&len) {
            Ordering::Greater => {
                let ins_count = new_len.to_usize() - len.to_usize();
                self._try_reserve(ins_count, false)?;
                let mut insert = Inserter::for_buffer(&mut self.buffer);
                for _ in 0..ins_count {
                    insert.push_clone(&value);
                }
                insert.complete();
                // SAFETY: capacity of the buffer has been established as > 0 by _try_reserve
                unsafe { self.buffer.set_length(new_len) }
            }
            Ordering::Less => {
                self.truncate(new_len);
            }
            Ordering::Equal => {}
        }
        Ok(())
    }

    #[inline]
    pub fn resize_with<F>(&mut self, new_len: C::Index, f: F)
    where
        F: FnMut() -> T,
    {
        match self.try_resize_with(new_len, f) {
            Ok(_) => (),
            Err(err) => err.panic(),
        }
    }

    pub fn try_resize_with<F>(&mut self, new_len: C::Index, mut f: F) -> Result<(), StorageError>
    where
        F: FnMut() -> T,
    {
        let len = self.buffer.length();
        match new_len.cmp(&len) {
            Ordering::Greater => {
                let ins_count = new_len.to_usize() - len.to_usize();
                self._try_reserve(ins_count, false)?;
                let mut insert = Inserter::for_buffer(&mut self.buffer);
                for _ in 0..ins_count {
                    insert.push(f());
                }
                insert.complete();
                // SAFETY: capacity of the buffer has been established as > 0 by _try_reserve
                unsafe { self.buffer.set_length(new_len) }
            }
            Ordering::Less => {
                self.truncate(new_len);
            }
            Ordering::Equal => {}
        }
        Ok(())
    }

    #[inline]
    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(&T) -> bool,
    {
        self.retain_mut(|r| f(r))
    }

    pub fn retain_mut<F>(&mut self, mut f: F)
    where
        F: FnMut(&mut T) -> bool,
    {
        let orig_len = self.buffer.length().to_usize();
        if orig_len == 0 {
            return;
        }
        let mut tail = self.as_mut_ptr();
        // SAFETY: capacity of the buffer has been established as > 0
        unsafe { self.buffer.set_length(C::Index::ZERO) };
        // FIXME drop remainder on panic
        let mut len = 0;
        for idx in 0..orig_len {
            unsafe {
                let read = self.as_mut_ptr().add(idx);
                if f(&mut *read) {
                    if tail != read {
                        ptr::copy_nonoverlapping(read, tail, 1);
                    }
                    tail = tail.add(1);
                    len += 1;
                } else {
                    ptr::drop_in_place(read);
                }
            }
        }
        unsafe { self.buffer.set_length(C::Index::from_usize(len)) };
    }

    #[inline]
    pub fn shrink_to(&mut self, min_capacity: C::Index) {
        match self.try_shrink_to(min_capacity) {
            Ok(_) => (),
            Err(err) => err.panic(),
        }
    }

    pub fn try_shrink_to(&mut self, min_capacity: C::Index) -> Result<(), StorageError> {
        let len = self.buffer.length().max(min_capacity);
        if self.buffer.capacity() != len {
            self.buffer.vec_try_resize(len, true)?;
        }
        Ok(())
    }

    #[inline]
    pub fn shrink_to_fit(&mut self) {
        match self.try_shrink_to_fit() {
            Ok(_) => (),
            Err(err) => err.panic(),
        }
    }

    #[inline]
    pub fn try_shrink_to_fit(&mut self) -> Result<(), StorageError> {
        self.try_shrink_to(self.buffer.length())
    }

    #[inline]
    pub fn spare_capacity_mut(&mut self) -> &mut [MaybeUninit<T>] {
        let length = self.len().into();
        &mut self.buffer.as_uninit_slice()[length..]
    }

    pub fn split_at_spare_mut(&mut self) -> (&mut [T], &mut [MaybeUninit<T>]) {
        let length = self.len().into();
        let (data, spare) = self.buffer.as_uninit_slice().split_at_mut(length);
        (
            unsafe { slice::from_raw_parts_mut(data.as_mut_ptr().cast(), length) },
            spare,
        )
    }

    pub fn split_off(&mut self, index: C::Index) -> Self
    where
        C: VecConfigSpawn<T>,
    {
        let len = self.buffer.length().to_usize();
        let index_usize = index.to_usize();
        if index_usize >= len {
            index_panic();
        }
        let move_len = C::Index::from_usize(len - index_usize);
        match C::vec_try_spawn(&self.buffer, move_len, false) {
            Ok(mut buffer) => {
                if index_usize == 0 {
                    mem::swap(&mut buffer, &mut self.buffer);
                } else {
                    unsafe {
                        ptr::copy_nonoverlapping(
                            self.buffer.data_ptr().add(index_usize),
                            buffer.data_ptr_mut(),
                            move_len.to_usize(),
                        );
                    }
                    // SAFETY: both buffer capacities are established as > 0
                    unsafe { buffer.set_length(move_len) };
                    unsafe { self.buffer.set_length(index) };
                }
                Self { buffer }
            }
            Err(err) => err.panic(),
        }
    }

    pub fn splice<R, I>(
        &mut self,
        range: R,
        replace_with: I,
    ) -> Splice<'_, <I as IntoIterator>::IntoIter, C::Buffer<T>, C::Grow>
    where
        R: RangeBounds<C::Index>,
        I: IntoIterator<Item = T>,
    {
        let range = bounds(range, self.buffer.length());
        Splice::new(&mut self.buffer, replace_with.into_iter(), range)
    }

    pub fn swap_remove(&mut self, index: C::Index) -> T {
        let index: usize = index.to_usize();
        let length: usize = self.buffer.length().to_usize();
        if index >= length {
            index_panic();
        }
        let last: usize = length - 1;
        // SAFETY: buffer capacity is established as > 0
        unsafe { self.buffer.set_length(C::Index::from_usize(last)) };
        let data = self.buffer.as_uninit_slice();
        let result = unsafe { data[index].assume_init_read() };
        if index != last {
            unsafe { data[index].write(data[last].assume_init_read()) };
        }
        result
    }

    pub fn truncate(&mut self, length: C::Index) {
        let old_len: usize = self.len().to_usize();
        let new_len = length.to_usize().min(old_len);
        let remove = old_len - new_len;
        if remove > 0 {
            // SAFETY: buffer capacity is established as > 0
            unsafe { self.buffer.set_length(C::Index::from_usize(new_len)) };
            unsafe {
                let to_drop: &mut [T] =
                    slice::from_raw_parts_mut(self.buffer.data_ptr_mut().add(new_len), remove);
                ptr::drop_in_place(to_drop);
            }
        }
    }
}

impl<T, C: VecConfig> AsRef<[T]> for Vec<T, C> {
    #[inline]
    fn as_ref(&self) -> &[T] {
        &**self
    }
}

impl<T, C: VecConfig> AsMut<[T]> for Vec<T, C> {
    #[inline]
    fn as_mut(&mut self) -> &mut [T] {
        &mut **self
    }
}

impl<T, C: VecConfig> Borrow<[T]> for Vec<T, C> {
    #[inline]
    fn borrow(&self) -> &[T] {
        &**self
    }
}

impl<T, C: VecConfig> BorrowMut<[T]> for Vec<T, C> {
    #[inline]
    fn borrow_mut(&mut self) -> &mut [T] {
        &mut **self
    }
}

impl<T: Clone, C: VecConfigSpawn<T>> Clone for Vec<T, C> {
    fn clone(&self) -> Self {
        let mut inst = Self {
            buffer: match C::vec_try_spawn(&self.buffer, self.buffer.length(), false) {
                Ok(buf) => buf,
                Err(err) => err.panic(),
            },
        };
        inst.extend_from_slice(self);
        inst
    }

    fn clone_from(&mut self, source: &Self) {
        self.truncate(C::Index::ZERO);
        self.extend_from_slice(source);
    }
}

impl<T, C: VecConfig> Default for Vec<T, C>
where
    C::Buffer<T>: Default,
{
    #[inline]
    fn default() -> Self {
        Self {
            buffer: C::Buffer::default(),
        }
    }
}

impl<T, C: VecConfig> Deref for Vec<T, C> {
    type Target = [T];

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe { slice::from_raw_parts(self.buffer.data_ptr(), self.len().into()) }
    }
}

impl<T, C: VecConfig> DerefMut for Vec<T, C> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { slice::from_raw_parts_mut(self.buffer.data_ptr_mut(), self.len().into()) }
    }
}

impl<T, C: VecConfig> Drop for Vec<T, C> {
    fn drop(&mut self) {
        let to_drop: &mut [T] = self.as_mut_slice();
        if !to_drop.is_empty() {
            unsafe {
                ptr::drop_in_place(to_drop);
            }
            // SAFETY: buffer capacity is established as > 0
            unsafe { self.buffer.set_length(C::Index::ZERO) };
        }
    }
}

impl<T, C: VecConfig> Extend<T> for Vec<T, C> {
    #[inline]
    fn extend<A: IntoIterator<Item = T>>(&mut self, iter: A) {
        match self.try_extend(&mut iter.into_iter()) {
            Ok(_) => (),
            Err(error) => error.panic(),
        }
    }
}

impl<'a, T: Clone + 'a, C: VecConfig> Extend<&'a T> for Vec<T, C> {
    #[inline]
    fn extend<A: IntoIterator<Item = &'a T>>(&mut self, iter: A) {
        match self.try_extend(&mut iter.into_iter().cloned()) {
            Ok(_) => (),
            Err(error) => error.panic(),
        }
    }
}

impl<T, C: VecConfigNew<T>> FromIterator<T> for Vec<T, C> {
    #[inline]
    fn from_iter<A: IntoIterator<Item = T>>(iter: A) -> Self {
        let iter = iter.into_iter();
        let (min_cap, _) = iter.size_hint();
        let Some(min_cap) = C::Index::try_from_usize(min_cap) else {
            index_panic();
        };
        let mut vec = Self::with_capacity(min_cap);
        vec.extend(iter);
        vec
    }
}

// This is intentionally simpler than the inferred bounds, C::VecBuffer<T>: Send.
// If a particular VecBuffer is not 'Send' then the VecConfig type must reflect that.
unsafe impl<T, C: VecConfig> Send for Vec<T, C>
where
    T: Send,
    C: Send,
{
}

// If a particular VecBuffer is not 'Sync' then the VecConfig type must reflect that.
unsafe impl<T, C: VecConfig> Sync for Vec<T, C>
where
    T: Sync,
    C: Sync,
{
}

impl<T, C: VecConfigAllocParts<T>> From<Box<[T], C::Alloc>> for Vec<T, C> {
    fn from(boxed: Box<[T], C::Alloc>) -> Self {
        let (ptr, alloc) = Box::into_parts(boxed);
        let Some(length) = C::Index::try_from_usize(ptr.len()) else {
            index_panic();
        };
        unsafe { Self::from_parts(ptr.cast(), length, length, alloc) }
    }
}

#[cfg(feature = "alloc")]
impl<T, C> From<alloc::boxed::Box<[T]>> for Vec<T, C>
where
    C: VecConfigAllocParts<T, Alloc = Global, Index = usize>,
{
    fn from(vec: alloc::boxed::Box<[T]>) -> Self {
        alloc::vec::Vec::<T>::from(vec).into()
    }
}

#[cfg(feature = "alloc")]
impl<T, C> From<Vec<T, C>> for alloc::boxed::Box<[T]>
where
    C: VecConfigAllocParts<T, Alloc = Global, Index = usize>,
{
    fn from(vec: Vec<T, C>) -> Self {
        alloc::vec::Vec::<T>::from(vec).into_boxed_slice()
    }
}

#[cfg(feature = "alloc")]
impl<T, C> From<alloc::vec::Vec<T>> for Vec<T, C>
where
    C: VecConfigAllocParts<T, Alloc = Global, Index = usize>,
{
    fn from(vec: alloc::vec::Vec<T>) -> Self {
        let capacity = vec.capacity();
        let length = vec.len();
        let data = unsafe { ptr::NonNull::new_unchecked(ManuallyDrop::new(vec).as_mut_ptr()) };
        unsafe { Self::from_parts(data, length, capacity, Global) }
    }
}

#[cfg(feature = "alloc")]
impl<T, C> From<Vec<T, C>> for alloc::vec::Vec<T>
where
    C: VecConfigAllocParts<T, Alloc = Global, Index = usize>,
{
    fn from(vec: Vec<T, C>) -> Self {
        let mut buffer = ManuallyDrop::new(vec.into_inner());
        let capacity = buffer.capacity();
        let length = buffer.length();
        let data = buffer.data_ptr_mut();
        unsafe { alloc::vec::Vec::from_raw_parts(data, length, capacity) }
    }
}

// #[cfg(feature = "alloc")]
// impl<'b, T: Clone, S> From<alloc::borrow::Cow<'b, [T]>> for Vec<T, S>
// where
//     S: StorageNew + StorageWithAlloc,
// {
//     fn from(cow: alloc::borrow::Cow<'b, [T]>) -> Self {
//         match cow {
//             alloc::borrow::Cow::Borrowed(data) => Vec::<T, S>::from_slice(data),
//             alloc::borrow::Cow::Owned(vec) => vec.into(),
//         }
//     }
// }

// #[cfg(feature = "alloc")]
// impl<'b, T: Clone, S: StorageWithAlloc> From<Vec<T, S>> for alloc::borrow::Cow<'b, [T]> {
//     fn from(vec: Vec<T, S>) -> alloc::borrow::Cow<'b, [T]> {
//         alloc::borrow::Cow::Owned(alloc::vec::Vec::from(vec))
//     }
// }

impl<T: Clone, C: VecConfigNew<T>> From<&[T]> for Vec<T, C> {
    #[inline]
    fn from(data: &[T]) -> Self {
        Self::from_slice(data)
    }
}

impl<T: Clone, C: VecConfigNew<T>> From<&mut [T]> for Vec<T, C> {
    #[inline]
    fn from(data: &mut [T]) -> Self {
        Self::from_slice(data)
    }
}

impl<T: Clone, C: VecConfigNew<T>, const N: usize> From<&[T; N]> for Vec<T, C> {
    #[inline]
    fn from(data: &[T; N]) -> Self {
        Self::from_slice(data)
    }
}

impl<T: Clone, C: VecConfigNew<T>, const N: usize> From<&mut [T; N]> for Vec<T, C> {
    #[inline]
    fn from(data: &mut [T; N]) -> Self {
        Self::from_slice(data)
    }
}

impl<T, C: VecConfigNew<T>, const N: usize> From<[T; N]> for Vec<T, C> {
    #[inline]
    fn from(data: [T; N]) -> Self {
        Self::from_iter(data)
    }
}

impl<T, C: VecConfig> IntoIterator for Vec<T, C> {
    type Item = T;
    type IntoIter = IntoIter<C::Buffer<T>>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        IntoIter::new(self.into_inner())
    }
}

impl<'a, T, C: VecConfig> IntoIterator for &'a Vec<T, C> {
    type Item = &'a T;
    type IntoIter = <&'a [T] as IntoIterator>::IntoIter;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.as_slice().into_iter()
    }
}

impl<'a, T, C: VecConfig> IntoIterator for &'a mut Vec<T, C> {
    type Item = &'a mut T;
    type IntoIter = <&'a mut [T] as IntoIterator>::IntoIter;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.as_mut_slice().into_iter()
    }
}

impl<T1, C1, T2, C2> PartialEq<Vec<T2, C2>> for Vec<T1, C1>
where
    C1: VecConfig,
    C2: VecConfig,
    T1: PartialEq<T2>,
{
    #[inline]
    fn eq(&self, other: &Vec<T2, C2>) -> bool {
        (&**self).eq(&**other)
    }
}

impl<T: Eq, C: VecConfig> Eq for Vec<T, C> {}

impl<T1, C1, T2> PartialEq<&[T2]> for Vec<T1, C1>
where
    T1: PartialEq<T2>,
    C1: VecConfig,
{
    #[inline]
    fn eq(&self, other: &&[T2]) -> bool {
        (&**self).eq(*other)
    }
}

impl<T1, C1, T2> PartialEq<&mut [T2]> for Vec<T1, C1>
where
    T1: PartialEq<T2>,
    C1: VecConfig,
{
    #[inline]
    fn eq(&self, other: &&mut [T2]) -> bool {
        (&**self).eq(*other)
    }
}

impl<T1, C1, T2> PartialEq<[T2]> for Vec<T1, C1>
where
    T1: PartialEq<T2>,
    C1: VecConfig,
{
    #[inline]
    fn eq(&self, other: &[T2]) -> bool {
        (&**self).eq(other)
    }
}

impl<T1, C1, T2, const N: usize> PartialEq<&[T2; N]> for Vec<T1, C1>
where
    T1: PartialEq<T2>,
    C1: VecConfig,
{
    #[inline]
    fn eq(&self, other: &&[T2; N]) -> bool {
        (&**self).eq(&other[..])
    }
}

impl<T1, C1, T2, const N: usize> PartialEq<&mut [T2; N]> for Vec<T1, C1>
where
    T1: PartialEq<T2>,
    C1: VecConfig,
{
    #[inline]
    fn eq(&self, other: &&mut [T2; N]) -> bool {
        (&**self).eq(&other[..])
    }
}

impl<T1, C1, T2, const N: usize> PartialEq<[T2; N]> for Vec<T1, C1>
where
    T1: PartialEq<T2>,
    C1: VecConfig,
{
    #[inline]
    fn eq(&self, other: &[T2; N]) -> bool {
        (&**self).eq(&other[..])
    }
}

impl<T1, T2, C2> PartialEq<Vec<T2, C2>> for &[T1]
where
    T2: PartialEq<T1>,
    C2: VecConfig,
{
    #[inline]
    fn eq(&self, other: &Vec<T2, C2>) -> bool {
        other.eq(self)
    }
}

impl<T1, T2, C2> PartialEq<Vec<T2, C2>> for &mut [T1]
where
    T2: PartialEq<T1>,
    C2: VecConfig,
{
    #[inline]
    fn eq(&self, other: &Vec<T2, C2>) -> bool {
        other.eq(self)
    }
}

impl<T1, T2, C2> PartialEq<Vec<T2, C2>> for [T1]
where
    T2: PartialEq<T1>,
    C2: VecConfig,
{
    #[inline]
    fn eq(&self, other: &Vec<T2, C2>) -> bool {
        other.eq(self)
    }
}

impl<T1, T2, C2, const N: usize> PartialEq<Vec<T2, C2>> for [T1; N]
where
    T2: PartialEq<T1>,
    C2: VecConfig,
{
    #[inline]
    fn eq(&self, other: &Vec<T2, C2>) -> bool {
        other.eq(self)
    }
}

#[cfg(feature = "alloc")]
impl<A, B, C> PartialEq<alloc::vec::Vec<A>> for Vec<B, C>
where
    B: PartialEq<A>,
    C: VecConfig,
{
    #[inline]
    fn eq(&self, other: &alloc::vec::Vec<A>) -> bool {
        other.eq(self)
    }
}

#[cfg(feature = "alloc")]
impl<A, B, C> PartialEq<Vec<B, C>> for alloc::vec::Vec<A>
where
    B: PartialEq<A>,
    C: VecConfig,
{
    #[inline]
    fn eq(&self, other: &Vec<B, C>) -> bool {
        other.eq(self)
    }
}

// FIXME
// From<String>
// From<CString>
// From<&str>
// TryFrom<Vec<T>> for [T; N]
// io::Write
// dedup_by_key
// push_within_capacity
// leak

/// ```compile_fail,E0597
/// use flex_alloc::{byte_storage, vec::Vec};
///
/// fn run<F: FnOnce() -> () + 'static>(f: F) { f() }
///
/// let mut buf = byte_storage::<10>();
/// let mut v = Vec::<usize, _>::new_in(&mut buf);
/// run(move || v.clear());
/// ```
fn _lifetime_check() {}
