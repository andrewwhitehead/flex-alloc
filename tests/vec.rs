use core::marker::PhantomData as Cfg;
use core::mem::ManuallyDrop;
#[cfg(feature = "alloc")]
use core::mem::{size_of, size_of_val};

use rstest::rstest;

use const_default::ConstDefault;
use flex_alloc::{
    index::Index,
    storage::{aligned_byte_storage, array_storage, byte_storage, Inline},
    vec::{
        config::{VecConfig, VecConfigNew, VecNewIn},
        InlineVec, Vec as FlexVec,
    },
};

#[cfg(feature = "alloc")]
use flex_alloc::{
    storage::{Global, Thin, WithAlloc},
    vec,
    vec::{config::Custom, ThinVec},
};

const SLICE: &[usize] = &[1, 2, 3, 4, 5];

#[derive(Default, Copy, Clone)]
struct Zst;

#[rstest]
#[cfg_attr(feature="alloc", case::global(Cfg::<Global>))]
#[cfg_attr(feature="alloc", case::thin(Cfg::<Thin>))]
#[cfg_attr(feature="alloc", case::custom(Cfg::<Custom<Global, u8>>))]
#[case::inline(Cfg::<Inline<10>>)]
fn vec_default<C: VecConfigNew<usize>>(#[case] _config: Cfg<C>) {
    let _ = FlexVec::<usize, C>::default();
}

#[rstest]
#[cfg_attr(feature="alloc", case::global(Cfg::<Global>))]
#[cfg_attr(feature="alloc", case::thin(Cfg::<Thin>))]
#[cfg_attr(feature="alloc", case::custom(Cfg::<Custom<Global, u8>>))]
#[case::inline(Cfg::<Inline<10>>)]
fn vec_new_as_slice<C: VecConfigNew<usize>>(#[case] _config: Cfg<C>) {
    let mut v = FlexVec::<usize, C>::new();
    assert!(v.as_slice().is_empty());
    assert!(v.drain(..).as_slice().is_empty());
    assert!(v.into_iter().as_slice().is_empty());
}

#[rstest]
#[cfg_attr(feature = "alloc", case::global(Global))]
#[cfg_attr(feature = "alloc", case::thin(Thin))]
#[cfg_attr(feature="alloc", case::custom(Custom::<Global, u8>::default()))]
#[case::array(&mut array_storage::<_, 10>())]
#[case::aligned(&mut aligned_byte_storage::<usize, 1000>())]
#[case::bytes(&mut byte_storage::<1000>())]
#[case::inline(Inline::<10>)]
fn vec_new_in_as_slice<C: VecNewIn<usize>>(#[case] buf: C) {
    let mut v = FlexVec::new_in(buf);
    assert!(v.as_slice().is_empty());
    assert!(v.drain(..).as_slice().is_empty());
    assert!(v.into_iter().as_slice().is_empty());
}

#[rstest]
#[cfg_attr(feature="alloc", case::global(Cfg::<Global>))]
#[cfg_attr(feature="alloc", case::thin(Cfg::<Thin>))]
#[cfg_attr(feature="alloc", case::custom(Cfg::<Custom<Global, u8>>))]
#[case::inline(Cfg::<Inline<10>>)]
fn vec_with_capacity_push<C: VecConfigNew<usize>>(#[case] _config: Cfg<C>) {
    let mut v = FlexVec::<usize, C>::with_capacity(C::Index::from_usize(10));
    v.push(1);
    assert_eq!(v.len().to_usize(), 1);
}

#[rstest]
#[cfg_attr(feature = "alloc", case::global(Global))]
#[cfg_attr(feature = "alloc", case::thin(Thin))]
#[cfg_attr(feature="alloc", case::custom(Custom::<Global, u8>::default()))]
#[case::array(&mut array_storage::<_, 10>())]
#[case::aligned(&mut aligned_byte_storage::<usize, 1000>())]
#[case::bytes(&mut byte_storage::<1000>())]
#[case::inline(Inline::<10>)]
fn vec_with_capacity_in_push<C: VecNewIn<usize>>(#[case] buf: C) {
    let mut v = FlexVec::with_capacity_in(<C::Config as VecConfig>::Index::from_usize(10), buf);
    v.push(1);
    assert_eq!(v.len().to_usize(), 1);
}

