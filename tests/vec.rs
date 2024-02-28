use core::mem::ManuallyDrop;
use std::mem::{size_of, size_of_val};

use flex_alloc::{
    aligned_byte_storage, array_storage, boxed::Box as FlexBox, byte_storage, storage::Global,
    vec::config::Custom, vec::Vec as FlexVec, Inline, Thin,
};

const SLICE: &[usize] = &[1, 2, 3, 4, 5];

#[cfg(feature = "alloc")]
#[test]
fn vec_default_global() {
    let _ = FlexVec::<usize>::default();
}

#[cfg(feature = "alloc")]
#[test]
fn vec_with_capacity_global() {
    let _ = FlexVec::<usize>::with_capacity(10);
}

#[cfg(feature = "alloc")]
#[test]
fn vec_check_capacity_growth() {
    let mut res = [0usize; 10];
    let mut vec = FlexVec::<usize>::new();
    for cap in res.iter_mut() {
        vec.push(1);
        *cap = vec.capacity();
    }
    assert_eq!(res, [4, 4, 4, 4, 8, 8, 8, 8, 16, 16]);
}

#[test]
fn vec_extend_new_global() {
    let mut v = FlexVec::<usize>::new();
    v.extend(SLICE.iter().cloned());
    assert!(v.capacity() >= SLICE.len());
    assert!(v.len() == SLICE.len());
    assert_eq!(v.as_slice(), SLICE);
}

#[test]
fn vec_extend_new_in_global() {
    let mut v = FlexVec::<usize>::new_in(Global);
    v.extend(SLICE.iter().cloned());
    assert!(v.capacity() >= SLICE.len());
    assert!(v.len() == SLICE.len());
    assert_eq!(v.as_slice(), SLICE);
}

#[test]
fn vec_extend_new_thin() {
    let mut v = FlexVec::<usize, Thin>::new();
    v.extend(SLICE.iter().cloned());
    assert!(v.capacity() >= SLICE.len());
    assert!(v.len() == SLICE.len());
    assert_eq!(v.as_slice(), SLICE);
    assert!(size_of_val(&v) == size_of::<*const ()>());
}

#[test]
fn vec_extend_new_in_thin() {
    let mut v = FlexVec::new_in(Thin);
    v.extend(SLICE.iter().cloned());
    assert!(v.capacity() >= SLICE.len());
    assert!(v.len() == SLICE.len());
    assert_eq!(v.as_slice(), SLICE);
}

#[cfg(feature = "alloc")]
#[test]
fn vec_with_capacity_thin() {
    let _ = FlexVec::<usize, Thin>::with_capacity(10);
}

#[test]
fn vec_extend_new_inline() {
    let mut v = FlexVec::<usize, Inline<10>>::new();
    v.extend(SLICE.iter().cloned());
    assert!(v.capacity() >= SLICE.len());
    assert!(v.len() == SLICE.len());
    assert_eq!(v.as_slice(), SLICE);
}

#[test]
fn vec_extend_new_in_inline() {
    let mut v = FlexVec::new_in(Inline::<10>);
    v.extend(SLICE.iter().cloned());
    assert!(v.capacity() >= SLICE.len());
    assert!(v.len() == SLICE.len());
    assert_eq!(v.as_slice(), SLICE);
}

#[test]
fn vec_extend_new_global_medium() {
    let mut data = [0usize; 100];
    for i in 0..100 {
        data[i] = i;
    }
    let mut v = FlexVec::<usize>::new();
    v.extend(data.iter().cloned());
    assert!(v.capacity() >= data.len());
    assert!(v.len() == data.len());
    assert_eq!(v.as_slice(), data);
}

#[test]
fn vec_extend_from_slice_new_global() {
    let mut v = FlexVec::<usize>::new();
    v.extend_from_slice(SLICE);
    assert!(v.capacity() >= SLICE.len());
    assert!(v.len() == SLICE.len());
    assert_eq!(v.as_slice(), SLICE);
}

