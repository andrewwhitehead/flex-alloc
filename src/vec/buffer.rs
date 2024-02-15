use core::alloc::Layout;
use core::fmt::Debug;
use core::marker::PhantomData;
use core::mem::MaybeUninit;
use core::ptr::NonNull;
use core::slice;

use crate::error::StorageError;
use crate::index::Index;
use crate::storage::utils::{aligned_byte_slice, array_layout};
use crate::storage::{
    AllocBuffer, AllocHeader, AllocLayout, ByteBuffer, Fixed, FixedBuffer, InlineBuffer, RawBuffer,
};

use super::config::VecConfig;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct VecHeader<I: Index = usize> {
    pub capacity: I,
    pub length: I,
}

impl<I: Index> AllocHeader for VecHeader<I> {
    const EMPTY: Self = VecHeader {
        capacity: I::ZERO,
        length: I::ZERO,
    };

    #[inline]
    fn is_empty(&self) -> bool {
        self.capacity == I::ZERO
    }
}

#[derive(Debug)]
pub struct VecData<T, I: Index = usize>(PhantomData<(T, I)>);

impl<T, I: Index> AllocLayout for VecData<T, I> {
    type Header = VecHeader<I>;
    type Data = T;

    #[inline]
    fn layout(header: &Self::Header) -> Result<Layout, StorageError> {
        array_layout::<T>(header.capacity.to_usize())
    }
}

pub trait VecBuffer: RawBuffer<RawData = Self::Data> {
    type Data;
    type Index: Index;

    fn capacity(&self) -> Self::Index;

    fn length(&self) -> Self::Index;

    /// The capacity of the buffer must be established as greater than zero,
    /// otherwise this method may attempt to write into unallocated memory.
    unsafe fn set_length(&mut self, len: Self::Index);

    #[inline]
    fn as_uninit_slice(&mut self) -> &mut [MaybeUninit<Self::Data>] {
        unsafe { slice::from_raw_parts_mut(self.data_ptr_mut().cast(), self.capacity().to_usize()) }
    }

    #[inline]
    fn as_slice(&self) -> &[Self::Data] {
        unsafe { slice::from_raw_parts(self.data_ptr(), self.length().to_usize()) }
    }

    #[inline]
    fn as_mut_slice(&mut self) -> &mut [Self::Data] {
        unsafe { slice::from_raw_parts_mut(self.data_ptr_mut(), self.length().to_usize()) }
    }

    #[inline]
    unsafe fn uninit_index(&mut self, index: usize) -> &mut MaybeUninit<Self::Data> {
        &mut *self.data_ptr_mut().add(index).cast()
    }

    #[inline]
    fn vec_resize(&mut self, capacity: Self::Index) -> Result<(), StorageError> {
        let _ = capacity;
        Err(StorageError::Unsupported)
    }
}

pub trait VecBufferSpawn: VecBuffer {
    #[inline]
    fn vec_spawn(&self, capacity: Self::Index) -> Result<Self, StorageError> {
        let _ = capacity;
        Err(StorageError::Unsupported)
    }
}

pub trait IntoFixedVec<'a, T> {
    fn into_fixed_vec(self) -> <Fixed<'a> as VecConfig>::Buffer<T>;
}

impl<'a, T: 'a, const N: usize> VecBuffer for InlineBuffer<T, N> {
    type Data = T;
    type Index = usize;

    #[inline]
    fn capacity(&self) -> usize {
        N
    }

    #[inline]
    fn length(&self) -> usize {
        self.length
    }

    #[inline]
    unsafe fn set_length(&mut self, len: usize) {
        self.length = len;
    }
}

impl<'a, T, I: Index> VecBuffer for FixedBuffer<VecHeader<I>, T> {
    type Data = T;
    type Index = I;

    #[inline]
    fn capacity(&self) -> I {
        self.header.capacity
    }

    #[inline]
    fn length(&self) -> I {
        self.header.length
    }

    #[inline]
    unsafe fn set_length(&mut self, len: I) {
        self.header.length = len;
    }
}

impl<'a, T: 'a, const N: usize> IntoFixedVec<'a, T> for &'a mut [MaybeUninit<T>; N] {
    fn into_fixed_vec(self) -> <Fixed<'a> as VecConfig>::Buffer<T> {
        self[..].into_fixed_vec()
    }
}

