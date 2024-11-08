use flex_alloc_secure::{
    alloc::UNINIT_ALLOC_BYTE,
    boxed::{ProtectedBox, ShieldedBox},
    vec::SecureVec,
    ExposeProtected, ProtectedInit, ProtectedInitSlice,
};

const UNINIT_USIZE: usize = usize::from_ne_bytes([UNINIT_ALLOC_BYTE; size_of::<usize>()]);

#[test]
fn protected_box() {
    let prot = ProtectedBox::init_with(|| 99usize);
    prot.expose_read(|r| {
        assert_eq!(r.as_ref(), &99usize);
    });

    let mut prot = ProtectedBox::init_slice(3, |mut s| {
        s.copy_from_slice(&[10usize, 9, 8]);
    });
    prot.expose_read(|r| {
        assert_eq!(r.as_ref(), &[10, 9, 8]);
    });
    prot.expose_write(|mut w| {
        w[0] = 11;
    });
    prot.expose_read(|r| {
        assert_eq!(r.as_ref(), &[11, 9, 8]);
    });
}

#[test]
fn protected_box_send() {
    use std::sync::OnceLock;
    static ONCE: OnceLock<ProtectedBox<usize>> = OnceLock::new();

    let prot = ProtectedBox::init_with(|| 99usize);
    std::thread::spawn(move || {
        prot.expose_read(|r| {
            assert_eq!(r.as_ref(), &99usize);
        });
    });

    ONCE.get_or_init(|| ProtectedBox::init_with(|| 98usize))
        .expose_read(|r| {
            assert_eq!(r.as_ref(), &98usize);
        });
}

#[test]
fn shielded_box() {
    let prot = ShieldedBox::init_with(|| 99usize);
    prot.expose_read(|r| {
        assert_eq!(r.as_ref(), &99usize);
    });

    let mut prot = ShieldedBox::init_slice(3, |mut s| {
        s.copy_from_slice(&[10usize, 9, 8]);
    });
    prot.expose_read(|r| {
        assert_eq!(r.as_ref(), &[10, 9, 8]);
    });
    prot.expose_write(|mut w| {
        w[0] = 11;
    });
    prot.expose_read(|r| {
        assert_eq!(r.as_ref(), &[11, 9, 8]);
    });
}

#[test]
fn protected_vec() {
    let mut sec = SecureVec::<usize>::with_capacity(4);
    assert_eq!(
        unsafe { sec.spare_capacity_mut()[0].assume_init() },
        UNINIT_USIZE
    );
    sec.extend_from_slice(&[5, 4, 3]);
    let prot = ProtectedBox::<[usize]>::from(sec);
    prot.expose_read(|r| {
        assert_eq!(r.as_ref(), &[5, 4, 3]);
    });
}
