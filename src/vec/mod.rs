use core::borrow::{Borrow, BorrowMut};
use core::cmp::Ordering;
use core::fmt;
use core::iter::repeat;
use core::mem::{self, ManuallyDrop, MaybeUninit};
use core::ops::{Bound, Deref, DerefMut, Range, RangeBounds};
use core::ptr::{self, slice_from_raw_parts_mut, NonNull};
use core::slice;

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

#[macro_use]
mod macros;

mod cow;
mod drain;
pub(crate) mod insert;
mod into_iter;
mod splice;

/// A `Vec` which stores its contained data inline, using no external allocation.
pub type InlineVec<T, const N: usize> = Vec<T, crate::storage::Inline<N>>;

#[cfg(feature = "alloc")]
/// A `Vec` which is pointer-sized, storing its capacity and length in the
/// allocated buffer.
pub type ThinVec<T> = Vec<T, crate::storage::Thin>;

#[cfg(feature = "zeroize")]
/// A `Vec` which automatically zeroizes its buffer when dropped.
pub type ZeroizingVec<T> = Vec<T, crate::storage::ZeroizingAlloc<Global>>;

#[cold]
#[inline(never)]
pub(super) fn index_panic() -> ! {
    panic!("Invalid element index");
}

#[inline]
fn bounds_to_range<I: Index>(range: impl RangeBounds<I>, length: I) -> Range<usize> {
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

#[cfg(feature = "alloc")]
#[inline]
/// Create a `Vec<T>` from an array `[T; N]`.
pub fn from_array<T, const N: usize>(data: [T; N]) -> Vec<T> {
    let mut v = Vec::new();
    v.extend(data);
    v
}

#[inline]
/// Create a `Vec<T, C>` from an array `[T; N]` and an instance of `VecNewIn<T>`.
pub fn from_array_in<T, C, const N: usize>(data: [T; N], alloc_in: C) -> Vec<T, C::Config>
where
    C: VecNewIn<T>,
{
    let mut v = Vec::new_in(alloc_in);
    v.extend(data);
    v
}

#[cfg(feature = "alloc")]
#[inline]
/// Create a `Vec<T>` from a cloneable element T and a count of the number of elements.
pub fn from_elem<T: Clone>(elem: T, count: usize) -> Vec<T, Global> {
    Vec::from_iter(repeat(elem).take(count))
}

#[inline]
/// Create a `Vec<T, C>` from a cloneable element T, a count of the number of elements,
/// and an instance of `VecNewIn<T>`.
pub fn from_elem_in<T, C>(elem: T, count: usize, alloc_in: C) -> Vec<T, C::Config>
where
    T: Clone,
    C: VecNewIn<T>,
{
    Vec::from_iter_in(repeat(elem).take(count), alloc_in)
}

#[repr(transparent)]
pub struct Vec<T, C: VecConfig = Global> {
    buffer: C::Buffer<T>,
}

impl<T, C: VecConfigNew<T>> Vec<T, C> {
    /// Constructs a new, empty `Vec<T, C>`.
    ///
    /// The vector will not allocate until elements are pushed onto it.
    ///
    /// # Examples
    ///
    /// ```
    /// # #![allow(unused_mut)]
    /// # #[cfg(feature = "alloc")]
    /// let mut vec: Vec<i32> = Vec::new();
    /// ```
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

    pub fn from_slice_in<A>(data: &[T], alloc_in: A) -> Self
    where
        T: Clone,
        A: VecNewIn<T, Config = C>,
    {
        let Some(len) = C::Index::try_from_usize(data.len()) else {
            index_panic();
        };
        let mut vec = Self::with_capacity_in(len, alloc_in);
        vec.extend_from_slice(data);
        vec
    }

    pub fn try_from_slice_in<A>(data: &[T], alloc_in: A) -> Result<Self, StorageError>
    where
        T: Clone,
        A: VecNewIn<T, Config = C>,
    {
        let Some(len) = C::Index::try_from_usize(data.len()) else {
            return Err(StorageError::CapacityLimit);
        };
        let mut vec = Self::try_with_capacity_in(len, alloc_in)?;
        vec.extend_from_slice(data);
        Ok(vec)
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

#[cfg(feature = "alloc")]
impl<T, C> Vec<T, C>
where
    C: VecConfigAllocParts<T, Alloc = Global, Index = usize>,
{
    pub fn into_boxed_slice(mut self) -> alloc::boxed::Box<[T]> {
        self.shrink_to_fit();
        let (data, length, capacity, _alloc) = self.into_parts();
        assert_eq!(capacity, length);
        let data = slice_from_raw_parts_mut(data.as_ptr(), length);
        unsafe { alloc::boxed::Box::from_raw(data) }
    }

    pub fn try_into_boxed_slice(mut self) -> Result<alloc::boxed::Box<[T]>, StorageError> {
        self.try_shrink_to_fit()?;
        let (data, length, capacity, _alloc) = self.into_parts();
        assert_eq!(capacity, length);
        let data = slice_from_raw_parts_mut(data.as_ptr(), length);
        Ok(unsafe { alloc::boxed::Box::from_raw(data) })
    }
}

impl<T, C: VecConfigAllocParts<T>> Vec<T, C> {
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
    pub fn leak<'a>(self) -> &'a mut [T]
    where
        C: 'a,
    {
        let mut me = ManuallyDrop::new(self);
        unsafe { slice::from_raw_parts_mut(me.as_mut_ptr(), me.len().to_usize()) }
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
        let range = bounds_to_range(range, self.buffer.length());
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

    fn from_iter_in<A, I>(iter: A, alloc_in: I) -> Self
    where
        A: IntoIterator<Item = T>,
        I: VecNewIn<T, Config = C>,
    {
        let iter = iter.into_iter();
        let (min_cap, _) = iter.size_hint();
        let Some(min_cap) = C::Index::try_from_usize(min_cap) else {
            index_panic();
        };
        let mut vec = Self::with_capacity_in(min_cap, alloc_in);
        vec.extend(iter);
        vec
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
        let range = bounds_to_range(range, self.buffer.length());
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
        self.as_slice()
    }
}

impl<T, C: VecConfig> AsMut<[T]> for Vec<T, C> {
    #[inline]
    fn as_mut(&mut self) -> &mut [T] {
        self.as_mut_slice()
    }
}

impl<T, C: VecConfig> Borrow<[T]> for Vec<T, C> {
    #[inline]
    fn borrow(&self) -> &[T] {
        self.as_slice()
    }
}

impl<T, C: VecConfig> BorrowMut<[T]> for Vec<T, C> {
    #[inline]
    fn borrow_mut(&mut self) -> &mut [T] {
        self.as_mut_slice()
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

impl<T: fmt::Debug, C: VecConfig> fmt::Debug for Vec<T, C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_slice().fmt(f)
    }
}

impl<T, C: VecConfigNew<T>> Default for Vec<T, C> {
    #[inline]
    fn default() -> Self {
        Self::new()
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
unsafe impl<T: Send, C: VecConfig + Send> Send for Vec<T, C> {}

// If a particular VecBuffer is not 'Sync' then the VecConfig type must reflect that.
unsafe impl<T: Sync, C: VecConfig + Sync> Sync for Vec<T, C> {}

#[cfg(feature = "alloc")]
impl<T, C> From<alloc::boxed::Box<[T]>> for Vec<T, C>
where
    C: VecConfigAllocParts<T, Alloc = Global, Index = usize>,
{
    #[inline]
    fn from(vec: alloc::boxed::Box<[T]>) -> Self {
        alloc::vec::Vec::<T>::from(vec).into()
    }
}

#[cfg(feature = "allocator-api2")]
impl<T, C, A> From<allocator_api2::boxed::Box<[T], A>> for Vec<T, C>
where
    A: allocator_api2::alloc::Allocator,
    C: VecConfigAllocParts<T, Alloc = A, Index = usize>,
{
    #[inline]
    fn from(vec: allocator_api2::boxed::Box<[T], A>) -> Self {
        allocator_api2::vec::Vec::<T, A>::from(vec).into()
    }
}

#[cfg(feature = "alloc")]
impl<T, C> From<Vec<T, C>> for alloc::boxed::Box<[T]>
where
    C: VecConfigAllocParts<T, Alloc = Global, Index = usize>,
{
    #[inline]
    fn from(vec: Vec<T, C>) -> Self {
        alloc::vec::Vec::<T>::from(vec).into_boxed_slice()
    }
}

#[cfg(feature = "allocator-api2")]
impl<T, C, A> From<Vec<T, C>> for allocator_api2::boxed::Box<[T], A>
where
    A: allocator_api2::alloc::Allocator,
    C: VecConfigAllocParts<T, Alloc = A, Index = usize>,
{
    #[inline]
    fn from(vec: Vec<T, C>) -> Self {
        allocator_api2::vec::Vec::<T, A>::from(vec).into_boxed_slice()
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

#[cfg(feature = "allocator-api2")]
impl<T, C, A> From<allocator_api2::vec::Vec<T, A>> for Vec<T, C>
where
    A: allocator_api2::alloc::Allocator,
    C: VecConfigAllocParts<T, Alloc = A, Index = usize>,
{
    #[inline]
    fn from(vec: allocator_api2::vec::Vec<T, A>) -> Self {
        let (data, length, capacity, alloc) = vec.into_raw_parts_with_alloc();
        unsafe { Self::from_parts(NonNull::new_unchecked(data), length, capacity, alloc) }
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

#[cfg(feature = "allocator-api2")]
impl<T, C, A> From<Vec<T, C>> for allocator_api2::vec::Vec<T, A>
where
    A: allocator_api2::alloc::Allocator,
    C: VecConfigAllocParts<T, Alloc = A, Index = usize>,
{
    fn from(vec: Vec<T, C>) -> Self {
        let (data, length, capacity, alloc) = C::vec_into_parts(vec.into_inner());
        unsafe {
            allocator_api2::vec::Vec::from_raw_parts_in(data.as_ptr(), length, capacity, alloc)
        }
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

impl<C: VecConfigNew<u8>> From<&str> for Vec<u8, C> {
    #[inline]
    fn from(data: &str) -> Self {
        Self::from_slice(data.as_bytes())
    }
}

#[cfg(feature = "alloc")]
impl<C> From<alloc::string::String> for Vec<u8, C>
where
    C: VecConfigAllocParts<u8, Alloc = Global, Index = usize>,
{
    #[inline]
    fn from(string: alloc::string::String) -> Self {
        string.into_bytes().into()
    }
}

#[cfg(feature = "alloc")]
impl<C: VecConfigNew<u8>> From<&alloc::string::String> for Vec<u8, C> {
    #[inline]
    fn from(string: &alloc::string::String) -> Self {
        string.as_bytes().into()
    }
}

#[cfg(feature = "alloc")]
impl<C> From<alloc::ffi::CString> for Vec<u8, C>
where
    C: VecConfigAllocParts<u8, Alloc = Global, Index = usize>,
{
    #[inline]
    fn from(string: alloc::ffi::CString) -> Self {
        string.into_bytes().into()
    }
}

#[cfg(feature = "alloc")]
impl<C: VecConfigNew<u8>> From<&alloc::ffi::CString> for Vec<u8, C> {
    #[inline]
    fn from(string: &alloc::ffi::CString) -> Self {
        string.as_bytes().into()
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
        self.as_slice().iter()
    }
}

impl<'a, T, C: VecConfig> IntoIterator for &'a mut Vec<T, C> {
    type Item = &'a mut T;
    type IntoIter = <&'a mut [T] as IntoIterator>::IntoIter;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.as_mut_slice().iter_mut()
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
        self.as_slice().eq(other.as_slice())
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
        self.as_slice().eq(*other)
    }
}

impl<T1, C1, T2> PartialEq<&mut [T2]> for Vec<T1, C1>
where
    T1: PartialEq<T2>,
    C1: VecConfig,
{
    #[inline]
    fn eq(&self, other: &&mut [T2]) -> bool {
        self.as_slice().eq(*other)
    }
}

impl<T1, C1, T2> PartialEq<[T2]> for Vec<T1, C1>
where
    T1: PartialEq<T2>,
    C1: VecConfig,
{
    #[inline]
    fn eq(&self, other: &[T2]) -> bool {
        self.as_slice().eq(other)
    }
}

impl<T1, C1, T2, const N: usize> PartialEq<&[T2; N]> for Vec<T1, C1>
where
    T1: PartialEq<T2>,
    C1: VecConfig,
{
    #[inline]
    fn eq(&self, other: &&[T2; N]) -> bool {
        self.as_slice().eq(&other[..])
    }
}

impl<T1, C1, T2, const N: usize> PartialEq<&mut [T2; N]> for Vec<T1, C1>
where
    T1: PartialEq<T2>,
    C1: VecConfig,
{
    #[inline]
    fn eq(&self, other: &&mut [T2; N]) -> bool {
        self.as_slice().eq(&other[..])
    }
}

impl<T1, C1, T2, const N: usize> PartialEq<[T2; N]> for Vec<T1, C1>
where
    T1: PartialEq<T2>,
    C1: VecConfig,
{
    #[inline]
    fn eq(&self, other: &[T2; N]) -> bool {
        self.as_slice().eq(&other[..])
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

#[cfg(feature = "allocator-api2")]
impl<A, B, C> PartialEq<allocator_api2::vec::Vec<A>> for Vec<B, C>
where
    B: PartialEq<A>,
    C: VecConfig,
{
    #[inline]
    fn eq(&self, other: &allocator_api2::vec::Vec<A>) -> bool {
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

#[cfg(feature = "allocator-api2")]
impl<A, B, C> PartialEq<Vec<B, C>> for allocator_api2::vec::Vec<A>
where
    B: PartialEq<A>,
    C: VecConfig,
{
    #[inline]
    fn eq(&self, other: &Vec<B, C>) -> bool {
        other.eq(self)
    }
}

impl<T, C: VecConfig, const N: usize> TryFrom<Vec<T, C>> for [T; N] {
    type Error = Vec<T, C>;

    #[inline]
    fn try_from(mut vec: Vec<T, C>) -> Result<Self, Self::Error> {
        if vec.len().to_usize() != N {
            return Err(vec);
        }

        unsafe { vec.set_len(C::Index::ZERO) };

        let data = vec.as_ptr() as *const [T; N];
        Ok(unsafe { data.read() })
    }
}

#[cfg(feature = "alloc")]
impl<T, C, const N: usize> TryFrom<Vec<T, C>> for alloc::boxed::Box<[T; N]>
where
    C: VecConfigAllocParts<T, Alloc = Global, Index = usize>,
{
    type Error = Vec<T, C>;

    #[inline]
    fn try_from(vec: Vec<T, C>) -> Result<Self, Self::Error> {
        if vec.len().to_usize() != N {
            return Err(vec);
        }

        let (data, length, capacity, _alloc) = vec.into_parts();
        assert_eq!(capacity, length);
        Ok(unsafe { alloc::boxed::Box::from_raw(data.as_ptr().cast()) })
    }
}

#[cfg(feature = "allocator-api2")]
impl<T, C, A, const N: usize> TryFrom<Vec<T, C>> for allocator_api2::boxed::Box<[T; N], A>
where
    C: VecConfigAllocParts<T, Alloc = A, Index = usize>,
    A: allocator_api2::alloc::Allocator,
{
    type Error = Vec<T, C>;

    #[inline]
    fn try_from(vec: Vec<T, C>) -> Result<Self, Self::Error> {
        if vec.len().to_usize() != N {
            return Err(vec);
        }

        let (data, length, capacity, alloc) = vec.into_parts();
        assert_eq!(capacity, length);
        Ok(unsafe { allocator_api2::boxed::Box::from_raw_in(data.as_ptr().cast(), alloc) })
    }
}

#[cfg(feature = "std")]
impl<C: VecConfig> std::io::Write for Vec<u8, C> {
    #[inline]
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }

    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self._try_reserve(buf.len(), false) {
            Ok(_) => {
                unsafe { self.extend_unchecked(buf) };
                Ok(buf.len())
            }
            Err(StorageError::CapacityLimit) => {
                // extend_within_capacity?
                let spare = self.capacity().to_usize() - self.len().to_usize();
                if spare > 0 {
                    unsafe { self.extend_unchecked(&buf[..spare]) };
                }
                Ok(spare)
            }
            Err(err) => Err(std::io::Error::new(std::io::ErrorKind::Other, err)),
        }
    }
}

#[cfg(feature = "zeroize")]
impl<T, C: crate::storage::RawAlloc> zeroize::Zeroize
    for Vec<T, crate::storage::ZeroizingAlloc<C>>
{
    #[inline]
    fn zeroize(&mut self) {
        self.shrink_to(0);
    }
}

#[cfg(feature = "zeroize")]
impl<T, C: crate::storage::RawAlloc> zeroize::ZeroizeOnDrop
    for Vec<T, crate::storage::ZeroizingAlloc<C>>
{
}

// From<CString>
// push_within_capacity

/// ```compile_fail,E0597
/// use flex_alloc::{storage::byte_storage, vec::Vec};
///
/// fn run<F: FnOnce() -> () + 'static>(f: F) { f() }
///
/// let mut buf = byte_storage::<10>();
/// let mut v = Vec::<usize, _>::new_in(&mut buf);
/// run(move || v.clear());
/// ```
#[cfg(doctest)]
fn _lifetime_check() {}
