//! Secure memory management for flex-alloc.

#![warn(missing_docs)]

#[cfg(all(not(unix), not(windows), not(miri)))]
compile_error!("Only Unix and Windows platforms are currently supported");

pub mod alloc;
pub mod protect;
pub mod random;
pub mod secured;

pub mod boxed {
    //! Secure `Box` types for managing allocated memory.

    use crate::alloc::SecureAlloc;
    use crate::protect::{Protect, ProtectedBox};

    pub use flex_alloc::boxed::Box;

    /// A `Box` which keeps its contents in physical memory and
    /// can be converted into a [`ProtectedBox`].
    pub type SecureBox<T> = Box<T, SecureAlloc>;

    impl<T: ?Sized> Protect for SecureBox<T> {
        type Value = T;

        fn protect(self) -> ProtectedBox<T> {
            ProtectedBox::from(self)
        }
    }
}

pub mod vec {
    //! Secure `Vec` types for managing allocated memory.

    use crate::alloc::SecureAlloc;
    use crate::protect::{Protect, ProtectedBox};

    pub use flex_alloc::{vec, vec::Vec};

    /// A vector which keeps its contents in physical memory and
    /// can be converted into a [`ProtectedBox`] boxed slice.
    pub type SecureVec<T> = Vec<T, SecureAlloc>;

    impl<T> Protect for SecureVec<T> {
        type Value = [T];

        fn protect(self) -> ProtectedBox<[T]> {
            ProtectedBox::from(self)
        }
    }
}
