//! A fast arena allocator for network buffers.

use std::alloc::Layout;
use std::ops::{Deref, DerefMut};
use std::ptr::NonNull;
use std::slice;
use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

use bytes::{Bytes, Vtable};
use parking_lot::Mutex;

const PAGE: usize = 65536;

#[derive(Debug)]
pub struct Arena {
    chunk_size: usize,
    // FIXME: Maybe this can be a lock-free intrusive doubly linked list.
    chunks: Mutex<Vec<Chunk>>,
}

impl Arena {
    pub fn new(chunk_size: usize) -> Self {
        Self {
            chunk_size,
            chunks: Mutex::new(Vec::new()),
        }
    }

    pub fn get(&self, size: usize) -> BytesMut {
        if size > self.chunk_size {
            panic!("Cannot allocate larger than chunk size");
        }

        let mut chunks = self.chunks.lock();

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

        let chunk = Chunk::new(self.chunk_size);
        let buf = chunk.get(size).unwrap();
        chunks.push(chunk);

        buf
    }

    pub fn chunks(&self) -> usize {
        let chunks = self.chunks.lock();
        chunks.len()
    }
}

impl Default for Arena {
    #[inline]
    fn default() -> Self {
        Self::new(PAGE)
    }
}

#[derive(Debug)]
pub struct BytesMut {
    chunk: NonNull<ChunkInner>,
    ptr: NonNull<u8>,
    len: usize,
}

impl BytesMut {
    pub fn truncate(&mut self, len: usize) {
        if len <= self.len() {
            unsafe { self.set_len(len) };
        }
    }

    pub fn freeze(self) -> Bytes {
        let ptr = self.ptr.as_ptr();
        let len = self.len;
        let data = AtomicPtr::new(self.chunk.as_ptr().cast());

        // Do not decrement the reference count.
        std::mem::forget(self);

        unsafe { Bytes::with_vtable(ptr, len, data, &SHARED_VTABLE) }
    }

