/// Allocate a new Vec instance with optional initial items.
#[macro_export]
macro_rules! vec {
    (in $alloc:expr $(;)?) => (
        $crate::vec::Vec::new_in($alloc)
    );
    (in $alloc:expr; $elem:expr; $n:expr) => (
        $crate::vec::from_elem_in($elem, $n, $alloc)
    );
    (in $alloc:expr; $($x:expr),+ $(,)?) => (
        $crate::vec::from_array_in([$($x),+], $alloc)
    );
    () => (
        $crate::vec::Vec::new()
    );
    ($elem:expr; $n:expr) => (
        $crate::vec::from_elem($elem, $n)
    );
    ($($x:expr),+ $(,)?) => (
        $crate::vec::from_array([$($x),+])
    );
}