impl<'a, T: 'a> IntoFixedVec<'a, T> for &'a mut [MaybeUninit<T>] {
    fn into_fixed_vec(self) -> <Fixed<'a> as VecConfig>::Buffer<T> {
        FixedBuffer::new(
            VecHeader {
                capacity: self.len(),
                length: 0,
            },
            unsafe { NonNull::new_unchecked(self.as_mut_ptr()).cast() },
        )
    }
}

impl<'a, T: 'a, T2, const N: usize> IntoFixedVec<'a, T> for &'a mut ByteBuffer<T2, N> {
    fn into_fixed_vec(self) -> <Fixed<'a> as VecConfig>::Buffer<T> {
        let (data, capacity) = aligned_byte_slice(self.as_uninit_slice());
        FixedBuffer::new(
            VecHeader {
                capacity,
                length: 0,
            },
            data,
        )
    }
}

impl<'a, B, T, I: Index> VecBuffer for B
where
    B: AllocBuffer<Meta = VecData<T, I>>,
{
    type Data = T;
    type Index = I;

    #[inline]
    fn capacity(&self) -> I {
        if self.is_empty_buffer() {
            I::ZERO
        } else {
            unsafe { self.header() }.capacity
        }
    }

    #[inline]
    fn length(&self) -> I {
        if self.is_empty_buffer() {
            I::ZERO
        } else {
            unsafe { self.header() }.length
        }
    }

    #[inline]
    unsafe fn set_length(&mut self, len: I) {
        self.header_mut().length = len;
    }

    #[inline]
    fn vec_resize(&mut self, capacity: Self::Index) -> Result<(), StorageError> {
        let length = self.length();
        self.resize_buffer(VecHeader { capacity, length })?;
        Ok(())
    }
}

impl<'a, B, T, I: Index> VecBufferSpawn for B
where
    B: AllocBuffer<Meta = VecData<T, I>>,
{
    #[inline]
    fn vec_spawn(&self, capacity: Self::Index) -> Result<Self, StorageError> {
        let length = self.length();
        self.spawn_buffer(VecHeader { capacity, length })
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::storage::buffer::BufferAllocIn;
//     use core::mem;
//     use core::slice;

//     fn alloc_test<'a, T, I: Index, A: BufferAllocIn<'a, VecHeader<I>, T>>(
//         mut allocator: A,
//         count: Option<I>,
//     ) -> Result<A::Buffer, StorageError> {
//         let elt_size = mem::size_of::<T>();
//         let capacity = if let Some(count) = count {
//             count
//         } else if elt_size == 0 {
//             I::MAX
//         } else if let Some(fixed_size) = allocator.fixed_size() {
//             I::from_usize(fixed_size / elt_size).unwrap_or(I::MAX)
//         } else {
//             I::ZERO
//         };
//         if let Some(size) = elt_size.checked_mul(capacity.into()) {
//             allocator.alloc_in(
//                 VecHeader {
//                     capacity,
//                     length: I::ZERO,
//                 },
//                 size,
//             )
//         } else {
//             Err(StorageError::CapacityLimit)
//         }
//     }

//     #[test]
//     fn fixed_buf() {
//         pub const fn fixed_buffer<const SIZE: usize>() -> [MaybeUninit<u8>; SIZE] {
//             [MaybeUninit::uninit(); SIZE]
//         }

//         let mut buf = fixed_buffer::<32>();

//         let mut res =
//             alloc_test::<usize, usize, _>(buf.as_mut_slice(), Some(3)).expect("Error allocating");
//         println!("5: {:?}", res);

//         let data = unsafe {
//             slice::from_raw_parts_mut::<MaybeUninit<usize>>(res.data_ptr_mut().cast(), 3)
//         };
//         data[0].write(1);
//         data[1].write(2);
//         data[2].write(3);

//         println!("{:?}", unsafe {
//             slice::from_raw_parts_mut::<u8>(buf.as_mut_ptr().cast(), buf.len())
//         });
//         // println!("{:?}", unsafe {
//         //     slice::from_raw_parts_mut::<u8>(data.as_mut_ptr().cast(), 24)
//         // });

//         // println!("{:?}", mem::size_of::<[usize; 3]>());
//     }
// }
