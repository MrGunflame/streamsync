//! Buffer for SRT transmission
//!
//! When a packet is transmitted it is held in the buffer until acknowledged by the peer. This
//! allows retransmission of lost packets.
//!
//! This buffer implementation uses a ringbuffer with a fixed size on creation to store segments.
//! Segments are held until acknowledged by the peer, at which point they will be removed from the
//! buffer. Once the ring buffer overflows old segments are overwritten, even if not acknowledged
//! by the peer.

use std::mem::MaybeUninit;
use std::ptr;

/// A circular buffer designed for storing SRT segments.
#[derive(Debug)]
pub struct Buffer<T> {
    buf: Box<[MaybeUninit<T>]>,
    /// The index of the next slot in the buffer.
    head: usize,
    /// The index of the last readable slot in the buffer.
    tail: usize,
}

impl<T> Buffer<T> {
    /// Creates a new `Buffer` with the given `size`.
    pub fn new(size: usize) -> Self {
        let mut vec = Vec::with_capacity(size);
        for _ in 0..size {
            vec.push(MaybeUninit::uninit());
        }

        let buf = vec.into_boxed_slice();

        Self {
            buf,
            head: 0,
            tail: 0,
        }
    }

    /// Returns the size of the `Buffer`.
    ///
    /// Note that this is not the number of elements in the `Buffer`. Use [`len`] to get the number
    /// of elements in the `Buffer`.
    ///
    /// [`len`]: Buffer::len
    #[inline]
    pub fn size(&self) -> usize {
        self.buf.len()
    }

    /// Returns the number of elements in the `Buffer`.
    ///
    /// Note that this is not the size of the `Buffer`. Use [`size`] to get the size of the
    /// `Buffer`.
    ///
    /// [`size`]: Buffer::size
    #[inline]
    pub fn len(&self) -> usize {
        if self.head < self.size() {
            self.head
        } else {
            self.size()
        }
    }

    /// Returns `true` if the `Buffer` contains no elements.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Removes all elements from the buffer.
    pub fn clear(&mut self) {
        // First rotation, we only drop the initialized elements.
        let len = if self.head <= self.size() {
            self.head
        } else {
            self.buf.len()
        };

        for index in 0..len {
            unsafe {
                let elem = self.buf.get_unchecked_mut(index);
                elem.assume_init_drop();
            }
        }

        self.head = 0;
        self.tail = 0;
    }

    pub fn push(&mut self, item: T) {
        let pos = self.head % self.size();

        let mut old = unsafe {
            let ptr = self.buf.as_mut_ptr().add(pos);

            ptr::replace(ptr, MaybeUninit::new(item))
        };

        // If this is not the first write rotation, we drop the old value.
        if self.head > self.size() {
            unsafe {
                old.assume_init_drop();
            }
        }

        if self.head - self.tail > self.size() {
            self.tail += 1;
        }

        self.head += 1;
    }

    pub fn get(&self, index: usize) -> Option<&T> {
        if index >= self.head || index < self.tail {
            return None;
        }

        let pos = index % self.size();

        // SAFETY: The index is between self.head and self.tail meaning that it
        // is initialized.
        Some(unsafe { self.buf.get_unchecked(pos).assume_init_ref() })
    }
}

impl<T> Extend<T> for Buffer<T> {
    fn extend<I>(&mut self, iter: I)
    where
        I: IntoIterator<Item = T>,
    {
        for elem in iter.into_iter() {
            self.push(elem);
        }
    }
}

impl<T> Drop for Buffer<T> {
    fn drop(&mut self) {
        self.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::Buffer;

    #[test]
    fn test_buffer_new() {
        let mut buf = Buffer::new(8192);
        assert_eq!(buf.size(), 8192);
        assert_eq!(buf.len(), 0);

        for i in 0..4096 {
            buf.push(i);
        }

        assert_eq!(buf.size(), 8192);
        assert_eq!(buf.len(), 4096);

        buf.clear();
        assert_eq!(buf.size(), 8192);
        assert_eq!(buf.len(), 0);

        for i in 0..4096 * 3 {
            buf.push(i);
        }

        assert_eq!(buf.size(), 8192);
        assert_eq!(buf.len(), 8192);
    }

    #[test]
    fn test_buffer_get() {
        let mut buf = Buffer::new(8192);
        for i in 0..4096 {
            buf.push(i);
        }

        for i in 0..4096 {
            assert_eq!(*buf.get(i).unwrap(), i);
        }

        assert_eq!(buf.get(4096), None);

        let mut buf = Buffer::new(8192);
        for i in 0..4096 * 3 {
            buf.push(i);
        }

        for i in 0..4095 {
            assert_eq!(buf.get(i), None);
        }

        for i in 4096..4096 * 3 {
            assert_eq!(*buf.get(i).unwrap(), i);
        }
    }
}