#[test]
fn vec_from_iter_new_global() {
    let v = FlexVec::<usize>::from_iter(SLICE.iter().cloned());
    assert!(v.capacity() >= SLICE.len());
    assert!(v.len() == SLICE.len());
    assert_eq!(v.as_slice(), SLICE);
}

#[test]
fn vec_from_slice_global() {
    let v = FlexVec::<usize>::from_slice(SLICE);
    assert!(v.capacity() >= SLICE.len());
    assert!(v.len() == SLICE.len());
    assert_eq!(v.as_slice(), SLICE);
}

#[test]
fn vec_extend_grow_global() {
    let mut v = FlexVec::<usize>::with_capacity(1);
    v.extend_from_slice(SLICE);
    assert!(v.capacity() >= SLICE.len());
    assert!(v.len() == SLICE.len());
    assert_eq!(v.as_slice(), SLICE);
}

#[test]
fn vec_inline() {
    let mut b = FlexVec::<u32, Inline<10>>::new();
    b.push(32);
    assert_eq!(b.as_slice(), &[32]);
    assert_eq!(b.pop(), Some(32));
    assert_eq!(b.pop(), None);
    b.extend_from_slice(&[0, 1, 2, 3, 4, 5, 6, 7]);
    assert_eq!(b, &[0, 1, 2, 3, 4, 5, 6, 7][..]);
    assert_eq!(b.swap_remove(1), 1);
    assert_eq!(b, &[0, 7, 2, 3, 4, 5, 6][..]);
    assert_eq!(b.swap_remove(6), 6);
    assert_eq!(b, &[0, 7, 2, 3, 4, 5][..]);
}

#[test]
fn vec_clone_inline() {
    let v = FlexVec::<usize, Inline<10>>::from_slice(SLICE);
    let v2 = v.clone();
    assert_eq!(v, v2);
}

#[test]
fn vec_new_in_array() {
    let mut z = array_storage::<_, 32>();
    let mut b = FlexVec::new_in(&mut z);
    b.push(32);
    assert_eq!(b.as_slice(), &[32]);
    assert_eq!(b.pop(), Some(32));
    assert_eq!(b.pop(), None);
    b.extend_from_slice(&[0, 1, 2, 3, 4, 5, 6, 7]);
    assert_eq!(b, &[0, 1, 2, 3, 4, 5, 6, 7][..]);
    assert_eq!(b.swap_remove(1), 1);
    assert_eq!(b, &[0, 7, 2, 3, 4, 5, 6][..]);
    assert_eq!(b.swap_remove(6), 6);
    assert_eq!(b, &[0, 7, 2, 3, 4, 5][..]);
}

#[test]
fn vec_new_in_array_zst() {
    struct Item;
    let mut z = array_storage::<Item, 32>();
    let mut b = FlexVec::new_in(&mut z);
    assert_eq!(b.capacity(), 32);
    b.push(Item);
}

#[test]
fn vec_new_in_bytes() {
    let mut z = byte_storage::<500>();
    let mut b = FlexVec::new_in(&mut z);
    b.push(32);
    assert_eq!(b.as_slice(), &[32]);
    assert_eq!(b.pop(), Some(32));
    assert_eq!(b.pop(), None);
    b.extend_from_slice(&[0, 1, 2, 3, 4, 5, 6, 7]);
    assert_eq!(b, &[0, 1, 2, 3, 4, 5, 6, 7][..]);
    assert_eq!(b.swap_remove(1), 1);
    assert_eq!(b, &[0, 7, 2, 3, 4, 5, 6][..]);
    assert_eq!(b.swap_remove(6), 6);
    assert_eq!(b, &[0, 7, 2, 3, 4, 5][..]);
}

#[test]
fn vec_new_in_bytes_zst() {
    struct Item;
    let mut z = byte_storage::<500>();
    let mut b = FlexVec::<Item, _>::new_in(&mut z);
    assert_eq!(b.capacity(), usize::MAX);
    b.push(Item);
}

