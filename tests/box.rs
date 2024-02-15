use flex_vec::boxed::Box;
use flex_vec::storage::Global;

#[cfg(feature = "alloc")]
#[test]
fn test_box_alloc() {
    let b = Box::<u32>::new(10u32);
    assert_eq!(*b, 10u32);
}

#[cfg(feature = "alloc")]
#[test]
fn test_box_in() {
    let b = Box::new_in(10u32, Global);
    assert_eq!(*b, 10u32);
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

// #[cfg(feature = "alloc")]
// #[test]
// fn test_vec_into_boxed_slice() {
//     let vec = Vec::<_, AllocGlobal>::from_slice(&[1, 2, 3, 4]);
//     let boxed: Box<_, AllocGlobal> = vec.into();
//     assert_eq!(&*boxed, &[1, 2, 3, 4]);
//     let vec = Vec::<_, AllocGlobal>::from(boxed);
//     assert_eq!(&*vec, &[1, 2, 3, 4]);
//     assert_eq!(vec.capacity(), 4);
// }

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