#[rstest]
#[cfg_attr(feature="alloc", case::global(Cfg::<Global>))]
#[cfg_attr(feature="alloc", case::thin(Cfg::<Thin>))]
#[cfg_attr(feature="alloc", case::custom(Cfg::<Custom<Global, u8>>))]
#[case::inline(Cfg::<Inline<10>>)]
fn vec_with_capacity_push_zst<C: VecConfigNew<Zst>>(#[case] _config: Cfg<C>) {
    let mut v = FlexVec::<Zst, C>::with_capacity(C::Index::from_usize(10));
    v.push(Zst);
    assert_eq!(v.len().to_usize(), 1);
}

#[rstest]
#[cfg_attr(feature="alloc", case::global(Cfg::<Global>))]
#[cfg_attr(feature="alloc", case::thin(Cfg::<Thin>))]
#[cfg_attr(feature="alloc", case::custom(Cfg::<Custom<Global, u8>>))]
#[case::inline(Cfg::<Inline<10>>)]
fn vec_clone<C: VecConfigNew<usize>>(#[case] _config: Cfg<C>) {
    let mut v = FlexVec::<usize, C>::new();
    v.push(1);
    let v2 = v.clone();
    assert_eq!(v, v2);
    assert_eq!(v2, [1]);
}

#[rstest]
#[cfg_attr(feature="alloc", case::global(Cfg::<Global>))]
#[cfg_attr(feature="alloc", case::thin(Cfg::<Thin>))]
#[cfg_attr(feature="alloc", case::custom(Cfg::<Custom<Global, u8>>))]
#[case::inline(Cfg::<Inline<10>>)]
fn vec_append<C: VecConfigNew<usize>>(#[case] _config: Cfg<C>) {
    let mut v1 = FlexVec::<usize, C>::from([1, 2, 3]);
    let mut v2 = FlexVec::from([4, 5, 6]);
    v1.append(&mut v2);
    assert_eq!(v1, &[1, 2, 3, 4, 5, 6]);
    assert!(v2.is_empty());
}

#[rstest]
#[cfg_attr(feature="alloc", case::global(Cfg::<Global>))]
#[cfg_attr(feature="alloc", case::thin(Cfg::<Thin>))]
#[cfg_attr(feature="alloc", case::custom(Cfg::<Custom<Global, u8>>))]
#[case::inline(Cfg::<Inline<10>>)]
fn vec_append_to_empty<C: VecConfigNew<usize>>(#[case] _config: Cfg<C>) {
    let mut v1 = FlexVec::<usize, C>::new();
    let mut v2 = FlexVec::from([1, 2, 3]);
    v1.append(&mut v2);
    assert_eq!(v1, &[1, 2, 3]);
    assert!(v2.is_empty());
}

#[rstest]
#[cfg_attr(feature="alloc", case::global(Cfg::<Global>))]
#[cfg_attr(feature="alloc", case::thin(Cfg::<Thin>))]
#[cfg_attr(feature="alloc", case::custom(Cfg::<Custom<Global, u8>>))]
#[case::inline(Cfg::<Inline<10>>)]
fn vec_from_iter<C: VecConfigNew<usize>>(#[case] _config: Cfg<C>) {
    let v = FlexVec::<usize, C>::from_iter(SLICE.iter().cloned());
    assert!(v.capacity().to_usize() >= SLICE.len());
    assert!(v.len().to_usize() == SLICE.len());
    assert_eq!(v.as_slice(), SLICE);
}

