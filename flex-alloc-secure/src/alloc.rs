//! Support for virtual memory management, including memory protections.

use core::alloc::Layout;
use core::mem::transmute;
use core::ptr::{self, NonNull};
use core::sync::atomic::{AtomicUsize, Ordering};
use core::{fmt, slice};

#[cfg(all(windows, not(miri)))]
use core::mem::MaybeUninit;

use flex_alloc::alloc::{AllocError, Allocator, AllocatorDefault, AllocatorZeroizes};
use flex_alloc::StorageError;
use zeroize::Zeroize;

#[cfg(all(unix, not(miri)))]
use libc::{free, mlock, mprotect, munlock, posix_memalign};

#[cfg(all(windows, not(miri)))]
use windows_sys::Win32::System::{Memory, SystemInformation};

/// Indicator value to help detect uninitialized protected data.
pub const UNINIT_ALLOC_BYTE: u8 = 0xdb;

/// An error which may result from a memory operation such as locking.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MemoryError;

impl fmt::Display for MemoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Memory error")
    }
}

impl std::error::Error for MemoryError {}

/// Enumeration of options for setting the memory protection mode.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProtectionMode {
    /// No read or write access
    NoAccess,
    /// Read-only access
    ReadOnly,
    /// Read-write access
    #[default]
    ReadWrite,
}

impl ProtectionMode {
    #[cfg(all(unix, not(miri)))]
    pub(crate) const fn as_native(self) -> i32 {
        match self {
            Self::NoAccess => libc::PROT_NONE,
            Self::ReadOnly => libc::PROT_READ,
            Self::ReadWrite => libc::PROT_READ | libc::PROT_WRITE,
        }
    }

    #[cfg(all(windows, not(miri)))]
    pub(crate) const fn as_native(self) -> u32 {
        match self {
            Self::NoAccess => windows_sys::Win32::System::Memory::PAGE_NOACCESS,
            Self::ReadOnly => windows_sys::Win32::System::Memory::PAGE_READONLY,
            Self::ReadWrite => windows_sys::Win32::System::Memory::PAGE_READWRITE,
        }
    }
}

/// Fetch the system-specific page size.
pub fn default_page_size() -> usize {
    static CACHE: AtomicUsize = AtomicUsize::new(0);

    let mut size = CACHE.load(Ordering::Relaxed);

    if size == 0 {
        #[cfg(miri)]
        {
            size = 4096;
        }
        #[cfg(all(target_os = "macos", not(miri)))]
        {
            size = unsafe { libc::vm_page_size };
        }
        #[cfg(all(unix, not(target_os = "macos"), not(miri)))]
        {
            size = unsafe { libc::sysconf(libc::_SC_PAGE_SIZE) } as usize;
        }
        #[cfg(all(windows, not(miri)))]
        {
            let mut sysinfo = MaybeUninit::<SystemInformation::SYSTEM_INFO>::uninit();
            unsafe { SystemInformation::GetSystemInfo(sysinfo.as_mut_ptr()) };
            size = unsafe { sysinfo.assume_init_ref() }.dwPageSize as usize;
        }

        debug_assert_ne!(size, 0);
        // inputs to posix_memalign must be a multiple of the pointer size
        debug_assert_eq!(size % size_of::<*const ()>(), 0);

        CACHE.store(size, Ordering::Relaxed);
    }

    size
}

/// Allocate a page-aligned buffer. The alignment will be rounded up to a multiple of
/// the platform pointer size if necessary.
pub fn alloc_pages(len: usize) -> Result<NonNull<[u8]>, AllocError> {
    let page_size = default_page_size();
    let alloc_len = page_rounded_length(len, page_size);

    #[cfg(miri)]
    {
        let addr =
            unsafe { std::alloc::alloc(Layout::from_size_align_unchecked(alloc_len, page_size)) };
        let range = ptr::slice_from_raw_parts_mut(addr, alloc_len);
        NonNull::new(range).ok_or_else(|| AllocError)
    }

    #[cfg(all(unix, not(miri)))]
    {
        let mut addr = ptr::null_mut();
        let ret = unsafe { posix_memalign(&mut addr, page_size, alloc_len) };
        if ret == 0 {
            let range = ptr::slice_from_raw_parts_mut(addr.cast(), alloc_len);
            Ok(NonNull::new(range).expect("null allocation result"))
        } else {
            Err(AllocError)
        }
    }

    #[cfg(all(windows, not(miri)))]
    {
        let addr = unsafe {
            Memory::VirtualAlloc(
                ptr::null_mut(),
                alloc_len,
                Memory::MEM_COMMIT | Memory::MEM_RESERVE,
                Memory::PAGE_READWRITE,
            )
        };
        let range = ptr::slice_from_raw_parts_mut(addr.cast(), alloc_len);
        NonNull::new(range).ok_or_else(|| AllocError)
    }
}

