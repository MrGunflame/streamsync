mod buffer;
mod player;
mod source;
mod stream;

use libc::{c_char, c_int, c_void};

use std::{
    ffi::{CStr, CString},
    io::Read,
    mem::MaybeUninit,
    sync::{Arc, Mutex},
};

use crate::buffer::VideoBuffer;

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
        seek_cb: *mut libvlc_media_seek_cb,
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
    extern "C" fn(opaque: *mut c_void, datap: *mut *const c_void, sizep: *mut u64) -> c_int;
type libvlc_media_read_cb = extern "C" fn(opaque: *mut c_void, buf: *mut u8, len: isize) -> isize;
type libvlc_media_seek_cb = extern "C" fn(opaque: *mut c_void, offset: u64) -> c_int;
type libvlc_media_close_cb = extern "C" fn(opaque: *mut c_void);

fn main() {
    let mut buffer = Arc::new(VideoBuffer::new());

    // let mut buf = Vec::new();
    // std::fs::File::open(PATH2)
    //     .unwrap()
    //     .read_to_end(&mut buf)
    //     .unwrap();
    // buffer.lock().unwrap().write(&buf);

    // stream::smem();
    source::stream(buffer.clone());

    loop {
        std::thread::sleep_ms(10000);
    }

    // println!("FILE: {:?}", buf.len());

    // let video = Box::leak(Box::new(Video::new(buf))) as *const Video;
    // println!("VIDEO ADDR: {:?}", video);

    let v = CString::new(String::from("--verbose=2")).unwrap();

    let args = [v.as_c_str().as_ptr()];

    let instance = unsafe { libvlc_new(1, args.as_ptr()) };
    println!("{:?}", instance);

    // println!("inst");

    // let media = unsafe {
    //     libvlc_media_new_callbacks(
    //         instance,
    //         open_cb,
    //         read_cb,
    //         seek_cb,
    //         close_cb,
    //         video as *const c_void,
    //     )
    // };

    // let media = Video::new(buf.clone()).into_media(instance);
    // let media = media_from_buffer(buffer, instance);

    // let player1 = unsafe { libvlc_media_player_new_from_media(media) };
    // println!("{:?}", player1);

    // unsafe { libvlc_media_player_play(player1) };

    // let media = Video::new(buf).into_media(instance);

    // let player2 = unsafe { libvlc_media_player_new_from_media(media) };
    // println!("{:?}", player2);

    // unsafe { libvlc_media_player_play(player2) };

    loop {}
    // std::thread::sleep_ms(1000000000);
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

// https://videolan.videolan.me/vlc/group__libvlc__media.html#ga4dbbb5d64230beacae95ea9bc00e4104
pub extern "C" fn open_cb(
    opaque: *mut c_void,
    datap: *mut *const c_void,
    sizep: *mut u64,
) -> c_int {
    println!(
        "open_cb(opaque: {:?}, datap: {:?}, sizep: {:?})",
        opaque, datap, sizep
    );

    // let video = unsafe { &mut *(opaque as *mut Video) };

    unsafe {
        *datap = opaque;
        *sizep = u64::MAX;
    }

    0
}

pub extern "C" fn read_cb(opaque: *mut c_void, buf: *mut u8, len: isize) -> isize {
    println!(
        "read_cb(opaque: {:?}, buf: {:?}, len: {:?})",
        opaque, buf, len
    );

    let mux = unsafe { &*(opaque as *mut Arc<Mutex<VideoBuffer>>) };

    let mut video = mux.lock().unwrap();
    let src = loop {
        if !video.can_read(len as usize) {
            println!("Cant read {}, waiting 1ms", len);
            drop(video);
            println!("drop");
            std::thread::sleep_ms(1000);
            video = mux.lock().unwrap();
            continue;
        }

        break video.read(len as usize).unwrap();
    };

    println!("Got {} bytes", src.len());

    unsafe {
        std::ptr::copy_nonoverlapping(src.as_ptr(), buf, src.len());
    }

    src.len() as isize
}

pub extern "C" fn seek_cb(opaque: *mut c_void, offset: u64) -> c_int {
    println!("seek_cb(opaque: {:?}, offset: {:?})", opaque, offset);

    let video = unsafe { &mut *(opaque as *mut Arc<Mutex<VideoBuffer>>) };

    // video.seek(offset);

    0
}

pub extern "C" fn close_cb(opaque: *mut c_void) {
    println!("close_cb(opaque: {:?})", opaque);

    unsafe {
        drop(Box::from_raw(opaque as *mut Arc<Mutex<VideoBuffer>>));
    }
}

#[derive(Debug)]
pub struct Video {
    buf: Vec<u8>,
    head: usize,
}

impl Video {
    pub fn new(buf: Vec<u8>) -> Self {
        Self { buf, head: 0 }
    }

    fn into_media(self, instance: *mut libvlc_instance_t) -> *mut libvlc_media_t {
        let video = Box::leak(Box::new(self)) as *mut Self;

        unsafe {
            libvlc_media_new_callbacks(
                instance,
                open_cb,
                read_cb,
                0 as *mut _,
                close_cb,
                video as *const c_void,
            )
        }
    }

    pub unsafe fn read(&mut self, buf: *mut u8, len: isize) -> isize {
        let bytes_remaining = self.buf.len() - self.head;

        let bytes_to_copy;
        if bytes_remaining >= len as usize {
            bytes_to_copy = len as usize;
        } else if bytes_remaining < len as usize {
            bytes_to_copy = bytes_remaining;
        } else {
            // EOF
            return 0;
        }

        let ptr = self.buf.as_ptr().add(self.head);
        std::ptr::copy(ptr, buf, bytes_to_copy);

        self.head += bytes_to_copy;

        bytes_to_copy as isize
    }

    pub fn seek(&mut self, offset: u64) -> c_int {
        self.head = offset as usize;
        0
    }
}

fn media_from_buffer(
    buffer: Arc<Mutex<VideoBuffer>>,
    instance: *mut libvlc_instance_t,
) -> *mut libvlc_media_t {
    let video = Box::leak(Box::new(buffer)) as *const _;

    unsafe {
        libvlc_media_new_callbacks(
            instance,
            open_cb,
            read_cb,
            0 as *mut _,
            close_cb,
            video as *const c_void,
        )
    }
}