#[rstest]
#[cfg_attr(feature="alloc", case::global(Cfg::<Global>))]
#[cfg_attr(feature="alloc", case::thin(Cfg::<Thin>))]
#[cfg_attr(feature="alloc", case::custom(Cfg::<Custom<Global, u8>>))]
#[case::inline(Cfg::<Inline<10>>)]
fn vec_from_slice<C: VecConfigNew<usize>>(#[case] _config: Cfg<C>) {
    let v = FlexVec::<usize, C>::from_slice(SLICE);
    assert!(v.capacity().to_usize() >= SLICE.len());
    assert!(v.len().to_usize() == SLICE.len());
    assert_eq!(v.as_slice(), SLICE);
}

#[rstest]
#[cfg_attr(feature="alloc", case::global(Cfg::<Global>))]
#[cfg_attr(feature="alloc", case::thin(Cfg::<Thin>))]
#[cfg_attr(feature="alloc", case::custom(Cfg::<Custom<Global, u8>>))]
#[case::inline(Cfg::<Inline<10>>)]
fn vec_extend_new<C: VecConfigNew<usize>>(#[case] _config: Cfg<C>) {
    let mut v = FlexVec::<usize, C>::new();
    v.extend(SLICE.iter().cloned());
    assert!(v.capacity().to_usize() >= SLICE.len());
    assert!(v.len().to_usize() == SLICE.len());
    assert_eq!(v.as_slice(), SLICE);
}

#[rstest]
#[cfg_attr(feature = "alloc", case::global(Global))]
#[cfg_attr(feature = "alloc", case::thin(Thin))]
#[cfg_attr(feature="alloc", case::custom(Custom::<Global, u8>::default()))]
#[case::array(&mut array_storage::<_, 10>())]
#[case::aligned(&mut aligned_byte_storage::<usize, 1000>())]
#[case::bytes(&mut byte_storage::<1000>())]
#[case::inline(Inline::<10>)]
fn vec_extend_new_in<C: VecNewIn<usize>>(#[case] buf: C) {
    let mut v = FlexVec::new_in(buf);
    v.extend(SLICE.iter().cloned());
    assert!(v.capacity().to_usize() >= SLICE.len());
    assert!(v.len().to_usize() == SLICE.len());
    assert_eq!(v.as_slice(), SLICE);
}

#[rstest]
#[cfg_attr(feature="alloc", case::global(Cfg::<Global>))]
#[cfg_attr(feature="alloc", case::thin(Cfg::<Thin>))]
#[cfg_attr(feature="alloc", case::custom(Cfg::<Custom<Global, u8>>))]
#[case::inline(Cfg::<Inline<10>>)]
fn vec_extend_from_slice_new<C: VecConfigNew<usize>>(#[case] _config: Cfg<C>) {
    let mut v = FlexVec::<usize, C>::new();
    v.extend_from_slice(SLICE);
    assert!(v.capacity().to_usize() >= SLICE.len());
    assert!(v.len().to_usize() == SLICE.len());
    assert_eq!(v.as_slice(), SLICE);
}

#[rstest]
#[cfg_attr(feature = "alloc", case::global(Global))]
#[cfg_attr(feature = "alloc", case::thin(Thin))]
#[cfg_attr(feature="alloc", case::custom(Custom::<Global, u8>::default()))]
#[case::array(&mut array_storage::<_, 10>())]
#[case::aligned(&mut aligned_byte_storage::<usize, 1000>())]
#[case::bytes(&mut byte_storage::<1000>())]
#[case::inline(Inline::<10>)]
fn vec_extend_from_slice_new_in<C: VecNewIn<usize>>(#[case] buf: C) {
    let mut v = FlexVec::new_in(buf);
    v.extend_from_slice(SLICE);
    assert!(v.capacity().to_usize() >= SLICE.len());
    assert!(v.len().to_usize() == SLICE.len());
    assert_eq!(v.as_slice(), SLICE);
}