#[test]
fn vec_new_in_bytes_aligned() {
    let mut z = aligned_byte_storage::<i32, 500>();
    assert!(core::mem::align_of_val(&z) == core::mem::align_of::<i32>());
    let mut b = FlexVec::<i32, _>::new_in(&mut z);
    assert!(b.capacity() == 125);
    b.push(32);
    assert_eq!(b.as_slice(), &[32]);
    assert_eq!(b.pop(), Some(32));
    assert_eq!(b.pop(), None);
    b.extend_from_slice(&[0, 1, 2, 3, 4, 5, 6, 7]);
    assert_eq!(b, &[0, 1, 2, 3, 4, 5, 6, 7][..]);
    assert_eq!(b.swap_remove(1), 1);
    assert_eq!(b, &[0, 7, 2, 3, 4, 5, 6][..]);
    assert_eq!(b.swap_remove(6), 6);
    assert_eq!(b, &[0, 7, 2, 3, 4, 5][..]);
}

#[test]
fn vec_custom_index_new() {
    let mut v = FlexVec::<usize, Custom<Global, u8>>::new();
    v.resize(255, 1);
    assert!(v.try_push(1).is_err());
}

#[test]
fn vec_custom_index_new_in() {
    let mut v = FlexVec::new_in(Custom::<Global, u8>::new());
    v.resize(255, 1);
    assert!(v.try_push(1).is_err());
}

#[test]
fn vec_custom_index_thin_new() {
    let mut v = FlexVec::<usize, Custom<Thin, u8>>::new();
    v.resize(255, 1);
    assert!(v.try_push(1).is_err());
    assert!(size_of_val(&v) == size_of::<*const ()>());
}

// #[test]
// fn vec_inline_2() {
//     use crate::buffer::StackBuffer;

//     let mut z = StackBuffer::<[u32], 32>::new();
//     let mut b = FlexVec::with_buffer(&mut z);
//     b.insert_slice(0, &[1, 2, 3, 4]);
//     assert_eq!(b, &[1, 2, 3, 4]);
//     b.remove(1);
//     assert_eq!(b, &[1, 3, 4]);
// }

// #[cfg(feature = "alloc")]
// #[test]
// fn vec_into_static() {
//     use crate::buffer::StackBuffer;

//     let mut z = StackBuffer::<[u32], 32>::new();
//     let mut b = FlexVec::with_buffer(&mut z);
//     b.insert_slice(0, &[1, 2, 3, 4]);
//     let mut b2 = b.into_static();
//     b2[0] = 5;
//     assert_eq!(b2, &[5, 2, 3, 4]);
//     assert_eq!(unsafe { z[0].assume_init() }, 1);
// }

#[cfg(feature = "alloc")]
#[test]
fn vec_insert_2() {
    let mut b = FlexVec::<u32>::new();
    b.insert_slice(0, &[1, 2, 3, 4]);
    assert_eq!(b, &[1, 2, 3, 4]);
    b.remove(1);
    assert_eq!(b, &[1, 3, 4]);
}

#[cfg(feature = "alloc")]
#[test]
#[cfg_attr(miri, ignore)]
fn vec_insert_large() {
    let mut b = FlexVec::<u32>::new();
    let count = 1000000;
    b.extend(0..count);
    for i in 0..count {
        assert_eq!(b[i as usize], i);
    }
}

#[cfg(feature = "alloc")]
#[test]
fn vec_retain() {
    let mut b = FlexVec::<u32>::new();
    b.insert_slice(0, &[1, 2, 3, 4]);
    assert_eq!(b, &[1, 2, 3, 4]);
    b.retain(|i| i % 2 == 0);
    assert_eq!(b, &[2, 4]);
}

#[cfg(feature = "alloc")]
#[test]
fn vec_split_spare_mut() {
    let mut b = FlexVec::<u32>::with_capacity(10);
    b.insert_slice(0, &[1, 2, 3, 4]);
    let (vals, remain) = b.split_at_spare_mut();
    assert_eq!(vals, &[1, 2, 3, 4]);
    assert_eq!(remain.len(), b.capacity() - 4);
}

