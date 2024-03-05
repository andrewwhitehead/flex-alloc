#![cfg(all(feature = "alloc", feature = "zeroize"))]

use core::alloc::Layout;
use core::cell::RefCell;
use core::ptr::NonNull;
use core::slice;

use flex_alloc::{
    boxed::Box as FlexBox,
    storage::{array_storage, byte_storage, Global, RawAlloc, WithAlloc, ZeroizingAlloc},
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
    let b = FlexBox::new_in(99u32, &alloc);
    drop(b);
    let log = alloc.released.borrow().clone();
    assert_eq!(log, &[99u32.to_ne_bytes()]);
}

#[test]
fn box_zeroize() {
    // test zeroizing alloc
    let alloc = TestAlloc::new(Global);
    let b = FlexBox::new_in(99u32, ZeroizingAlloc(&alloc));
    drop(b);
    let log = alloc.released.borrow().clone();
    assert_eq!(log, &[&[0, 0, 0, 0]]);
}

#[test]
fn box_zeroize_spill() {
    // test zeroizing alloc with spill
    let mut buf = zeroize::Zeroizing::new(byte_storage::<2>());
    let alloc = TestAlloc::new(Global);
    let b = FlexBox::new_in(99u32, buf.with_alloc_in(&alloc));
    drop(b);
    let log = alloc.released.borrow().clone();
    assert_eq!(log, &[&[0, 0, 0, 0]]);
}

#[test]
fn vec_zeroize() {
    let mut v = FlexVec::<usize, _>::new_in(ZeroizingAlloc::<Global>::default());
    v.extend([1, 2, 3]);

    let mut z = zeroize::Zeroizing::new(array_storage::<usize, 1>());
    let mut v = FlexVec::<usize, _>::new_in(&mut *z);
    v.push(1);

    let mut z = zeroize::Zeroizing::new(byte_storage::<100>());
    let mut v = FlexVec::<usize, _>::new_in(&mut *z);
    v.push(1);

    let mut z = zeroize::Zeroizing::new(array_storage::<usize, 1>());
    let mut v = FlexVec::<usize, _>::new_in(z.with_alloc());
    v.extend([1, 2, 3]);

    let mut z = zeroize::Zeroizing::new(byte_storage::<10>());
    let mut v = FlexVec::<usize, _>::new_in(z.with_alloc());
    v.extend([1, 2, 3]);

    // test type alias
    let mut v = ZeroizingVec::new();
    v.extend([1, 2, 3]);
}