#[rstest]
#[cfg_attr(feature="alloc", case::global(Cfg::<Global>))]
#[cfg_attr(feature="alloc", case::thin(Cfg::<Thin>))]
#[cfg_attr(feature="alloc", case::custom(Cfg::<Custom<Global, u8>>))]
#[case::inline(Cfg::<Inline<10>>)]
fn vec_extend_from_within_new<C: VecConfigNew<usize>>(#[case] _config: Cfg<C>) {
    let mut v = FlexVec::<usize, C>::from_slice(SLICE);
    let len = C::Index::from_usize(SLICE.len());
    v.extend_from_within(..len);
    assert_eq!(v.len(), len.saturating_mul(2));
    assert_eq!(&v[..SLICE.len()], SLICE);
    assert_eq!(&v[SLICE.len()..], SLICE);
}

#[rstest]
#[cfg_attr(feature="alloc", case::global(Cfg::<Global>))]
#[cfg_attr(feature="alloc", case::thin(Cfg::<Thin>))]
#[cfg_attr(feature="alloc", case::custom(Cfg::<Custom<Global, u8>>))]
#[case::inline(Cfg::<Inline<10>>)]
fn vec_dedup<C: VecConfigNew<usize>>(#[case] _config: Cfg<C>) {
    let mut vec = FlexVec::<usize, C>::from_iter([0, 1, 1, 0, 2, 4, 7, 7, 7]);
    vec.dedup();
    assert_eq!(vec, &[0, 1, 0, 2, 4, 7]);
}

#[rstest]
#[cfg_attr(feature="alloc", case::global(Cfg::<Global>))]
#[cfg_attr(feature="alloc", case::thin(Cfg::<Thin>))]
#[cfg_attr(feature="alloc", case::custom(Cfg::<Custom<Global, u8>>))]
#[case::inline(Cfg::<Inline<10>>)]
fn vec_drain<C: VecConfigNew<usize>>(#[case] _config: Cfg<C>) {
    let mut b = FlexVec::<usize, C>::from_iter(0..10);
    b.drain(C::Index::from_usize(3)..C::Index::from_usize(8));
    assert_eq!(&b[..], &[0, 1, 2, 8, 9]);
}

#[rstest]
#[cfg_attr(feature="alloc", case::global(Cfg::<Global>))]
#[cfg_attr(feature="alloc", case::thin(Cfg::<Thin>))]
#[cfg_attr(feature="alloc", case::custom(Cfg::<Custom<Global, u8>>))]
#[case::inline(Cfg::<Inline<10>>)]
fn vec_drain_iter<C: VecConfigNew<usize>>(#[case] _config: Cfg<C>) {
    let mut b = FlexVec::<usize, C>::from_iter(0..10);
    let mut drain = b.drain(C::Index::from_usize(5)..C::Index::from_usize(8));
    assert_eq!(drain.len().to_usize(), 3);
    assert_eq!(drain.next(), Some(5));
    assert_eq!(drain.next_back(), Some(7));
    assert_eq!(drain.next(), Some(6));
    assert_eq!(drain.next(), None);
    drop(drain);
    assert_eq!(&b[..], &[0, 1, 2, 3, 4, 8, 9]);
}

#[rstest]
#[cfg_attr(feature="alloc", case::global(Cfg::<Global>))]
#[cfg_attr(feature="alloc", case::thin(Cfg::<Thin>))]
#[cfg_attr(feature="alloc", case::custom(Cfg::<Custom<Global, u8>>))]
#[case::inline(Cfg::<Inline<10>>)]
fn vec_drain_forget<C: VecConfigNew<usize>>(#[case] _config: Cfg<C>) {
    let mut b = FlexVec::<usize, C>::from_iter(0..10);
    let _ = ManuallyDrop::new(b.drain(C::Index::from_usize(5)..C::Index::from_usize(6)));
    assert_eq!(&b[..], &[0, 1, 2, 3, 4]);
}

