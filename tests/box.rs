use flex_alloc::{boxed::Box, byte_storage, StorageError};

#[cfg(feature = "alloc")]
use flex_alloc::storage::Global;

#[cfg(feature = "alloc")]
#[test]
fn test_box_alloc() {
    let b = Box::<u32>::new(10u32);
    assert_eq!(*b, 10u32);
}

#[cfg(feature = "alloc")]
#[test]
fn test_box_in_alloc() {
    let b = Box::new_in(10u32, Global);
    assert_eq!(*b, 10u32);
}

#[cfg(feature = "alloc")]
#[test]
fn test_box_zst_in_alloc() {
    #[derive(Debug, PartialEq)]
    struct A;
    let b = Box::new_in(A, Global);
    assert_eq!(b.as_ref(), &A);
}

#[test]
fn test_box_in_byte_buffer() {
    let mut z = byte_storage::<500>();
    let b = Box::new_in(10u32, &mut z);
    assert_eq!(b.as_ref(), &10u32);
}

#[test]
fn test_box_zst_in_byte_buffer() {
    #[derive(Debug, PartialEq)]
    struct A;
    let mut z = byte_storage::<500>();
    let b = Box::new_in(A, &mut z);
    assert_eq!(b.as_ref(), &A);
}

#[test]
fn test_box_in_byte_buffer_over_cap() {
    let mut z = byte_storage::<3>();
    let res = Box::try_new_in(10u32, &mut z);
    assert_eq!(res, Err(StorageError::CapacityLimit));
}

#[cfg(feature = "alloc")]
#[test]
fn test_box_uninit() {
    let b = Box::new_uninit_in(Global);
    let b = Box::write(b, 10u32);
    assert_eq!(*b, 10u32);
    let b2 = b.clone();
    assert_eq!(*b2, 10u32);
}

#[cfg(feature = "alloc")]
#[test]
fn test_box_clone() {
    let b = Box::<u32>::new(10u32);
    let mut b2 = Box::new(11u32);
    b2.clone_from(&b);
    assert_eq!(*b2, 10u32);
    let b3 = b.clone();
    assert_eq!(*b3, 10u32);
}

#[cfg(feature = "alloc")]
#[test]
fn test_empty_str() {
    let s = Box::<str>::default();
    assert_eq!(&*s, "");
}

#[cfg(feature = "alloc")]
#[test]
fn test_boxed_str_from() {
    let s = Box::<str>::from("hello");
    assert_eq!(&*s, "hello");
}

// #[cfg(feature = "alloc")]
// #[test]
// fn test_boxed_from_alloc() {
//     let s = alloc::boxed::Box::<str>::from("hello");
//     let s2 = AllocBox::<str>::from(s);
//     assert_eq!(&*s2, "hello");
// }
