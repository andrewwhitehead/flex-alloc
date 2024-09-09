//! Types used to specify indexes, ranges, lengths, and capacities of collections.

use core::fmt::{Debug, Display};

use crate::storage::utils::min_non_zero_cap;

/// Types which may be used to index and define the length and capacity of collections.
pub trait Index:
    Copy
    + Clone
    + Debug
    + Display
    + Into<usize>
    + PartialEq
    + Eq
    + PartialOrd
    + Ord
    + Send
    + Sync
    + Sized
    + 'static
{
    /// The zero value
    const ZERO: Self;
    /// The maximum representable value of this type as a `usize`
    const MAX_USIZE: usize;

    /// Create an instance of this type from a usize, panicking if
    /// the bounds are exceeded
    fn from_usize(val: usize) -> Self;

    /// Try to create an instance of this type from a usize
    fn try_from_usize(val: usize) -> Option<Self>;

    /// Convert this instance into a `usize`
    #[inline]
    fn to_usize(self) -> usize {
        self.into()
    }

    /// Add a `usize` without exceeding the bounds of this type
    fn saturating_add(self, val: usize) -> Self;

    /// Subtract a `usize` without exceeding the bounds of this type
    fn saturating_sub(self, val: usize) -> Self;

    /// Multiply by a `usize` without exceeding the bounds of this type
    fn saturating_mul(self, val: usize) -> Self;
}

impl Index for u8 {
    const ZERO: Self = 0u8;
    const MAX_USIZE: usize = u8::MAX as usize;

    #[inline]
    fn from_usize(val: usize) -> Self {
        val as Self
    }

    #[inline]
    fn try_from_usize(val: usize) -> Option<Self> {
        val.try_into().ok()
    }

    #[inline]
    fn to_usize(self) -> usize {
        self as usize
    }

    #[inline]
    fn saturating_add(self, val: usize) -> Self {
        // self.to_usize().saturating_add(val).min(Self::MAX_USIZE) as Self
        (self as usize + val) as Self
        // self + (val as Self)
    }

    fn saturating_sub(self, val: usize) -> Self {
        self.to_usize().saturating_sub(val) as Self
    }

    fn saturating_mul(self, val: usize) -> Self {
        self.to_usize().saturating_mul(val).min(Self::MAX_USIZE) as Self
    }
}

impl Index for usize {
    const ZERO: Self = 0usize;
    const MAX_USIZE: usize = usize::MAX;

    #[inline]
    fn from_usize(val: usize) -> Self {
        val
    }

    #[inline]
    fn try_from_usize(val: usize) -> Option<Self> {
        Some(val)
    }

    fn saturating_add(self, val: usize) -> Self {
        self.saturating_add(val)
    }

    fn saturating_sub(self, val: usize) -> Self {
        self.saturating_sub(val)
    }

    fn saturating_mul(self, val: usize) -> Self {
        self.saturating_mul(val)
    }
}

/// Growth behavior for collections which have exceeded their available storage
pub trait Grow: Debug {
    /// Calculate the next capacity to request from the allocator
    fn next_capacity<T, I: Index>(prev: I, minimum: I) -> I;
}

/// Growth behavior which never requests extra capacity
#[derive(Debug, Copy, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct GrowExact;

impl Grow for GrowExact {
    #[inline]
    fn next_capacity<T, I: Index>(_prev: I, minimum: I) -> I {
        minimum
    }
}

/// Growth behavior which consistently doubles in size
#[derive(Debug, Copy, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct GrowDoubling;

impl Grow for GrowDoubling {
    #[inline]
    fn next_capacity<T, I: Index>(prev: I, minimum: I) -> I {
        let preferred = if prev == I::ZERO {
            I::from_usize(min_non_zero_cap::<T>())
        } else {
            prev.saturating_mul(2)
        };
        preferred.max(minimum)
    }
}
