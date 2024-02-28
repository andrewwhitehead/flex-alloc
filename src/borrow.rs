use core::{
    borrow::Borrow,
    fmt::{self, Debug, Display},
    ops::Deref,
};

use crate::storage::{RawAlloc, RawAllocIn};
use crate::{error::StorageError, storage::RawAllocNew};

pub type Owned<B, A> = <B as ToOwnedIn<A>>::Owned;

// #[cfg(feature = "alloc")]
// pub type AllocCow<'b, T> = Cow<'b, T, Global>;

// pub type FlexCow<'b, T> = Cow<'b, T, Flex<'b>>;

// pub type RefCow<'b, T> = Cow<'b, T, Fixed<'b>>;

pub trait ToOwnedIn<A: RawAlloc> {
    type Owned: Borrow<Self>;

    fn to_owned_in<I>(&self, alloc_in: I) -> Self::Owned
    where
        I: RawAllocIn<RawAlloc = A>,
    {
        match self.try_to_owned_in(alloc_in) {
            Ok(inst) => inst,
            Err(err) => err.panic(),
        }
    }

    fn try_to_owned_in<I>(&self, alloc_in: I) -> Result<Self::Owned, StorageError>
    where
        I: RawAllocIn<RawAlloc = A>;

    // fn clone_into(&self, target: &mut Self::Owned) {
    //     *target = self.to_owned();
    // }
}

impl<T: Clone + 'static, A: RawAlloc> ToOwnedIn<A> for T {
    type Owned = T;

    fn try_to_owned_in<I>(&self, _alloc_in: I) -> Result<Self::Owned, StorageError>
    where
        I: RawAllocIn<RawAlloc = A>,
    {
        Ok(self.clone())
    }
}

pub enum Cow<'b, T: ToOwnedIn<A> + ?Sized, A: RawAlloc> {
    Borrowed(&'b T),
    Owned(Owned<T, A>),
}

impl<'b, T: ToOwnedIn<A> + ?Sized, A: RawAlloc> Cow<'b, T, A> {
    #[inline]
    pub fn is_borrowed(&self) -> bool {
        matches!(self, Self::Borrowed(_))
    }

    #[inline]
    pub fn is_owned(&self) -> bool {
        matches!(self, Self::Owned(_))
    }

    pub fn to_mut(&mut self) -> &mut Owned<T, A>
    where
        A: RawAllocNew,
    {
        match *self {
            Self::Borrowed(borrowed) => {
                *self = Self::Owned(borrowed.to_owned_in(A::NEW));
                let Self::Owned(owned) = self else {
                    unreachable!()
                };
                owned
            }
            Self::Owned(ref mut owned) => owned,
        }
    }

    // FIXME to_mut_in?

    pub fn into_owned(self) -> Owned<T, A>
    where
        A: RawAllocNew,
    {
        match self {
            Self::Borrowed(borrowed) => borrowed.to_owned_in(A::NEW),
            Self::Owned(owned) => owned,
        }
    }

    pub fn into_owned_in<I>(self, alloc_in: I) -> Owned<T, A>
    where
        I: RawAllocIn<RawAlloc = A>,
    {
        match self {
            Self::Borrowed(borrowed) => borrowed.to_owned_in(alloc_in),
            Self::Owned(owned) => owned,
        }
    }

    pub fn try_into_owned(self) -> Result<Owned<T, A>, StorageError>
    where
        A: RawAllocNew,
    {
        match self {
            Self::Borrowed(borrowed) => borrowed.try_to_owned_in(A::NEW),
            Self::Owned(owned) => Ok(owned),
        }
    }

    pub fn try_into_owned_in<I>(self, storage: I) -> Result<Owned<T, A>, StorageError>
    where
        I: RawAllocIn<RawAlloc = A>,
    {
        match self {
            Self::Borrowed(borrowed) => borrowed.try_to_owned_in(storage),
            Self::Owned(owned) => Ok(owned),
        }
    }
}

impl<T: ToOwnedIn<A> + ?Sized, A: RawAlloc> AsRef<T> for Cow<'_, T, A> {
    fn as_ref(&self) -> &T {
        self
    }
}

impl<T: ToOwnedIn<A> + ?Sized, A: RawAlloc> Borrow<T> for Cow<'_, T, A> {
    fn borrow(&self) -> &T {
        &**self
    }
}

impl<T: ToOwnedIn<A> + ?Sized, A: RawAlloc> Clone for Cow<'_, T, A>
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
            (&mut Self::Owned(ref mut dest), &Self::Owned(ref o)) => dest.clone_from(o),
            (t, s) => *t = s.clone(),
        }
    }
}

impl<T, A: RawAlloc> Debug for Cow<'_, T, A>
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

impl<T, A: RawAlloc> Display for Cow<'_, T, A>
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

impl<T: ToOwnedIn<A> + ?Sized, A: RawAlloc> Deref for Cow<'_, T, A> {
    type Target = T;

    fn deref(&self) -> &T {
        match *self {
            Self::Borrowed(borrowed) => borrowed,
            Self::Owned(ref owned) => owned.borrow(),
        }
    }
}

impl<T: ToOwnedIn<A> + ?Sized, A: RawAlloc> Default for Cow<'_, T, A>
where
    Owned<T, A>: Default,
{
    #[inline]
    fn default() -> Self {
        Self::Owned(Default::default())
    }
}

impl<'b, T: ToOwnedIn<A> + ?Sized, A: RawAlloc> From<&'b T> for Cow<'b, T, A> {
    #[inline]
    fn from(borrow: &'b T) -> Self {
        Self::Borrowed(borrow)
    }
}

impl<'b, T: ToOwnedIn<A> + ?Sized, A: RawAlloc> From<&'b mut T> for Cow<'b, T, A> {
    #[inline]
    fn from(borrow: &'b mut T) -> Self {
        Self::Borrowed(borrow)
    }
}

impl<'a, 'b, T: ToOwnedIn<A> + ?Sized, A: RawAlloc, U: ToOwnedIn<B> + ?Sized, B: RawAlloc>
    PartialEq<Cow<'b, U, B>> for Cow<'a, T, A>
where
    T: PartialEq<U>,
{
    #[inline]
    fn eq(&self, other: &Cow<'b, U, B>) -> bool {
        (&**self).eq(&**other)
    }
}

impl<'a, 'b, T: ToOwnedIn<A> + Eq + ?Sized, A: RawAlloc> Eq for Cow<'a, T, A> {}
