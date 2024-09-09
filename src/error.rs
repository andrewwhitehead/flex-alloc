//! Error handling.

use core::alloc::LayoutError;
use core::fmt;

/// An enumeration of error types raised by storage implementations
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StorageError {
    /// A memory allocation failed
    AllocError,
    /// The limit of the current allocation was reached
    CapacityLimit,
    /// The provided layout was not allocatable
    LayoutError(LayoutError),
    /// The requested operation is not supported for this storage
    Unsupported,
}

impl StorageError {
    /// Generic description of this error
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AllocError => "Allocation error",
            Self::CapacityLimit => "Exceeded storage capacity limit",
            Self::LayoutError(_) => "Layout error",
            Self::Unsupported => "Unsupported",
        }
    }

    /// Generate a panic with this error as the reason
    #[cold]
    #[inline(never)]
    pub fn panic(self) -> ! {
        panic!("{}", self.as_str());
    }
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<LayoutError> for StorageError {
    fn from(err: LayoutError) -> Self {
        Self::LayoutError(err)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for StorageError {}

/// An error raised by insertion operations when appropriate storage
/// was not available. Includes the value that was to be inserted.
#[derive(Clone)]
pub struct InsertionError<T> {
    pub(crate) error: StorageError,
    pub(crate) value: T,
}

impl<T> InsertionError<T> {
    pub(crate) fn new(error: StorageError, value: T) -> Self {
        Self { error, value }
    }

    /// Generic description of this error
    pub fn as_str(&self) -> &'static str {
        "Insertion error"
    }

    /// Get a reference to the contained `StorageError`
    pub fn error(&self) -> &StorageError {
        &self.error
    }

    /// Unwrap the inner value of this error
    pub fn into_value(self) -> T {
        self.value
    }

    /// Generate a panic with this error as the reason
    #[cold]
    #[inline(never)]
    pub fn panic(self) -> ! {
        panic!("{}: {}", self.as_str(), self.error.as_str());
    }
}

impl<T> fmt::Debug for InsertionError<T> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InsertionError")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

impl<T> fmt::Display for InsertionError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("{}: {}", self.as_str(), self.error))
    }
}

#[cfg(feature = "std")]
impl<T> std::error::Error for InsertionError<T> {}
