//! A fast arena allocator for network buffers.

use std::alloc::Layout;
use std::ops::{Deref, DerefMut};
use std::ptr::NonNull;
use std::slice;
use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

use bytes::{Bytes, Vtable};
use parking_lot::RwLock;

/// The default chunk size.
pub const PAGE: usize = 65536;

/// A fast arena-based byte allocator optimized for short lifetime objects.
///
/// `Arena` is optimized for objects with a short lifetime, e.g. network buffers. The allocator
/// implementation is not stabilized and might change in the future.
///
/// The current implementation is based on a thread-shared bump allocator with reusable chunks.
#[derive(Debug)]
pub struct Arena {
    chunk_size: usize,
    // FIXME: Maybe this can be a lock-free intrusive doubly linked list.
    chunks: RwLock<Vec<Chunk>>,
    max_chunks: usize,
}

impl Arena {
    /// Creates a new `Arena` with the given `chunk_size`.
    #[inline]
    pub fn new(chunk_size: usize) -> Self {
        Self {
            chunk_size,
            chunks: RwLock::new(Vec::new()),
            max_chunks: usize::MAX,
        }
    }

    #[inline]
    pub const fn builder() -> ArenaBuilder {
        ArenaBuilder::new()
    }

    /// Acquires a new, mutable memory block from `Arena` and returns it in form of a [`BytesMut`].
    pub fn get(&self, size: usize) -> BytesMut {
        if size > self.chunk_size {
            panic!("Cannot allocate larger than chunk size");
        }

        let chunks = self.chunks.read();

        for chunk in &*chunks {
            // We have the only pointer to the chunk. The chunk allocation
            // can be reused.
            if chunk.ref_count.load(Ordering::Relaxed) == 1 {
                unsafe {
                    chunk.reset();
                }
            }

            if let Some(buf) = chunk.get(size) {
                return buf;
            }
        }

        if chunks.len() >= self.max_chunks {
            panic!("Arena chunks oom");
        }

        drop(chunks);
        let chunk = Chunk::new(self.chunk_size);
        let buf = chunk.get(size).unwrap();

        let mut chunks = self.chunks.write();
        chunks.push(chunk);

        buf
    }

    #[inline]
    pub fn chunks(&self) -> usize {
        let chunks = self.chunks.read();
        chunks.len()
    }
}

impl Default for Arena {
    #[inline]
    fn default() -> Self {
        Self::new(PAGE)
    }
}

#[derive(Copy, Clone, Debug)]
pub struct ArenaBuilder {
    chunk_size: usize,
    min_chunks: usize,
    max_chunks: usize,
}

impl ArenaBuilder {
    #[inline]
    pub const fn new() -> Self {
        Self {
            chunk_size: PAGE,
            min_chunks: 0,
            max_chunks: usize::MAX,
        }
    }

    #[inline]
    pub const fn chunk_size(mut self, chunk_size: usize) -> Self {
        self.chunk_size = chunk_size;
        self
    }

    #[inline]
    pub const fn min_chunks(mut self, min_chunks: usize) -> Self {
        self.min_chunks = min_chunks;
        self
    }

    #[inline]
    pub const fn max_chunks(mut self, max_chunks: usize) -> Self {
        self.max_chunks = max_chunks;
        self
    }

    pub fn build(self) -> Arena {
        let mut chunks = Vec::with_capacity(self.min_chunks);
        for _ in 0..self.min_chunks {
            chunks.push(Chunk::new(self.chunk_size));
        }

        Arena {
            chunk_size: self.chunk_size,
            chunks: RwLock::new(chunks),
            max_chunks: self.max_chunks,
        }
    }
}