#[cfg(feature = "alloc")]
#[test]
fn vec_drain() {
    let mut b = FlexVec::<u32>::from_iter(0..10);
    b.drain(3..8);
    assert_eq!(&b[..], &[0, 1, 2, 8, 9]);
}

#[cfg(feature = "alloc")]
#[test]
fn vec_drain_forget() {
    let mut b = FlexVec::<u32>::from_iter(0..10);
    let _ = ManuallyDrop::new(b.drain(5..6));
    assert_eq!(&b[..], &[0, 1, 2, 3, 4]);
}

#[cfg(feature = "alloc")]
#[test]
fn vec_drain_iter() {
    let mut b = FlexVec::<u32>::from_iter(0..10);
    let mut drain = b.drain(5..8);
    assert_eq!(drain.len(), 3);
    assert_eq!(drain.next(), Some(5));
    assert_eq!(drain.next_back(), Some(7));
    assert_eq!(drain.next(), Some(6));
    assert_eq!(drain.next(), None);
    drop(drain);
    assert_eq!(&b[..], &[0, 1, 2, 3, 4, 8, 9]);
}

#[cfg(feature = "alloc")]
#[test]
fn vec_into_iter() {
    let b = FlexVec::<u32>::from_iter(0..3);
    let mut iter = b.into_iter();
    assert_eq!(iter.len(), 3);
    assert_eq!(iter.next(), Some(0));
    assert_eq!(iter.next_back(), Some(2));
    assert_eq!(iter.next(), Some(1));
    assert_eq!(iter.next(), None);
}

#[cfg(feature = "alloc")]
#[test]
fn vec_into_iter_skip() {
    let mut iter = FlexVec::<u32>::from_iter(0..3).into_iter().skip(1);
    assert_eq!(iter.next(), Some(1));
    assert_eq!(iter.next(), Some(2));
    assert_eq!(iter.next(), None);
}

#[cfg(feature = "alloc")]
#[test]
fn vec_zst() {
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    struct Zst;

    let mut b = FlexVec::<Zst>::new();
    b.push(Zst);
    assert_eq!(b.len(), 1);
    assert_eq!(b[0], Zst);
    assert_eq!(b.pop(), Some(Zst));
    assert_eq!(b.pop(), None);

    let mut b = FlexVec::<Zst>::new();
    b.extend([Zst, Zst, Zst]);
    let mut drain = b.drain(..);
    assert_eq!(drain.len(), 3);
    assert_eq!(drain.next(), Some(Zst));
    assert_eq!(drain.next_back(), Some(Zst));
    assert_eq!(drain.next(), Some(Zst));
    assert_eq!(drain.next(), None);

    let mut b = FlexVec::<Zst>::new();
    b.extend([Zst, Zst, Zst]);
    let mut iter = b.into_iter();
    assert_eq!(iter.len(), 3);
    assert_eq!(iter.next(), Some(Zst));
    assert_eq!(iter.next_back(), Some(Zst));
    assert_eq!(iter.next(), Some(Zst));
    assert_eq!(iter.next(), None);
}

#[cfg(feature = "alloc")]
#[test]
fn vec_collect() {
    let v: FlexVec<_> = (0..5).into_iter().collect();
    assert_eq!(v, &[0, 1, 2, 3, 4]);
}

#[cfg(feature = "alloc")]
#[test]
fn vec_append() {
    let mut v1 = FlexVec::<u32>::from([1, 2, 3]);
    let mut v2 = FlexVec::from([4, 5, 6]);
    v1.append(&mut v2);
    assert_eq!(v1, &[1, 2, 3, 4, 5, 6]);
    assert_eq!(v2, &[]);
}

#[cfg(feature = "alloc")]
#[test]
fn vec_append_to_empty() {
    let mut v1 = FlexVec::<u32>::new();
    let mut v2 = FlexVec::from([1, 2, 3]);
    v1.append(&mut v2);
    assert_eq!(v1, &[1, 2, 3]);
    assert_eq!(v2, &[]);
}

