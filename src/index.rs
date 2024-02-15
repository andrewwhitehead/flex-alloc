use core::fmt::{Debug, Display};

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
    const ZERO: Self;
    const MAX_USIZE: usize;

    fn from_usize(val: usize) -> Self;

    fn try_from_usize(val: usize) -> Option<Self>;

    #[inline]
    fn to_usize(self) -> usize {
        self.into()
    }

    fn saturating_add(self, val: usize) -> Self;

    fn saturating_sub(self, val: usize) -> Self;

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