impl Default for ArenaBuilder {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

/// A fixed-size mutable continguous slice of memory.
#[derive(Debug)]
pub struct BytesMut {
    chunk: NonNull<ChunkInner>,
    ptr: NonNull<u8>,
    len: usize,
}

impl BytesMut {
    /// Shortens the length of the buffer to `len`.
    ///
    /// If `len` is greater than the current length of the buffer, this has no effect.
    #[inline]
    pub fn truncate(&mut self, len: usize) {
        if len <= self.len() {
            unsafe { self.set_len(len) };
        }
    }

    /// Converts `self` into an immutable [`Bytes`].
    #[inline]
    pub fn freeze(self) -> Bytes {
        let ptr = self.ptr.as_ptr();
        let len = self.len;
        let data = AtomicPtr::new(self.chunk.as_ptr().cast());

        // Do not decrement the reference count.
        std::mem::forget(self);

        unsafe { Bytes::with_vtable(ptr, len, data, &SHARED_VTABLE) }
    }

    #[inline]
    unsafe fn set_len(&mut self, len: usize) {
        self.len = len;
    }

    #[inline]
    fn as_slice(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }

    #[inline]
    fn as_slice_mut(&mut self) -> &mut [u8] {
        unsafe { slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }
}

impl AsRef<[u8]> for BytesMut {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl AsMut<[u8]> for BytesMut {
    #[inline]
    fn as_mut(&mut self) -> &mut [u8] {
        self.as_slice_mut()
    }
}

impl Deref for BytesMut {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl DerefMut for BytesMut {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_slice_mut()
    }
}

impl Drop for BytesMut {
    fn drop(&mut self) {
        let chunk = unsafe { self.chunk.as_ref() };

        let rc = chunk.ref_count.fetch_sub(1, Ordering::Release);

        if rc != 1 {
            return;
        }

        unsafe { Box::from_raw(self.ptr.as_ptr()) };
    }
}

unsafe impl Send for BytesMut {}
unsafe impl Sync for BytesMut {}

/// A strong reference to a chunk.
#[derive(Debug)]
struct Chunk {
    ptr: NonNull<ChunkInner>,
}

impl Chunk {
    pub fn new(size: usize) -> Self {
        let inner = Box::new(ChunkInner::new(size));
        let ptr = Box::leak(inner).into();

        Self { ptr }
    }

    pub fn get(&self, size: usize) -> Option<BytesMut> {
        let inner = unsafe { self.ptr.as_ref() };
        let ptr = inner.get(size)?;

        unsafe {
            Some(BytesMut {
                chunk: self.ptr,
                ptr: NonNull::new_unchecked(ptr),
                len: size,
            })
        }
    }
}

impl Deref for Chunk {
    type Target = ChunkInner;

    fn deref(&self) -> &Self::Target {
        unsafe { self.ptr.as_ref() }
    }
}

impl Drop for Chunk {
    fn drop(&mut self) {
        let chunk = unsafe { self.ptr.as_ref() };

        let rc = chunk.ref_count.fetch_sub(1, Ordering::Release);

        if rc != 1 {
            return;
        }

        // Drop the allocation
        unsafe { Box::from_raw(self.ptr.as_ptr()) };
    }
}

unsafe impl Send for Chunk {}
unsafe impl Sync for Chunk {}

#[derive(Debug)]
struct ChunkInner {
    /// The layout used to allocate this chunk.
    layout: Layout,
    /// The starting pointer of the allocated chunk.
    ptr: *mut u8,
    /// Where to put the next bytes block.
    head: AtomicUsize,
    /// Strong reference counts
    ref_count: AtomicUsize,
}

impl ChunkInner {
    pub fn new(size: usize) -> Self {
        let layout = Layout::array::<u8>(size).unwrap();

        let ptr = unsafe { std::alloc::alloc(layout) };
        assert!(!ptr.is_null());

        Self {
            layout,
            ptr,
            head: AtomicUsize::new(0),
            ref_count: AtomicUsize::new(1),
        }
    }

