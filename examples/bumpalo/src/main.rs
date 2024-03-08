use bumpalo::Bump;
use flex_alloc::{
    boxed::Box,
    storage::{array_storage, WithAlloc},
    vec::Vec,
};

fn main() {
    let bump = Bump::new();
    let mut vec: Vec<u32, &Bump> = Vec::new_in(&bump);
    vec.push(83u32);
    assert_eq!(vec, &[83]);

    let boxed: Box<[u32], &Bump> = vec.into_boxed_slice();
    assert_eq!(&*boxed, &[83]);

    let mut buf = array_storage::<u32, 10>();
    let mut vec: Vec<u32, _> = Vec::new_in(buf.with_alloc_in(&bump));
    vec.extend(0..10000);
    assert_eq!(vec.len(), 10000);
}