    pub unsafe fn set_len(&mut self, len: usize) {
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
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl AsMut<[u8]> for BytesMut {
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
        let mut inner = Box::new(ChunkInner::new(size));
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

// #[derive(Debug)]
// struct ArenaInner {
//     page_size: usize,
//     chunks: RwLock<Vec<(Chunk, *const u8)>>,
// }

// impl Arena {
//     #[inline]
//     pub fn new(page_size: usize) -> Self {
//         Self {
//             inner: Arc::new(ArenaInner {
//                 page_size,
//                 chunks: RwLock::new(Vec::new()),
//             }),
//         }
//     }

//     unsafe fn alloc(&self, mut layout: Layout) -> NonNull<[u8]> {
//         layout = layout.pad_to_align();

//         if layout.size() > self.inner.page_size {
//             panic!("exceeded page size");
//             // The requested layout is bigger than the maximum chunk size. We can never
//             // allocate this object with this arena. Forward to global allocator.
//             // return unsafe { NonNull::new_unchecked(std::alloc::alloc(layout)) };
//             // Equivalent to ptr.cast() but is unsized.
//         }

//         let chunks = self.inner.chunks.read();
//         for (chunk, _) in &*chunks {
//             if let Ok(ptr) = unsafe { chunk.allocate_padded(layout) } {
//                 return ptr;
//             }
//         }

//         let (chunk, chunk_ptr) = self.make_chunk();

//         drop(chunks);
//         let mut chunks = self.inner.chunks.write();
//         let ptr = unsafe { chunk.allocate_padded(layout).unwrap() };

//         chunks.push((chunk, chunk_ptr));
//         ptr
//     }

//     unsafe fn dealloc(&self, ptr: NonNull<u8>, layout: Layout) {
//         let mut chunks = self.inner.chunks.write();
//         for (i, (chunk, base)) in chunks.iter().enumerate() {
//             // Pointer within base + page_size; the pointer is a part of this chunk.
//             if ptr.as_ptr() as usize <= *base as usize + self.inner.page_size {
//                 // SAFETY: `ptr` was previously allocated in this `chunk`.
//                 unsafe {
//                     chunk.deallocate(layout, ptr);
//                 }

//                 chunks.remove(i);
//                 break;
//             }
//         }
//     }

//     #[inline]
//     fn make_chunk(&self) -> (Chunk, *const u8) {
//         let layout = unsafe {
//             Layout::from_size_align_unchecked(
//                 std::mem::size_of::<u8>() * self.inner.page_size,
//                 std::mem::align_of::<usize>(),
//             )
//         };

//         let ptr = unsafe { std::alloc::alloc(layout) };
//         assert!(!ptr.is_null());

//         let ptr = unsafe { NonNull::new_unchecked(ptr) };

//         (
//             Chunk {
//                 arena: self.clone(),
//                 ptr,
//                 layout,
//                 head: AtomicUsize::new(0),
//                 entries: AtomicUsize::new(0),
//             },
//             ptr.as_ptr(),
//         )
//     }
// }

// impl Default for Arena {
//     #[inline]
//     fn default() -> Self {
//         Self::new(PAGE)
//     }
// }

// unsafe impl Allocator for Arena {
//     fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, std::alloc::AllocError> {
//         unsafe { Ok(self.alloc(layout)) }
//     }

//     unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
//         unsafe { self.dealloc(ptr, layout) };
//     }
// }

// /// A fixed size memory page.
// #[derive(Debug)]
// struct Chunk {
//     arena: Arena,
//     ptr: NonNull<u8>,
//     layout: Layout,
//     head: AtomicUsize,
//     /// The number of active references to the buffer of this chunk.
//     entries: AtomicUsize,
// }

// impl Chunk {
//     /// # Safety
//     ///
//     /// The `Chunk` must have enough [`contiguous_capacity`] to allocate using `layout`.
//     ///
//     /// [`contiguous_capacity`]: Self::contiguous_capacity
//     unsafe fn allocate_padded(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
//         // Acquire the head offset.
//         let mut head = self.head.load(Ordering::SeqCst);
//         if head > head + layout.size() {
//             return Err(AllocError);
//         }

//         while let Err(current) = self.head.compare_exchange_weak(
//             head,
//             head + layout.size(),
//             Ordering::SeqCst,
//             Ordering::SeqCst,
//         ) {
//             head = current;

//             // Another thread won the race and we no longer have enough space to
//             // allocate using this `layout`.
//             if head > head + layout.size() {
//                 return Err(AllocError);
//             }
//         }

//         self.entries.fetch_add(1, Ordering::SeqCst);

//         let ptr = unsafe { NonNull::new_unchecked(self.ptr.as_ptr().add(head)) };
//         let len = layout.size();

//         // SAFETY: The caller guarantees that `end < buffer.len()`.
//         // let fatptr = [self.buffer.as_ptr() as usize + head, layout.size()];
//         // Ok(unsafe { std::mem::transmute::<[usize; 2], NonNull<[u8]>>(fatptr) })
//         // Ok(unsafe { self.buffer.get_unchecked(head..end).into() })
//         Ok(NonNull::from_raw_parts(ptr.cast(), len))
//     }

//     /// Deallocate given memory block owned by this `Chunk`.
//     ///
//     /// # Safety
//     ///
//     /// The given `ptr` must be owned by this `Chunk`, i.e. have been previously allocated using
//     /// `allocate` of the same `Chunk`. The `layout` must be same layout used to allocate.
//     unsafe fn deallocate(&self, layout: Layout, ptr: NonNull<u8>) {
//         self.entries.fetch_sub(1, Ordering::SeqCst);
//     }
// }

// impl Drop for Chunk {
//     fn drop(&mut self) {
//         unsafe {
//             std::alloc::dealloc(self.ptr.as_ptr(), self.layout);
//         }
//     }
// }

// #[cfg(test)]
// mod tests {
//     use std::alloc::{Allocator, Layout};
//     use std::slice;

//     use super::{Arena, PAGE};

//     #[test]
//     fn test_arena() {
//         let arena = Arena::default();
//         let ptr = arena.allocate(Layout::new::<u8>()).unwrap();

//         unsafe { arena.deallocate(ptr.to_raw_parts().0.cast(), Layout::new::<u8>()) };

//         let mut ptr = arena.allocate(Layout::array::<u8>(PAGE).unwrap()).unwrap();
//         let slice = unsafe { ptr.as_mut() };
//         for b in slice {
//             *b = 25;
//         }

//         unsafe {
//             arena.deallocate(
//                 ptr.to_raw_parts().0.cast(),
//                 Layout::array::<u8>(PAGE).unwrap(),
//             );
//         }
//     }

//     #[test]
//     fn test_arena_vec() {
//         let arena = Arena::default();

//         let mut vec = Vec::new_in(arena);
//         vec.push("Hello World");
//         vec.push("str1");
//         vec.push("str2");
//         vec.push("str3");
//         vec.truncate(0);
//         vec.shrink_to_fit();
//         drop(vec);
//     }
// }

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
