use core::mem::size_of;

use flex_alloc_secure::{
    alloc::UNINIT_ALLOC_BYTE,
    boxed::SecureBox,
    protect::{Protect, ProtectedBox, ReadProtected, WriteProtected},
    vec::SecureVec,
};

const UNINIT_USIZE: usize = usize::from_ne_bytes([UNINIT_ALLOC_BYTE; size_of::<usize>()]);

#[test]
fn protected_box() {
    let sec = SecureBox::<usize>::new_uninit();
    assert_eq!(unsafe { sec.as_ref().assume_init() }, UNINIT_USIZE);
    let prot = sec.write(99usize).protect();
    assert_eq!(prot.read_protected().as_ref(), &99usize);

    let mut prot = ProtectedBox::from(&[10, 9, 8]);
    assert_eq!(prot.read_protected().as_ref(), &[10, 9, 8]);
    prot.write_protected()[0] = 11;
    assert_eq!(prot.read_protected().as_ref(), &[11, 9, 8]);

    let mut prot = ProtectedBox::from(&99);
    assert_eq!(prot.read_protected().as_ref(), &99);
    *prot.write_protected() = 100;
    assert_eq!(prot.read_protected().as_ref(), &100);
}

#[test]
fn protected_vec() {
    let mut sec = SecureVec::<usize>::with_capacity(4);
    assert_eq!(
        unsafe { sec.spare_capacity_mut()[0].assume_init() },
        UNINIT_USIZE
    );
    sec.extend_from_slice(&[5, 4, 3]);
    let mut prot = sec.protect();
    assert_eq!(prot.read_protected().as_ref(), &[5, 4, 3]);
    prot.write_protected()[0] = 6;
    assert_eq!(prot.read_protected().as_ref(), &[6, 4, 3]);
}
