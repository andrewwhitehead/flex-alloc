#![cfg_attr(feature = "nightly", feature(allocator_api))]

use core::marker::PhantomData as Cfg;
#[cfg(feature = "alloc")]
use rstest::rstest;

#[cfg(feature = "alloc")]
use flex_alloc::alloc::{Global, SpillAlloc};
use flex_alloc::{
    alloc::{AllocateIn, AllocatorDefault},
    boxed::Box as FlexBox,
    storage::{aligned_byte_storage, byte_storage},
};

const SLICE: &[usize] = &[1, 2, 3, 4, 5];

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
struct Zst;

#[rstest]
#[cfg(feature = "alloc")]
#[case::global(Cfg::<Global>)]
fn box_default<A: AllocatorDefault>(#[case] _config: Cfg<A>) {
    let _ = FlexBox::<usize, A>::default();
}

#[rstest]
#[cfg(feature = "alloc")]
#[case::global(Cfg::<Global>)]
fn box_new_empty_slice<A: AllocatorDefault>(#[case] _config: Cfg<A>) {
    let v = FlexBox::<[usize], A>::new_uninit_slice(0);
    assert!(v.is_empty());
}

#[rstest]
#[cfg_attr(feature = "alloc", case::global(Global))]
#[case::aligned(&mut aligned_byte_storage::<usize, 1000>())]
#[case::bytes(&mut byte_storage::<1000>())]
fn box_new_in<A: AllocateIn>(#[case] buf: A) {
    let boxed = FlexBox::new_in(99usize, buf);
    assert_eq!(*boxed, 99);
}

#[rstest]
#[cfg_attr(feature = "alloc", case::global(Global))]
#[case::aligned(&mut aligned_byte_storage::<usize, 1000>())]
#[case::bytes(&mut byte_storage::<1000>())]
fn box_empty_slice_in<A: AllocateIn>(#[case] buf: A) {
    let boxed = FlexBox::<[usize], _>::new_uninit_slice_in(0, buf);
    assert!(boxed.is_empty());
}

#[rstest]
#[cfg_attr(feature = "alloc", case::global(Global))]
#[case::aligned(&mut aligned_byte_storage::<usize, 1000>())]
#[case::bytes(&mut byte_storage::<1000>())]
fn box_from_slice_in<A: AllocateIn>(#[case] buf: A) {
    let boxed = FlexBox::from_slice_in(&[1, 3, 5], buf);
    assert_eq!(boxed.as_ref(), &[1, 3, 5]);
}

#[rstest]
#[cfg_attr(feature = "alloc", case::global(Global))]
#[case::aligned(&mut aligned_byte_storage::<usize, 1000>())]
#[case::bytes(&mut byte_storage::<1000>())]
fn box_new_in_zst<A: AllocateIn>(#[case] buf: A) {
    let v = FlexBox::new_in(Zst, buf);
    assert_eq!(*v, Zst);
}

#[rstest]
#[cfg(feature = "alloc")]
#[case::global(Cfg::<Global>)]
fn box_clone<A: AllocatorDefault>(#[case] _config: Cfg<A>) {
    let b1 = FlexBox::<_, A>::new(98);
    let b2 = b1.clone();
    assert_eq!(b1, b2);
}

#[rstest]
#[cfg(feature = "alloc")]
#[case::global(Cfg::<Global>)]
fn box_from_iter<A: AllocatorDefault>(#[case] _config: Cfg<A>) {
    let v = FlexBox::<[usize], A>::from_iter(SLICE.iter().cloned());
    assert!(v.len() == SLICE.len());
    assert_eq!(&*v, SLICE);
}

#[cfg(feature = "alloc")]
#[test]
fn box_new_in_bytes_spill_alloc() {
    let mut z = byte_storage::<20>();
    // alloc should fit inside byte storage
    let b = FlexBox::new_in(97usize, z.spill_alloc());
    assert_eq!(*b, 97);
    drop(b);

    // alloc will not fit inside byte storage
    let b = FlexBox::from_slice_in(&[0, 1, 2, 3, 4, 5, 6, 7], z.spill_alloc());
    assert_eq!(&*b, &[0, 1, 2, 3, 4, 5, 6, 7]);
}