#[rstest]
#[cfg_attr(feature="alloc", case::global(Cfg::<Global>))]
#[cfg_attr(feature="alloc", case::thin(Cfg::<Thin>))]
#[cfg_attr(feature="alloc", case::custom(Cfg::<Custom<Global, u8>>))]
#[case::inline(Cfg::<Inline<10>>)]
fn vec_into_iter<C: VecConfigNew<usize>>(#[case] _config: Cfg<C>) {
    let b = FlexVec::<usize, C>::from_iter(0..3);
    let mut iter = b.into_iter();
    assert_eq!(iter.len().to_usize(), 3);
    assert_eq!(iter.next(), Some(0));
    assert_eq!(iter.next_back(), Some(2));
    assert_eq!(iter.next(), Some(1));
    assert_eq!(iter.next(), None);
}

#[rstest]
#[cfg_attr(feature="alloc", case::global(Cfg::<Global>))]
#[cfg_attr(feature="alloc", case::thin(Cfg::<Thin>))]
#[cfg_attr(feature="alloc", case::custom(Cfg::<Custom<Global, u8>>))]
#[case::inline(Cfg::<Inline<10>>)]
fn vec_into_iter_skip<C: VecConfigNew<usize>>(#[case] _config: Cfg<C>) {
    let mut iter = FlexVec::<usize, C>::from_iter(0..3).into_iter().skip(1);
    assert_eq!(iter.next(), Some(1));
    assert_eq!(iter.next(), Some(2));
    assert_eq!(iter.next(), None);
}

#[rstest]
#[cfg_attr(feature="alloc", case::global(Cfg::<Global>))]
#[cfg_attr(feature="alloc", case::thin(Cfg::<Thin>))]
#[cfg_attr(feature="alloc", case::custom(Cfg::<Custom<Global, u8>>))]
#[case::inline(Cfg::<Inline<10>>)]
fn vec_into_iter_collect<C: VecConfigNew<usize>>(#[case] _config: Cfg<C>) {
    let v: FlexVec<usize, C> = (0..5).collect();
    assert_eq!(v, &[0, 1, 2, 3, 4]);
}

#[cfg(feature = "alloc")]
#[test]
fn vec_retain() {
    let mut b = FlexVec::<usize>::new();
    b.insert_slice(0, &[1, 2, 3, 4]);
    assert_eq!(b, &[1, 2, 3, 4]);
    b.retain(|i| i % 2 == 0);
    assert_eq!(b, &[2, 4]);
}

#[rstest]
#[cfg_attr(feature="alloc", case::global(Cfg::<Global>))]
#[cfg_attr(feature="alloc", case::thin(Cfg::<Thin>))]
#[cfg_attr(feature="alloc", case::custom(Cfg::<Custom<Global, u8>>))]
#[case::inline(Cfg::<Inline<10>>)]
fn vec_resize<C: VecConfigNew<usize>>(#[case] _config: Cfg<C>) {
    let mut v = FlexVec::<usize, C>::from([1, 2, 3]);
    v.resize(C::Index::from_usize(5), 10);
    assert_eq!(v, &[1, 2, 3, 10, 10]);
}

#[rstest]
#[cfg_attr(feature="alloc", case::global(Cfg::<Global>))]
#[cfg_attr(feature="alloc", case::thin(Cfg::<Thin>))]
#[cfg_attr(feature="alloc", case::custom(Cfg::<Custom<Global, u8>>))]
#[case::inline(Cfg::<Inline<10>>)]
fn vec_resize_with<C: VecConfigNew<usize>>(#[case] _config: Cfg<C>) {
    let mut v = FlexVec::<usize, C>::from([1, 2, 3]);
    v.resize_with(C::Index::from_usize(5), || 10);
    assert_eq!(v, &[1, 2, 3, 10, 10]);
}

