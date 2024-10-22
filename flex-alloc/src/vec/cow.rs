use super::Vec;
use crate::alloc::{AllocateIn, Allocator, AllocatorDefault};
use crate::borrow::{Cow, ToOwnedIn};
use crate::error::StorageError;

impl<T: Clone, A: Allocator> ToOwnedIn<A> for [T] {
    type Owned = Vec<T, A>;

    fn try_to_owned_in<I>(&self, alloc_in: I) -> Result<Self::Owned, StorageError>
    where
        I: AllocateIn<Alloc = A>,
    {
        Vec::try_from_slice_in(self, alloc_in)
    }
}

impl<'a, T: Clone, A: Allocator, const N: usize> From<&'a [T; N]> for Cow<'a, [T], A> {
    fn from(s: &'a [T; N]) -> Cow<'a, [T], A> {
        Cow::Borrowed(s.as_slice())
    }
}

impl<'a, T: Clone, A: Allocator, const N: usize> From<&'a mut [T; N]> for Cow<'a, [T], A> {
    fn from(s: &'a mut [T; N]) -> Cow<'a, [T], A> {
        Cow::Borrowed(s.as_slice())
    }
}

impl<'a, T: Clone, A: Allocator> From<Vec<T, A>> for Cow<'a, [T], A> {
    fn from(vec: Vec<T, A>) -> Cow<'a, [T], A> {
        Cow::Owned(vec)
    }
}

impl<'a, T: Clone, A: Allocator> From<&'a Vec<T, A>> for Cow<'a, [T], A> {
    fn from(vec: &'a Vec<T, A>) -> Cow<'a, [T], A> {
        Cow::Borrowed(vec.as_slice())
    }
}

impl<'a, T: Clone, A: AllocatorDefault> FromIterator<T> for Cow<'a, [T], A> {
    fn from_iter<I: IntoIterator<Item = T>>(it: I) -> Cow<'a, [T], A> {
        Cow::Owned(Vec::from_iter(it))
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "alloc")]
    use crate::alloc::Global;

    #[cfg(feature = "alloc")]
    use const_default::ConstDefault;

    #[cfg(feature = "alloc")]
    use super::*;

    #[cfg(feature = "alloc")]
    #[test]
    fn cow_borrow_vec() {
        let mut b = Cow::<[u32], Global>::default();
        assert!(b.is_owned());
        b.to_mut().push(1);
        assert_eq!(b.into_owned(), &[1]);

        let b = Cow::<[u32], Global>::from(&[1, 2, 3]);
        assert!(b.is_borrowed());
        assert_eq!(b.into_owned(), &[1, 2, 3]);
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn const_default_cow() {
        let c = Cow::<[u32], Global>::DEFAULT;
        assert_eq!(c.as_ref(), &[]);
    }
}
