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
    buf: Vec<u8>,
    read_cursor: usize,
    writer_cursor: usize,
    caps: Option<Caps>,
}

impl VideoBuffer {
    pub fn new() -> Self {
        Self {
            buf: vec![0; 1000 * 1000 * 1000],
            read_cursor: 0,
            writer_cursor: 0,
            caps: None,
        }
    }

    pub fn set_caps(&mut self, caps: Caps) {
        self.caps = Some(caps);
    }

    pub fn caps(&self) -> Option<&Caps> {
        self.caps.as_ref()
    }

    /// Returns `true` if there is at least len byte to read.
    pub fn can_read(&self, len: usize) -> bool {
        println!("{} - {}", self.writer_cursor, self.read_cursor);
        self.writer_cursor - self.read_cursor >= len
    }

    /// Reads `len` bytes from the buffer.
    /// Returns `None` if there are currently no bytes to read. Note that this does not mean that
    /// there will never any more bytes to read.
    pub fn read(&mut self, len: usize) -> Option<Vec<u8>> {
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

        let bytes_remaining = self.writer_cursor - self.read_cursor;
        println!("Can read {}", bytes_remaining);

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
            let ptr = self.buf.as_ptr().add(self.read_cursor);
            std::ptr::copy(ptr, buf.as_mut_ptr(), bytes_to_copy);

            buf.set_len(bytes_to_copy);
        }

        self.read_cursor += bytes_to_copy;

        Some(buf)
    }

    pub fn can_write(&self, len: usize) -> bool {
        true
    }

    pub fn write(&mut self, buf: &[u8]) {
        println!("Write {} bytes", buf.len());
        self.buf.extend(buf);
        self.writer_cursor += buf.len();
    }
}