    pub fn get(&self, size: usize) -> Option<*mut u8> {
        let mut head = self.head.load(Ordering::Acquire);

        // Check that this chunk has enough free capacity to hold size bytes.
        if head + size > self.layout.size() {
            return None;
        }

        while let Err(curr) =
            self.head
                .compare_exchange_weak(head, head + size, Ordering::SeqCst, Ordering::SeqCst)
        {
            head = curr;

            if head + size > self.layout.size() {
                return None;
            }
        }

        self.ref_count.fetch_add(1, Ordering::SeqCst);

        // SAFETY: We have exlusive access to ptr + head.
        unsafe { Some(self.ptr.add(head)) }
    }

    unsafe fn reset(&self) {
        // The chunk may immediately be reused with self.get() which uses
        // acquire loads, hence the release store is necessary.
        self.head.store(0, Ordering::Release);
    }
}

impl Drop for ChunkInner {
    fn drop(&mut self) {
        unsafe {
            std::alloc::dealloc(self.ptr, self.layout);
        }
    }
}

// ==== impl Vtable ====

const SHARED_VTABLE: Vtable = Vtable {
    clone: chunk_clone,
    to_vec: chunk_to_vec,
    drop: chunk_drop,
};

unsafe fn chunk_clone(data: &AtomicPtr<()>, ptr: *const u8, len: usize) -> Bytes {
    let shared = data.load(Ordering::Relaxed) as *mut ChunkInner;

    let old = unsafe { (*shared).ref_count.fetch_add(1, Ordering::Relaxed) };

    if old > usize::MAX >> 1 {
        std::process::abort();
    }

    unsafe { Bytes::with_vtable(ptr, len, AtomicPtr::new(shared as *mut ()), &SHARED_VTABLE) }
}

unsafe fn chunk_to_vec(data: &AtomicPtr<()>, ptr: *const u8, len: usize) -> Vec<u8> {
    let mut buf = Vec::with_capacity(len);
    unsafe { std::ptr::copy_nonoverlapping(ptr, buf.as_mut_ptr(), len) };
    buf
}

unsafe fn chunk_drop(data: &mut AtomicPtr<()>, ptr: *const u8, len: usize) {
    let shared = data.load(Ordering::Relaxed) as *mut ChunkInner;

    let rc = unsafe { (*shared).ref_count.fetch_sub(1, Ordering::Release) };

    if rc != 1 {
        return;
    }

    // Fence to prevent reordering of data access after deletion.
    // Syncs with relase load (fetch_sub) above.
    unsafe { (*shared).ref_count.load(Ordering::Acquire) };

    unsafe { Box::from_raw(shared) };
}

#[cfg(test)]
mod tests {
    use super::Arena;

    #[test]
    fn test_arena() {
        let arena = Arena::new(30);

        let mut bufs = Vec::new();
        for _ in 0..30 {
            bufs.push(arena.get(1));
        }
        assert_eq!(arena.chunks(), 1);

        for buf in &bufs {
            assert_eq!(bufs[0].chunk, buf.chunk);
        }

        for buf in &mut bufs {
            for b in buf.as_mut() {
                *b = 42;
            }
        }

        let _c2 = arena.get(1);
        assert_eq!(arena.chunks(), 2);
    }

    #[test]
    fn test_arena_shared() {
        let arena = Arena::new(3000);

        let buf = arena.get(1500);
        let b1 = buf.freeze();
        let b2 = b1.clone();
        let b3 = b2.clone();
        assert_eq!(arena.chunks(), 1);

        drop(b2);
        drop(b1);
        drop(b3);
    }

    #[test]
    fn test_chunk_reuse() {
        let arena = Arena::new(3000);

        let b1 = arena.get(1500);
        let b2 = arena.get(1500);
        assert_eq!(arena.chunks(), 1);

        let ptr1 = b1.chunk;

        let _b3 = arena.get(1500);
        assert_eq!(arena.chunks(), 2);

        drop(b1);
        drop(b2);
        let b4 = arena.get(1500);

        assert_eq!(ptr1, b4.chunk);
    }
}
