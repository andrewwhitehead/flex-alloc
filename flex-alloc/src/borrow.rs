//! Support for flexibility over owned or borrowed collections.

use core::{
    borrow::Borrow,
    fmt::{self, Debug, Display},
    ops::Deref,
};

use const_default::ConstDefault;

use crate::alloc::{AllocateIn, Allocator};
use crate::error::StorageError;

/// The owned type for a collection which may be owned or borrowed.
pub type Owned<B, A> = <B as ToOwnedIn<A>>::Owned;

/// Support conversion from borrowed types to owned ones associated with an allocator.
pub trait ToOwnedIn<A: Allocator> {
    /// The owned representation of this type.
    type Owned: Borrow<Self>;

    /// Create an owned copy of this instance in a given allocation target.
    fn to_owned_in<I>(&self, alloc_in: I) -> Self::Owned
    where
        I: AllocateIn<Alloc = A>,
    {
        match self.try_to_owned_in(alloc_in) {
            Ok(inst) => inst,
            Err(err) => err.panic(),
        }
    }

    /// To to create an owned copy of this instance in a given allocation target.
    fn try_to_owned_in<I>(&self, alloc_in: I) -> Result<Self::Owned, StorageError>
    where
        I: AllocateIn<Alloc = A>;
}

impl<T: Clone + 'static, A: Allocator> ToOwnedIn<A> for T {
    type Owned = T;

    fn try_to_owned_in<I>(&self, _alloc_in: I) -> Result<Self::Owned, StorageError>
    where
        I: AllocateIn<Alloc = A>,
    {
        Ok(self.clone())
    }
}

/// Representation of either an owned or borrowed instance of a type.
pub enum Cow<'b, T: ToOwnedIn<A> + ?Sized, A: Allocator> {
    /// The borrowed variant, limited by a lifetime.
    Borrowed(&'b T),

    /// The owned variant.
    Owned(Owned<T, A>),
}

impl<'b, T: ToOwnedIn<A> + ?Sized, A: Allocator> Cow<'b, T, A> {
    /// Determine if this instance is borrowed.
    #[inline]
    pub fn is_borrowed(&self) -> bool {
        matches!(self, Self::Borrowed(_))
    }

    /// Determine if this instance is owned.
    #[inline]
    pub fn is_owned(&self) -> bool {
        matches!(self, Self::Owned(_))
    }

    /// If necessary, convert `self` into an owned instance. Return a mutable reference
    /// to the owned instance.
    #[inline]
    pub fn to_mut(&mut self) -> &mut Owned<T, A>
    where
        A: Default + Allocator,
    {
        self.to_mut_in(A::default())
    }

    /// If necessary, convert `self` into an owned instance given an allocation target.
    /// Return a mutable reference to the owned instance.
    pub fn to_mut_in<I>(&mut self, alloc_in: I) -> &mut Owned<T, A>
    where
        I: AllocateIn<Alloc = A>,
    {
        match *self {
            Self::Borrowed(borrowed) => {
                *self = Self::Owned(borrowed.to_owned_in(alloc_in));
                let Self::Owned(owned) = self else {
                    unreachable!()
                };
                owned
            }
            Self::Owned(ref mut owned) => owned,
        }
    }

    /// If necessary, convert `self` into an owned instance. Unwrap and return the
    /// owned instance.
    pub fn into_owned(self) -> Owned<T, A>
    where
        A: Default + Allocator,
    {
        match self {
            Self::Borrowed(borrowed) => borrowed.to_owned_in(A::default()),
            Self::Owned(owned) => owned,
        }
    }

    /// If necessary, convert `self` into an owned instance given an allocation target.
    /// Unwrap and return the owned instance.
    pub fn into_owned_in<I>(self, alloc_in: I) -> Owned<T, A>
    where
        I: AllocateIn<Alloc = A>,
    {
        match self {
            Self::Borrowed(borrowed) => borrowed.to_owned_in(alloc_in),
            Self::Owned(owned) => owned,
        }
    }

    /// If necessary, try to convert `self` into an owned instance.
    /// Unwrap and return the owned instance or a storage error.
    pub fn try_into_owned(self) -> Result<Owned<T, A>, StorageError>
    where
        A: Default + Allocator,
    {
        match self {
            Self::Borrowed(borrowed) => borrowed.try_to_owned_in(A::default()),
            Self::Owned(owned) => Ok(owned),
        }
    }

