//! Secure `Vec` container types.

use crate::alloc::SecureAlloc;
use flex_alloc::vec::Vec;

/// A [`Vec`] which is backed by a secured allocator and keeps its
/// contents in physical memory. When released, the allocated memory
/// is securely zeroed, including all intermediate buffers produced in
/// resizing the vector.
///
/// This container should be converted into a
/// [`ProtectedBox`] or [`ShieldedBox`] to protect secret data.
///
/// This type does NOT protect against accidental output of
/// contained values using the `Debug` trait.
pub type SecureVec<T> = Vec<T, SecureAlloc>;