#[rstest]
#[cfg_attr(feature="alloc", case::global(Cfg::<Global>))]
#[cfg_attr(feature="alloc", case::thin(Cfg::<Thin>))]
#[cfg_attr(feature="alloc", case::custom(Cfg::<Custom<Global, u8>>))]
#[case::inline(Cfg::<Inline<10>>)]
fn vec_split_off<C: VecConfigNew<usize>>(#[case] _config: Cfg<C>) {
    let mut v1 = FlexVec::<usize, C>::from([1, 2, 3, 4, 5, 6]);
    let v2 = v1.split_off(C::Index::from_usize(3));
    assert_eq!(v1, &[1, 2, 3]);
    assert_eq!(v2, &[4, 5, 6]);
}

#[rstest]
#[cfg_attr(feature="alloc", case::global(Cfg::<Global>))]
#[cfg_attr(feature="alloc", case::thin(Cfg::<Thin>))]
#[cfg_attr(feature="alloc", case::custom(Cfg::<Custom<Global, u8>>))]
#[case::inline(Cfg::<Inline<20>>)]
fn vec_splice<C: VecConfigNew<usize>>(#[case] _config: Cfg<C>) {
    let mut v = FlexVec::<usize, C>::from_iter(0..10);
    let mut splice = v.splice(
        C::Index::from_usize(1)..C::Index::from_usize(5),
        [11, 12, 13, 14, 15],
    );
    assert_eq!(splice.next(), Some(1));
    assert_eq!(splice.next_back(), Some(4));
    drop(splice);
    assert_eq!(&v[..], &[0, 11, 12, 13, 14, 15, 5, 6, 7, 8, 9])
}

#[rstest]
#[cfg_attr(feature="alloc", case::global(Cfg::<Global>))]
#[cfg_attr(feature="alloc", case::thin(Cfg::<Thin>))]
#[cfg_attr(feature="alloc", case::custom(Cfg::<Custom<Global, u8>>))]
#[case::inline(Cfg::<Inline<10>>)]
fn vec_split_spare_mut<C: VecConfigNew<usize>>(#[case] _config: Cfg<C>) {
    let mut b = FlexVec::<usize, C>::with_capacity(C::Index::from_usize(10));
    b.insert_slice(C::Index::ZERO, &[1, 2, 3, 4]);
    let (vals, remain) = b.split_at_spare_mut();
    assert_eq!(vals, &[1, 2, 3, 4]);
    assert_eq!(remain.len().to_usize(), b.capacity().to_usize() - 4);
}

#[cfg(feature = "alloc")]
#[test]
fn vec_check_grow_double() {
    let mut res = [0usize; 10];
    let mut vec = FlexVec::<usize>::new();
    for cap in res.iter_mut() {
        vec.push(1);
        *cap = vec.capacity();
    }
    assert_eq!(res, [4, 4, 4, 4, 8, 8, 8, 8, 16, 16]);
}