    /// If necessary, try to convert `self` into an owned instance given an allocation
    /// target. Unwrap and return the owned instance or a storage error.
    pub fn try_into_owned_in<I>(self, storage: I) -> Result<Owned<T, A>, StorageError>
    where
        I: AllocateIn<Alloc = A>,
    {
        match self {
            Self::Borrowed(borrowed) => borrowed.try_to_owned_in(storage),
            Self::Owned(owned) => Ok(owned),
        }
    }
}

impl<T: ToOwnedIn<A> + ?Sized, A: Allocator> AsRef<T> for Cow<'_, T, A> {
    fn as_ref(&self) -> &T {
        self
    }
}

impl<T: ToOwnedIn<A> + ?Sized, A: Allocator> Borrow<T> for Cow<'_, T, A> {
    fn borrow(&self) -> &T {
        self
    }
}

impl<T: ToOwnedIn<A> + ?Sized, A: Allocator> Clone for Cow<'_, T, A>
where
    Owned<T, A>: Clone,
{
    fn clone(&self) -> Self {
        match self {
            Self::Borrowed(b) => Self::Borrowed(*b),
            Self::Owned(o) => Self::Owned(o.clone()),
        }
    }

    fn clone_from(&mut self, source: &Self) {
        match (self, source) {
            (&mut Self::Owned(ref mut dest), Self::Owned(ref o)) => dest.clone_from(o),
            (t, s) => *t = s.clone(),
        }
    }
}

impl<T, A: Allocator> ConstDefault for Cow<'_, T, A>
where
    T: ToOwnedIn<A> + ?Sized,
    T::Owned: ConstDefault,
{
    const DEFAULT: Self = Self::Owned(T::Owned::DEFAULT);
}

impl<T, A: Allocator> Debug for Cow<'_, T, A>
where
    T: ToOwnedIn<A> + Debug + ?Sized,
    Owned<T, A>: Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Borrowed(b) => Debug::fmt(b, f),
            Self::Owned(ref o) => Debug::fmt(o, f),
        }
    }
}

impl<T, A: Allocator> Display for Cow<'_, T, A>
where
    T: ToOwnedIn<A> + Display + ?Sized,
    Owned<T, A>: Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Borrowed(ref b) => Display::fmt(b, f),
            Self::Owned(ref o) => Display::fmt(o, f),
        }
    }
}

impl<T: ToOwnedIn<A> + ?Sized, A: Allocator> Deref for Cow<'_, T, A> {
    type Target = T;

    fn deref(&self) -> &T {
        match *self {
            Self::Borrowed(borrowed) => borrowed,
            Self::Owned(ref owned) => owned.borrow(),
        }
    }
}

impl<T: ToOwnedIn<A> + ?Sized, A: Allocator> Default for Cow<'_, T, A>
where
    Owned<T, A>: Default,
{
    #[inline]
    fn default() -> Self {
        Self::Owned(Default::default())
    }
}

impl<'b, T: ToOwnedIn<A> + ?Sized, A: Allocator> From<&'b T> for Cow<'b, T, A> {
    #[inline]
    fn from(borrow: &'b T) -> Self {
        Self::Borrowed(borrow)
    }
}

impl<'b, T: ToOwnedIn<A> + ?Sized, A: Allocator> From<&'b mut T> for Cow<'b, T, A> {
    #[inline]
    fn from(borrow: &'b mut T) -> Self {
        Self::Borrowed(borrow)
    }
}

impl<'a, 'b, T: ToOwnedIn<A> + ?Sized, A: Allocator, U: ToOwnedIn<B> + ?Sized, B: Allocator>
    PartialEq<Cow<'b, U, B>> for Cow<'a, T, A>
where
    T: PartialEq<U>,
{
    #[inline]
    fn eq(&self, other: &Cow<'b, U, B>) -> bool {
        self.deref().eq(other.deref())
    }
}

impl<'a, T: ToOwnedIn<A> + Eq + ?Sized, A: Allocator> Eq for Cow<'a, T, A> {}
