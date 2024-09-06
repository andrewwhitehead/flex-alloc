#![cfg(all(feature = "alloc", feature = "zeroize"))]

use core::alloc::Layout;
use core::cell::RefCell;
use core::ptr::NonNull;
use core::slice;

use flex_alloc::{
    storage::{array_storage, byte_storage, Global, RawAlloc, WithAlloc, ZeroizingAlloc},
    vec,
    vec::{Vec as FlexVec, ZeroizingVec},
};

#[derive(Debug)]
struct TestAlloc<A: RawAlloc> {
    alloc: A,
    released: RefCell<Vec<Vec<u8>>>,
}

impl<A: RawAlloc> TestAlloc<A> {
    fn new(alloc: A) -> Self {
        Self {
            alloc,
            released: RefCell::new(Vec::new()),
        }
    }
}

impl<A: RawAlloc> RawAlloc for &TestAlloc<A> {
    fn try_alloc(&self, layout: Layout) -> Result<NonNull<[u8]>, flex_alloc::StorageError> {
        self.alloc.try_alloc(layout)
    }

    unsafe fn try_resize(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, flex_alloc::StorageError> {
        self.alloc.try_resize(ptr, old_layout, new_layout)
    }

    unsafe fn release(&self, ptr: NonNull<u8>, layout: Layout) {
        let cp = Vec::from(unsafe { slice::from_raw_parts(ptr.as_ptr(), layout.size()) });
        self.released.borrow_mut().push(cp);
        self.alloc.release(ptr, layout)
    }
}

#[test]
fn test_alloc_log() {
    // check functioning of alloc log
    let alloc = TestAlloc::new(Global);
    let b = vec![in &alloc; 99usize];
    drop(b);
    let log = alloc.released.borrow().clone();
    assert_eq!(log.len(), 1);
    assert!(log[0].starts_with(&99u32.to_ne_bytes()));
}

#[test]
fn vec_zeroizing_alloc_global() {
    let mut v = FlexVec::<usize, _>::new_in(ZeroizingAlloc::<Global>::default());
    v.extend([1, 2, 3]);
}

#[test]
fn vec_zeroizing_alloc_global_verify() {
    let alloc = TestAlloc::new(Global);
    let mut v = FlexVec::<usize, _>::new_in(ZeroizingAlloc(&alloc));
    v.push(1usize);
    drop(v);
    let log = alloc.released.borrow().clone();
    assert_eq!(log.len(), 1);
    assert!(log[0].iter().all(|i| *i == 0));
}

#[test]
fn vec_zeroizing_array_storage() {
    let mut z = zeroize::Zeroizing::new(array_storage::<usize, 1>());
    let mut v = FlexVec::<usize, _>::new_in(&mut *z);
    v.push(1);
}

#[test]
fn vec_zeroizing_byte_storage() {
    let mut z = zeroize::Zeroizing::new(byte_storage::<100>());
    let mut v = FlexVec::<usize, _>::new_in(&mut *z);
    v.push(1);
}

#[test]
fn vec_zeroizing_array_storage_spill() {
    let mut z = zeroize::Zeroizing::new(array_storage::<usize, 1>());
    let mut v = FlexVec::<usize, _>::new_in(z.with_alloc());
    v.extend([1, 2, 3]);
}

#[test]
fn vec_zeroizing_array_storage_spill_verify() {
    let alloc = TestAlloc::new(Global);
    let mut z = zeroize::Zeroizing::new(array_storage::<usize, 1>());
    let mut v = FlexVec::<usize, _>::new_in(z.with_alloc_in(&alloc));
    v.extend([1, 2, 3]);
    drop(v);
    let log = alloc.released.borrow().clone();
    assert_eq!(log.len(), 1);
    assert!(log[0].iter().all(|i| *i == 0));
}

#[test]
fn vec_zeroizing_byte_storage_spill() {
    let mut z = zeroize::Zeroizing::new(byte_storage::<10>());
    let mut v = FlexVec::<usize, _>::new_in(z.with_alloc());
    v.extend([1, 2, 3]);
}

#[test]
fn vec_zeroizing_byte_storage_spill_verify() {
    let alloc = TestAlloc::new(Global);
    let mut z = zeroize::Zeroizing::new(byte_storage::<10>());
    let mut v = FlexVec::<usize, _>::new_in(z.with_alloc_in(&alloc));
    v.extend([1, 2, 3]);
    drop(v);
    let log = alloc.released.borrow().clone();
    assert_eq!(log.len(), 1);
    assert!(log[0].iter().all(|i| *i == 0));
}

#[test]
fn vec_zeroizingvec_alias() {
    let mut v = ZeroizingVec::new();
    v.extend([1, 2, 3]);
}