/// Release a buffer allocated by `alloc_aligned`.
pub fn dealloc_pages(addr: *mut u8, len: usize) {
    #[cfg(miri)]
    {
        let page_size = default_page_size();
        let alloc_len = page_rounded_length(len, page_size);
        unsafe {
            std::alloc::dealloc(
                addr,
                Layout::from_size_align_unchecked(alloc_len, page_size),
            )
        };
        return;
    }

    #[cfg(all(unix, not(miri)))]
    {
        let _ = len;
        unsafe { free(addr.cast()) };
    }

    #[cfg(all(windows, not(miri)))]
    {
        let _ = len;
        unsafe { Memory::VirtualFree(addr.cast(), 0, Memory::MEM_RELEASE) };
    }
}

/// Prevent swapping for the given memory range.
/// On supported platforms, avoid including the memory in core dumps.
pub fn lock_pages(addr: *mut u8, len: usize) -> Result<(), MemoryError> {
    #[cfg(miri)]
    {
        _ = (addr, len);
        Ok(())
    }
    #[cfg(all(unix, not(miri)))]
    {
        #[cfg(target_os = "linux")]
        madvise(addr, len, libc::MADV_DONTDUMP)?;
        #[cfg(any(target_os = "freebsd", target_os = "dragonfly"))]
        madvise(addr, len, libc::MADV_NOCORE)?;

        let res = unsafe { mlock(addr.cast(), len) };
        if res == 0 {
            Ok(())
        } else {
            Err(MemoryError)
        }
    }
    #[cfg(all(windows, not(miri)))]
    {
        let res = unsafe { Memory::VirtualLock(addr.cast(), len) };
        if res != 0 {
            Ok(())
        } else {
            Err(MemoryError)
        }
    }
}

/// Resume normal swapping behavior for the given memory range.
pub fn unlock_pages(addr: *mut u8, len: usize) -> Result<(), MemoryError> {
    #[cfg(miri)]
    {
        _ = (addr, len);
        Ok(())
    }
    #[cfg(all(unix, not(miri)))]
    {
        #[cfg(target_os = "linux")]
        madvise(addr, len, libc::MADV_DODUMP)?;
        #[cfg(any(target_os = "freebsd", target_os = "dragonfly"))]
        madvise(addr, len, libc::MADV_CORE)?;

        let res = unsafe { munlock(addr.cast(), len) };
        if res == 0 {
            Ok(())
        } else {
            Err(MemoryError)
        }
    }
    #[cfg(all(windows, not(miri)))]
    {
        let res = unsafe { Memory::VirtualUnlock(addr.cast(), len) };
        if res != 0 {
            Ok(())
        } else {
            Err(MemoryError)
        }
    }
}

/// Adjust the protection mode for a given memory range.
pub fn set_page_protection(
    addr: *mut u8,
    len: usize,
    mode: ProtectionMode,
) -> Result<(), MemoryError> {
    #[cfg(miri)]
    {
        _ = (addr, len, mode);
        Ok(())
    }
    #[cfg(all(unix, not(miri)))]
    {
        let res = unsafe { mprotect(addr.cast(), len, mode.as_native()) };
        if res == 0 {
            Ok(())
        } else {
            Err(MemoryError)
        }
    }
    #[cfg(all(windows, not(miri)))]
    {
        let mut prev_mode = MaybeUninit::<u32>::uninit();
        let res = unsafe {
            Memory::VirtualProtect(addr.cast(), len, mode.as_native(), prev_mode.as_mut_ptr())
        };
        if res != 0 {
            Ok(())
        } else {
            Err(MemoryError)
        }
    }
}

