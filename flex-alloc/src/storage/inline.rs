use const_default::ConstDefault;

use super::{array::ArrayStorage, RawBuffer};
use crate::error::StorageError;

/// A marker type used to indicate the inline allocation strategy, which
/// stores all items within the collection handle.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
pub struct Inline<const N: usize>;

/// An inline storage buffer.
#[derive(Debug)]
pub struct InlineBuffer<T, const N: usize> {
    pub(crate) storage: ArrayStorage<T, N>,
    pub(crate) length: usize,
}

impl<T, const N: usize> InlineBuffer<T, N> {
    pub(crate) fn try_for_capacity(capacity: usize, exact: bool) -> Result<Self, StorageError> {
        if (!exact && capacity < N) || capacity == N {
            Ok(Self::DEFAULT)
        } else {
            Err(StorageError::CapacityLimit)
        }
    }
}

impl<T, const N: usize> ConstDefault for InlineBuffer<T, N> {
    const DEFAULT: Self = Self {
        storage: ArrayStorage::DEFAULT,
        length: 0,
    };
}

impl<T, const N: usize> Default for InlineBuffer<T, N> {
    #[inline]
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl<'a, T: 'a, const N: usize> RawBuffer for InlineBuffer<T, N> {
    type RawData = T;

    #[inline]
    fn data_ptr(&self) -> *const T {
        self.storage.0.as_ptr().cast()
    }

    #[inline]
    fn data_ptr_mut(&mut self) -> *mut T {
        self.storage.0.as_mut_ptr().cast()
    }
}
