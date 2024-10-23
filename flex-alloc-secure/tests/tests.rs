use flex_alloc_secure::alloc::{
    alloc_pages, dealloc_pages, default_page_size, lock_pages, unlock_pages,
};

#[test]
fn check_default_page_size() {
    let psize = default_page_size();
    assert!(psize >= 4096);
}

#[test]
fn check_alloc_aligned() {
    let psize = default_page_size();
    let len = 24;
    let mut page = alloc_pages(len).expect("error allocating");
    let addr = page.as_ptr().cast::<u8>() as usize;
    assert!(addr % psize == 0);
    assert!(page.len() >= 24);
    unsafe { page.as_mut() }.fill(1u8);
    dealloc_pages(page.as_ptr().cast(), len);
}

#[test]
fn check_lock_aligned() {
    let len = 256;
    let mut page = alloc_pages(len).expect("error allocating");
    lock_pages(page.as_ptr().cast(), page.len()).expect("error locking page");
    unlock_pages(page.as_ptr().cast(), page.len()).expect("error unlocking page");
    unsafe { page.as_mut() }.fill(1u8);
    dealloc_pages(page.as_ptr().cast(), len);
}
