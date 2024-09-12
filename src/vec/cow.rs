use crate::error::StorageError;
use crate::storage::alloc::RawAllocDefault;
use crate::{
    borrow::{Cow, ToOwnedIn},
    storage::{RawAlloc, RawAllocIn},
};

use super::Vec;

impl<T: Clone, A: RawAlloc> ToOwnedIn<A> for [T] {
    type Owned = Vec<T, A>;

    fn try_to_owned_in<I>(&self, alloc_in: I) -> Result<Self::Owned, StorageError>
    where
        I: RawAllocIn<RawAlloc = A>,
    {
        Vec::try_from_slice_in(self, alloc_in)
    }
}

impl<'a, T: Clone, A: RawAlloc, const N: usize> From<&'a [T; N]> for Cow<'a, [T], A> {
    fn from(s: &'a [T; N]) -> Cow<'a, [T], A> {
        Cow::Borrowed(s.as_slice())
    }
}

impl<'a, T: Clone, A: RawAlloc, const N: usize> From<&'a mut [T; N]> for Cow<'a, [T], A> {
    fn from(s: &'a mut [T; N]) -> Cow<'a, [T], A> {
        Cow::Borrowed(s.as_slice())
    }
}

impl<'a, T: Clone, A: RawAlloc> From<Vec<T, A>> for Cow<'a, [T], A> {
    fn from(vec: Vec<T, A>) -> Cow<'a, [T], A> {
        Cow::Owned(vec)
    }
}

impl<'a, T: Clone, A: RawAlloc> From<&'a Vec<T, A>> for Cow<'a, [T], A> {
    fn from(vec: &'a Vec<T, A>) -> Cow<'a, [T], A> {
        Cow::Borrowed(vec.as_slice())
    }
}

impl<'a, T: Clone, A: RawAllocDefault> FromIterator<T> for Cow<'a, [T], A> {
    fn from_iter<I: IntoIterator<Item = T>>(it: I) -> Cow<'a, [T], A> {
        Cow::Owned(Vec::from_iter(it))
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "alloc")]
    use super::*;

    #[cfg(feature = "alloc")]
    #[test]
    fn cow_borrow_vec() {
        use crate::storage::Global;
        let mut b = Cow::<[u32], Global>::default();
        assert!(b.is_owned());
        b.to_mut().push(1);
        assert_eq!(b.into_owned(), &[1]);

        let b = Cow::<[u32], Global>::from(&[1, 2, 3]);
        assert!(b.is_borrowed());
        assert_eq!(b.into_owned(), &[1, 2, 3]);
    }
}
