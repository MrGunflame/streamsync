use std::{
    cell::UnsafeCell,
    num::NonZeroUsize,
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc, Mutex,
    },
};

use gstreamer::Caps;

/// A fixed size video ring buffer.
///
/// When reading a stream we write to one end of the buffer, while reading from the other end.
/// **THIS MAY HAPPEN AT THE SAME TIME FROM DIFFERENT THREADS.**
///
/// # Writing
///
/// Writing to buffer is done using [`write`]. Note that the call to [`write`] blocks until there
/// is enough space in the buffer to write the complete slice.
pub struct VideoBuffer {
    buf: UnsafeCell<Vec<u8>>,
    read_cursor: AtomicUsize,
    writer_cursor: AtomicUsize,
    caps: Mutex<Option<Caps>>,
    // Non-Zero usize, 0 = None
    frame_size: AtomicUsize,
    tx: mpsc::Sender<gstreamer::Sample>,
    rx: mpsc::Receiver<gstreamer::Sample>,
}

impl VideoBuffer {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();

        Self {
            buf: UnsafeCell::new(vec![0; 1000 * 1000 * 1000]),
            read_cursor: AtomicUsize::new(0),
            writer_cursor: AtomicUsize::new(0),
            caps: Mutex::new(None),
            frame_size: AtomicUsize::new(0),
            tx,
            rx,
        }
    }

    pub fn set_frame_size(&self, size: usize) {
        self.frame_size.store(size, Ordering::SeqCst)
    }

    pub fn frame_size(&self) -> Option<NonZeroUsize> {
        match self.frame_size.load(Ordering::SeqCst) {
            0 => None,
            n => unsafe { Some(NonZeroUsize::new_unchecked(n)) },
        }
    }

    pub fn set_caps(&self, caps: Caps) {
        *self.caps.lock().unwrap() = Some(caps);
    }

    pub fn caps(&self) -> Option<Caps> {
        self.caps.lock().unwrap().clone()
    }

    /// Returns `true` if there is at least len byte to read.
    pub fn can_read(&self, len: usize) -> bool {
        // println!(
        //     "{} - {}",
        //     self.writer_cursor.load(Ordering::SeqCst),
        //     self.read_cursor.load(Ordering::SeqCst)
        // );
        self.writer_cursor.load(Ordering::SeqCst) - self.read_cursor.load(Ordering::SeqCst) >= len
    }

    pub fn write_buf(&self, buf: gstreamer::Sample) {
        self.tx.send(buf).unwrap();
    }

    pub fn read_buf(&self) -> gstreamer::Sample {
        self.rx.recv().unwrap()
    }

    /// Reads `len` bytes from the buffer.
    /// Returns `None` if there are currently no bytes to read. Note that this does not mean that
    /// there will never any more bytes to read.
    pub fn read(&self, len: usize) -> Option<Vec<u8>> {
        // let bytes_remaining = self.writer_cursor - self.read_cursor;
        // println!("Can read {} bytes", bytes_remaining);

        // let bytes_to_copy;
        // if bytes_remaining >= len {
        //     bytes_to_copy = len;
        // } else if bytes_remaining < len as usize {
        //     bytes_to_copy = bytes_remaining;
        // } else {
        //     return None;
        // }

        // let mut output = Vec::with_capacity(bytes_to_copy);

        // let start = self.read_cursor;
        // println!("Starting read at index {}", start);
        // println!("Read ahead {}", self.buf.len() - start);

        // if bytes_to_copy < self.buf.len() - start {
        // buf.extend(self.buf[]);
        // }

        let read_cursor = self.read_cursor.load(Ordering::SeqCst);

        let bytes_remaining = self.writer_cursor.load(Ordering::SeqCst) - read_cursor;
        //println!("Can read {}", bytes_remaining);

        let bytes_to_copy;
        if bytes_remaining >= len as usize {
            bytes_to_copy = len as usize;
        } else if bytes_remaining < len as usize {
            bytes_to_copy = bytes_remaining;
        } else {
            // EOF
            return None;
        }

        let mut buf = Vec::with_capacity(bytes_to_copy);

        unsafe {
            let vec = unsafe { &*self.buf.get() };

            let ptr = vec.as_ptr().add(read_cursor);
            std::ptr::copy(ptr, buf.as_mut_ptr(), bytes_to_copy);

            buf.set_len(bytes_to_copy);
        }

        self.read_cursor.fetch_add(bytes_to_copy, Ordering::SeqCst);

        Some(buf)
    }

    pub fn can_write(&self, len: usize) -> bool {
        true
    }

    pub fn write(&self, buf: &[u8]) {
        //println!("Write {} bytes", buf.len());

        let mut vec = unsafe { &mut *self.buf.get() };
        vec.extend(buf);
        self.writer_cursor.fetch_add(buf.len(), Ordering::SeqCst);
    }
}

unsafe impl Send for VideoBuffer {}
unsafe impl Sync for VideoBuffer {}
