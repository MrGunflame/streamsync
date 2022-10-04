use libc::{c_char, c_int, c_void};

use std::{
    ffi::{CStr, CString},
    io::Read,
    mem::MaybeUninit,
};

const PATH: &str = "/home/robert/Documents/Music/Stellaris/ridingthesolarwind.ogg";
const PATH2: &str = "/home/robert/Videos/kiss.mp4";

#[link(name = "vlc")]
extern "C" {
    fn libvlc_new(argc: c_int, argv: *const *const c_char) -> *mut libvlc_instance_t;
    fn libvlc_media_new_path(
        instance: *mut libvlc_instance_t,
        path: *const c_char,
    ) -> *mut libvlc_media_t;

    fn libvlc_media_player_new_from_media(media: *mut libvlc_media_t)
        -> *mut libvlc_media_player_t;

    fn libvlc_media_new_callbacks(
        instance: *mut libvlc_instance_t,
        open_cb: libvlc_media_open_cb,
        read_cb: libvlc_media_read_cb,
        seek_cb: libvlc_media_seek_cb,
        close_cb: libvlc_media_close_cb,
        opaque: *const c_void,
    ) -> *mut libvlc_media_t;

    fn libvlc_media_player_play(player: *mut libvlc_media_player_t) -> c_int;
    fn libvlc_media_player_pause(player: *mut libvlc_media_player_t);

    fn libvlc_media_release(media: *mut libvlc_media_t);
}

enum libvlc_instance_t {}

enum libvlc_media_player_t {}

enum libvlc_media_t {}

type libvlc_media_open_cb =
    extern "C" fn(opaque: *const c_void, datap: *mut *const c_void, sizep: *mut u64) -> c_int;
type libvlc_media_read_cb = extern "C" fn(opaque: *const c_void, buf: *mut u8, len: isize) -> isize;
type libvlc_media_seek_cb = extern "C" fn(opaque: *const c_void, offset: u64) -> c_int;
type libvlc_media_close_cb = extern "C" fn(opaque: *const c_void);

fn main() {
    let mut buf = Vec::new();
    std::fs::File::open(PATH2)
        .unwrap()
        .read_to_end(&mut buf)
        .unwrap();

    println!("FILE: {:?}", buf.len());

    unsafe {
        BUFFER.write(buf);
        HEAD = 0;
    }

    let instance = unsafe { libvlc_new(0, std::ptr::null()) };
    println!("{:?}", instance);

    println!("inst");

    let media = unsafe {
        libvlc_media_new_callbacks(
            instance,
            open_cb,
            read_cb,
            seek_cb,
            close_cb,
            0 as *const c_void,
        )
    };

    println!("cb");

    let player = unsafe { libvlc_media_player_new_from_media(media) };
    println!("{:?}", player);

    unsafe { libvlc_media_player_play(player) };

    loop {}
}

fn it_works() {
    let instance = unsafe { libvlc_new(0, std::ptr::null()) };
    println!("{:?}", instance);

    let cstr = CString::new(PATH).unwrap();
    let media = unsafe { libvlc_media_new_path(instance, cstr.as_c_str().as_ptr()) };
    println!("{:?}", media);

    let player = unsafe { libvlc_media_player_new_from_media(media) };
    println!("{:?}", player);

    unsafe { libvlc_media_player_play(player) };

    loop {}
}

static mut BUFFER: MaybeUninit<Vec<u8>> = MaybeUninit::uninit();
static mut HEAD: usize = 0;

// https://videolan.videolan.me/vlc/group__libvlc__media.html#ga4dbbb5d64230beacae95ea9bc00e4104
pub extern "C" fn open_cb(
    opaque: *const c_void,
    datap: *mut *const c_void,
    sizep: *mut u64,
) -> c_int {
    println!(
        "open_cb(opaque: {:?}, datap: {:?}, sizep: {:?})",
        opaque, datap, sizep
    );

    unsafe {
        *datap = BUFFER.assume_init_ref().as_ptr() as *const c_void;
        *sizep = BUFFER.assume_init_ref().len() as u64;
    }

    0
}

pub extern "C" fn read_cb(opaque: *const c_void, buf: *mut u8, len: isize) -> isize {
    println!(
        "read_cb(opaque: {:?}, buf: {:?}, len: {:?})",
        opaque, buf, len
    );

    let buffer = unsafe { BUFFER.assume_init_ref() };
    let head = unsafe { HEAD };

    let mut bytes_to_copy = 0;
    let bytes_so_far = 0;
    let bytes_remaining = buffer.len() - head;

    if bytes_remaining >= len as usize {
        bytes_to_copy = len as usize;
    } else if bytes_remaining < len as usize {
        bytes_to_copy = bytes_remaining;
    } else {
        // EOF
        return 0;
    }

    let start = buffer.as_ptr();
    let ptr = unsafe { start.add(head) };
    unsafe {
        std::ptr::copy(ptr, buf, bytes_to_copy);
    }

    unsafe {
        HEAD += bytes_to_copy;
    }

    bytes_to_copy as isize
}

pub extern "C" fn seek_cb(opaque: *const c_void, offset: u64) -> c_int {
    println!("seek_cb(opaque: {:?}, offset: {:?})", opaque, offset);

    unsafe {
        HEAD = offset as usize;
    }

    0
}

pub extern "C" fn close_cb(opaque: *const c_void) {
    println!("close_cb(opaque: {:?})", opaque);
}

#[derive(Debug)]
pub struct Video {
    buf: Vec<u8>,
    head: usize,
}

impl Video {
    pub unsafe fn read(&mut self, buf: *mut u8, len: isize) -> isize {
        let bytes_remaining = self.buf.len() - self.head;

        let mut bytes_to_copy = 0;
    }
}
