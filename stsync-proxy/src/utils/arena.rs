//! A fast arena allocator for network buffers.

use std::alloc::Layout;
use std::alloc::{AllocError, Allocator};
use std::cell::UnsafeCell;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use parking_lot::RwLock;

const PAGE: usize = 65536;

#[derive(Clone, Debug)]
pub struct Arena {
    inner: Arc<ArenaInner>,
}

#[derive(Debug)]
struct ArenaInner {
    page_size: usize,
    chunks: RwLock<Vec<(Chunk, *const u8)>>,
}

impl Arena {
    #[inline]
    pub fn new(page_size: usize) -> Self {
        Self {
            inner: Arc::new(ArenaInner {
                page_size,
                chunks: RwLock::new(Vec::new()),
            }),
        }
    }

    unsafe fn alloc(&self, mut layout: Layout) -> NonNull<[u8]> {
        layout = layout.pad_to_align();

        if layout.size() > self.inner.page_size {
            panic!("exceeded page size");
            // The requested layout is bigger than the maximum chunk size. We can never
            // allocate this object with this arena. Forward to global allocator.
            // return unsafe { NonNull::new_unchecked(std::alloc::alloc(layout)) };
            // Equivalent to ptr.cast() but is unsized.
        }

        let chunks = self.inner.chunks.read();
        for (chunk, _) in &*chunks {
            if let Ok(ptr) = unsafe { chunk.allocate_padded(layout) } {
                return ptr;
            }
        }

        let (chunk, chunk_ptr) = self.make_chunk();

        drop(chunks);
        let mut chunks = self.inner.chunks.write();
        let ptr = unsafe { chunk.allocate_padded(layout).unwrap() };

        chunks.push((chunk, chunk_ptr));
        ptr
    }

    unsafe fn dealloc(&self, ptr: NonNull<u8>, layout: Layout) {
        let mut chunks = self.inner.chunks.write();
        for (i, (chunk, base)) in chunks.iter().enumerate() {
            // Pointer within base + page_size; the pointer is a part of this chunk.
            if ptr.as_ptr() as usize <= *base as usize + self.inner.page_size {
                // SAFETY: `ptr` was previously allocated in this `chunk`.
                unsafe {
                    chunk.deallocate(layout, ptr);
                }

                chunks.remove(i);
                break;
            }
        }
    }

    #[inline]
    fn make_chunk(&self) -> (Chunk, *const u8) {
        let layout = unsafe {
            Layout::from_size_align_unchecked(
                std::mem::size_of::<u8>() * self.inner.page_size,
                std::mem::align_of::<usize>(),
            )
        };

        let ptr = unsafe { std::alloc::alloc(layout) };
        assert!(!ptr.is_null());

        let ptr = unsafe { NonNull::new_unchecked(ptr) };

        (
            Chunk {
                arena: self.clone(),
                ptr,
                layout,
                head: AtomicUsize::new(0),
                entries: AtomicUsize::new(0),
            },
            ptr.as_ptr(),
        )
    }
}

impl Default for Arena {
    #[inline]
    fn default() -> Self {
        Self::new(PAGE)
    }
}

unsafe impl Allocator for Arena {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, std::alloc::AllocError> {
        unsafe { Ok(self.alloc(layout)) }
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        unsafe { self.dealloc(ptr, layout) };
    }
}

/// A fixed size memory page.
#[derive(Debug)]
struct Chunk {
    arena: Arena,
    ptr: NonNull<u8>,
    layout: Layout,
    head: AtomicUsize,
    /// The number of active references to the buffer of this chunk.
    entries: AtomicUsize,
}

impl Chunk {
    /// # Safety
    ///
    /// The `Chunk` must have enough [`contiguous_capacity`] to allocate using `layout`.
    ///
    /// [`contiguous_capacity`]: Self::contiguous_capacity
    unsafe fn allocate_padded(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        // Acquire the head offset.
        let mut head = self.head.load(Ordering::SeqCst);
        if head > head + layout.size() {
            return Err(AllocError);
        }

        while let Err(current) = self.head.compare_exchange_weak(
            head,
            head + layout.size(),
            Ordering::SeqCst,
            Ordering::SeqCst,
        ) {
            head = current;

            // Another thread won the race and we no longer have enough space to
            // allocate using this `layout`.
            if head > head + layout.size() {
                return Err(AllocError);
            }
        }

        self.entries.fetch_add(1, Ordering::SeqCst);

        let ptr = unsafe { NonNull::new_unchecked(self.ptr.as_ptr().add(head)) };
        let len = layout.size();

        // SAFETY: The caller guarantees that `end < buffer.len()`.
        // let fatptr = [self.buffer.as_ptr() as usize + head, layout.size()];
        // Ok(unsafe { std::mem::transmute::<[usize; 2], NonNull<[u8]>>(fatptr) })
        // Ok(unsafe { self.buffer.get_unchecked(head..end).into() })
        Ok(NonNull::from_raw_parts(ptr.cast(), len))
    }

    /// Deallocate given memory block owned by this `Chunk`.
    ///
    /// # Safety
    ///
    /// The given `ptr` must be owned by this `Chunk`, i.e. have been previously allocated using
    /// `allocate` of the same `Chunk`. The `layout` must be same layout used to allocate.
    unsafe fn deallocate(&self, layout: Layout, ptr: NonNull<u8>) {
        self.entries.fetch_sub(1, Ordering::SeqCst);
    }
}

impl Drop for Chunk {
    fn drop(&mut self) {
        unsafe {
            std::alloc::dealloc(self.ptr.as_ptr(), self.layout);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::alloc::{Allocator, Layout};
    use std::slice;

    use super::{Arena, PAGE};

    #[test]
    fn test_arena() {
        let arena = Arena::default();
        let ptr = arena.allocate(Layout::new::<u8>()).unwrap();

        unsafe { arena.deallocate(ptr.to_raw_parts().0.cast(), Layout::new::<u8>()) };

        let mut ptr = arena.allocate(Layout::array::<u8>(PAGE).unwrap()).unwrap();
        let slice = unsafe { ptr.as_mut() };
        for b in slice {
            *b = 25;
        }

        unsafe {
            arena.deallocate(
                ptr.to_raw_parts().0.cast(),
                Layout::array::<u8>(PAGE).unwrap(),
            );
        }
    }

    #[test]
    fn test_arena_vec() {
        let arena = Arena::default();

        let mut vec = Vec::new_in(arena);
        vec.push("Hello World");
        vec.push("str1");
        vec.push("str2");
        vec.push("str3");
        vec.truncate(0);
        vec.shrink_to_fit();
        drop(vec);
    }
}