#[cfg(feature = "alloc")]
#[test]
#[cfg_attr(miri, ignore)]
fn vec_extend_large_global() {
    let mut b = FlexVec::<usize>::new();
    let count = 1000000;
    b.extend(0..count);
    for i in 0..count {
        assert_eq!(b[i], i);
    }
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

#[cfg(all(feature = "alloc", feature = "allocator-api2"))]
#[test]
fn vec_into_allocator_api2_vec() {
    let mut b = FlexVec::<usize>::with_capacity(10);
    b.insert_slice(0, &[1, 2, 3, 4]);
    let vec = allocator_api2::vec::Vec::<usize>::from(b);
    assert_eq!(vec, &[1, 2, 3, 4]);
}

#[cfg(feature = "alloc")]
#[test]
fn vec_into_std_vec() {
    let mut b = FlexVec::<usize>::with_capacity(10);
    b.insert_slice(0, &[1, 2, 3, 4]);
    let vec = std::vec::Vec::<usize>::from(b);
    assert_eq!(vec, &[1, 2, 3, 4]);
}

#[cfg(feature = "alloc")]
#[test]
fn vec_into_boxed_slice() {
    let vec = FlexVec::<_>::from_slice(SLICE);
    let boxed: Box<_> = vec.into();
    assert_eq!(&*boxed, SLICE);
    let vec = FlexVec::<_>::from(boxed);
    assert_eq!(&vec, SLICE);
    assert_eq!(vec.capacity(), SLICE.len());
    let boxed = vec.into_boxed_slice();
    assert_eq!(&*boxed, SLICE);
}

#[cfg(all(feature = "alloc", feature = "allocator-api2"))]
#[test]
fn vec_into_allocator_api2_boxed_slice() {
    let vec = FlexVec::<_>::from_slice(SLICE);
    let boxed: allocator_api2::boxed::Box<_> = vec.into();
    assert_eq!(&*boxed, SLICE);
    let vec = FlexVec::<_>::from(boxed);
    assert_eq!(&vec, SLICE);
    assert_eq!(vec.capacity(), SLICE.len());
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

#[test]
fn vec_inline() {
    let mut b = InlineVec::<usize, 10>::new();
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

#[cfg(feature = "alloc")]
#[test]
fn vec_new_in_array_with_alloc() {
    let mut z = array_storage::<_, 3>();
    // alloc will fit inside array storage
    let mut b = FlexVec::new_in(z.with_alloc());
    b.push(32);
    drop(b);

    // alloc will not fit inside array storage
    let mut b = FlexVec::from_slice_in(&[0, 1, 2, 3, 4, 5, 6, 7], z.with_alloc());
    b.extend_from_slice(&[0, 1, 2, 3, 4, 5, 6, 7]);
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

#[cfg(feature = "alloc")]
#[test]
fn vec_new_in_bytes_with_alloc() {
    let mut z = byte_storage::<20>();
    // alloc should fit inside byte storage
    let mut b = FlexVec::new_in(z.with_alloc());
    b.push(32);
    drop(b);

    // alloc will not fit inside byte storage
    let mut b = FlexVec::from_slice_in(&[0, 1, 2, 3, 4, 5, 6, 7], z.with_alloc());
    b.extend_from_slice(&[0, 1, 2, 3, 4, 5, 6, 7]);
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

#[cfg(feature = "alloc")]
#[test]
fn vec_thin() {
    let mut v = ThinVec::<usize>::new();
    v.push(32);
    assert_eq!(&v, &[32]);
    assert!(size_of_val(&v) == size_of::<*const ()>());
}

#[cfg(feature = "alloc")]
#[test]
fn vec_custom_index_capacity() {
    let mut v = FlexVec::<usize, Custom<Global, u8>>::new();
    v.resize(255, 1);
    assert!(v.try_push(1).is_err());

    let mut v = FlexVec::new_in(Custom::<Global, u8>::DEFAULT);
    v.resize(255, 1);
    assert!(v.try_push(1).is_err());
}

#[cfg(feature = "alloc")]
#[test]
fn vec_custom_index_thin_new() {
    let mut v = FlexVec::<usize, Custom<Thin, u8>>::new();
    v.resize(255, 1);
    assert!(v.try_push(1).is_err());
    assert!(size_of_val(&v) == size_of::<*const ()>());
}

#[cfg(feature = "alloc")]
#[test]
fn vec_macro() {
    let v: FlexVec<i32> = vec![];
    assert_eq!(&v, &[]);

    let v: FlexVec<i32> = vec![in Global];
    assert_eq!(&v, &[]);

    let v = vec![1; 5];
    assert_eq!(&v, &[1, 1, 1, 1, 1]);

    let v = vec![in Global; 1; 5];
    assert_eq!(&v, &[1, 1, 1, 1, 1]);

    let v = vec![1, 2, 3];
    assert_eq!(&v, &[1, 2, 3]);

    let v = vec![in Global; 1, 2, 3];
    assert_eq!(&v, &[1, 2, 3]);
}
