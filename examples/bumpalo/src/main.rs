use bumpalo::Bump;
use flex_alloc::{alloc::WithAlloc, storage::array_storage, vec::Vec};

fn main() {
    let bump = Bump::new();
    let mut vec: Vec<u32, &Bump> = Vec::new_in(&bump);
    vec.push(83u32);
    assert_eq!(vec, &[83]);

    let mut buf = array_storage::<u32, 10>();
    let mut vec: Vec<u32, _> = Vec::new_in(buf.with_alloc_in(&bump));
    vec.extend(0..10000);
    assert_eq!(vec.len(), 10000);
}