#[cfg(feature = "alloc")]
#[test]
fn vec_resize() {
    let mut v = FlexVec::<u32>::from([1, 2, 3]);
    v.resize(5, 10);
    assert_eq!(v, &[1, 2, 3, 10, 10]);
}

#[cfg(feature = "alloc")]
#[test]
fn vec_resize_with() {
    let mut v = FlexVec::<u32>::from([1, 2, 3]);
    v.resize_with(5, || 10);
    assert_eq!(v, &[1, 2, 3, 10, 10]);
}

#[cfg(feature = "alloc")]
#[test]
fn vec_split_off() {
    let mut v1 = FlexVec::<u32>::from([1, 2, 3, 4, 5, 6]);
    let v2 = v1.split_off(3);
    assert_eq!(v1, &[1, 2, 3]);
    assert_eq!(v2, &[4, 5, 6]);
}

#[cfg(feature = "alloc")]
#[test]
fn vec_splice_alloc() {
    let mut v = FlexVec::<u32>::from_iter(0..10);
    let mut splice = v.splice(1..5, [11, 12, 13, 14, 15]);
    assert_eq!(splice.next(), Some(1));
    assert_eq!(splice.next_back(), Some(4));
    drop(splice);
    assert_eq!(&v[..], &[0, 11, 12, 13, 14, 15, 5, 6, 7, 8, 9])
}

#[cfg(feature = "alloc")]
#[test]
fn vec_splice_compat() {
    let mut v = Vec::<u32>::from_iter(0..10);
    let mut splice = v.splice(1..5, [11, 12, 13, 14, 15]);
    assert_eq!(splice.next(), Some(1));
    assert_eq!(splice.next_back(), Some(4));
    drop(splice);
    assert_eq!(&v[..], &[0, 11, 12, 13, 14, 15, 5, 6, 7, 8, 9])
}

// #[test]
// fn vec_fixed_to_vec() {
//     use crate::buffer::StackBuffer;

//     let mut buf = StackBuffer::<[u32], 32>::new();
//     let mut vec = buf.to_vec();
//     vec.push(1);
//     drop(vec);
//     assert_eq!(unsafe { buf[0].assume_init() }, 1)
// }

#[cfg(feature = "alloc")]
#[test]
fn vec_dedup() {
    let mut vec = FlexVec::<u32>::from_iter([0, 1, 1, 0, 2, 4, 7, 7, 7]);
    vec.dedup();
    assert_eq!(vec, &[0, 1, 0, 2, 4, 7]);
}

#[cfg(feature = "alloc")]
#[test]
fn vec_into_std_vec() {
    let mut b = FlexVec::<u32>::with_capacity(10);
    b.insert_slice(0, &[1, 2, 3, 4]);
    let vec = std::vec::Vec::<u32>::from(b);
    assert_eq!(vec, &[1, 2, 3, 4]);
}

#[cfg(feature = "alloc")]
#[test]
fn vec_into_boxed_slice() {
    let vec = FlexVec::<_>::from_slice(SLICE);
    let boxed: FlexBox<_> = vec.into();
    assert_eq!(&*boxed, SLICE);
    let vec = FlexVec::<_>::from(boxed);
    assert_eq!(&vec, SLICE);
    assert_eq!(vec.capacity(), SLICE.len());
    let boxed = vec.into_boxed_slice();
    assert_eq!(&*boxed, SLICE);
}

#[cfg(feature = "alloc")]
#[test]
fn vec_into_std_boxed_slice() {
    let vec = FlexVec::<_>::from_slice(SLICE);
    let boxed: std::boxed::Box<_> = vec.into();
    assert_eq!(&*boxed, SLICE);
    let vec = FlexVec::<_>::from(boxed);
    assert_eq!(&vec, SLICE);
    assert_eq!(vec.capacity(), SLICE.len());
}

// FIXME test as_slice, into_iter(as_slice), drain(as_slice) for all empty vecs