#[cfg(unix)]
#[allow(unused)]
#[inline]
fn madvise(addr: *mut u8, len: usize, advice: i32) -> Result<(), MemoryError> {
    {
        let res = unsafe { libc::madvise(addr.cast(), len, advice) };
        if res == 0 {
            Ok(())
        } else {
            Err(MemoryError)
        }
    }
}

/// Round up a length of bytes to a multiple of the page size.
#[inline(always)]
pub fn page_rounded_length(len: usize, page_size: usize) -> usize {
    len + ((page_size - (len & (page_size - 1))) % page_size)
}

/// An allocator which obtains a discrete number of virtual memory pages.
///
/// The virutal memory pages are flagged using `mlock` (`VirtualLock` on
/// Windows) in order to restrict them to physical memory. When the
/// allocation is released, the allocated memory is securely zeroed.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct SecureAlloc;

impl SecureAlloc {
    pub(crate) fn set_page_protection(
        &self,
        ptr: *mut u8,
        len: usize,
        mode: ProtectionMode,
    ) -> Result<(), StorageError> {
        if len != 0 {
            let alloc_len = page_rounded_length(len, default_page_size());
            set_page_protection(ptr, alloc_len, mode).map_err(|_| StorageError::ProtectionError)
        } else {
            Ok(())
        }
    }
}

unsafe impl Allocator for SecureAlloc {
    #[inline]
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        debug_assert!(
            layout.align() <= default_page_size(),
            "alignment cannot exceed page size"
        );
        let layout_len = layout.size();
        if layout_len == 0 {
            // FIXME: use Layout::dangling when stabilized
            // SAFETY: layout alignments are guaranteed to be non-zero.
            #[allow(clippy::useless_transmute)]
            let dangling = unsafe { NonNull::new_unchecked(transmute(layout.align())) };
            Ok(NonNull::slice_from_raw_parts(dangling, 0))
        } else {
            let alloc = alloc_pages(layout_len).map_err(|_| AllocError)?;
            let alloc_len = alloc.len();

            // Initialize with uninitialized indicator value
            // SAFETY: the allocated pointer is guaranteed to be valid and have a length
            // equal to `alloc_len`.
            unsafe { ptr::write_bytes(alloc.as_ptr().cast::<u8>(), UNINIT_ALLOC_BYTE, alloc_len) };

            // Keep data page(s) out of swap
            lock_pages(alloc.as_ptr().cast(), alloc_len).map_err(|_| AllocError)?;

            Ok(alloc)
        }
    }

    #[inline]
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        let len = layout.size();
        if len > 0 {
            let alloc_len = page_rounded_length(len, default_page_size());

            // Zero protected data
            let mem = unsafe { slice::from_raw_parts_mut(ptr.as_ptr(), alloc_len) };
            mem.zeroize();

            // Restore normal swapping behavior
            unlock_pages(ptr.as_ptr().cast(), alloc_len).ok();

            // Free the memory
            dealloc_pages(ptr.as_ptr(), alloc_len);
        }
    }
}

impl AllocatorDefault for SecureAlloc {
    const DEFAULT: Self = Self;
}

impl AllocatorZeroizes for SecureAlloc {}

#[cfg(test)]
mod tests {
    use core::alloc::Layout;
    use flex_alloc::alloc::Allocator;

    use crate::{alloc::UNINIT_ALLOC_BYTE, vec::SecureVec};

    use super::SecureAlloc;

    #[test]
    fn check_extra_capacity() {
        let vec = SecureVec::<usize>::with_capacity(1);
        // We always allocate pages, so there should be plenty of room for more values.
        assert!(vec.capacity() > 1);
    }

    #[test]
    fn check_uninit() {
        let layout = Layout::new::<usize>();
        let buf = SecureAlloc.allocate(layout).expect("allocation error");
        #[allow(clippy::len_zero)]
        {
            assert!(buf.len() != 0 && unsafe { buf.as_ref() }[..4] == [UNINIT_ALLOC_BYTE; 4]);
        }
        unsafe { SecureAlloc.deallocate(buf.cast(), layout) };
    }
}
