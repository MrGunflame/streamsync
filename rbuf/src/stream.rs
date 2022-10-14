use std::ffi::{CStr, CString};

use libc::{c_int, c_uint, c_void, size_t};

use crate::{
    libvlc_instance_t, libvlc_media_new_path, libvlc_media_player_new_from_media,
    libvlc_media_player_play, libvlc_new,
};

pub struct Options {}

pub fn smem() {
    let instance = new_smem();

    let media = unsafe { libvlc_media_new_path(instance, super::PATH2.as_ptr() as *const i8) };

    let player = unsafe { libvlc_media_player_new_from_media(media) };

    unsafe {
        libvlc_media_player_play(player);
    }

    loop {}
}

fn new_smem() -> *mut libvlc_instance_t {
    let audio_postrender_callback = handle_stream as *const () as usize;
    let audio_prerender_callback = prepare_render as *const () as usize;
    println!("{:?}", audio_postrender_callback);
    println!("{:?}", audio_prerender_callback);

    let argv = format!("#transcode{{acodec=s16l}}:smem{{audio-postrender-callback={},audio-prerender-callback={}}}", audio_postrender_callback, audio_prerender_callback);

    println!("{:?}", argv);

    let smem_options = CString::new(argv).unwrap();

    let vlc = CString::new(String::from("vlc")).unwrap();
    let v = CString::new(String::from("--verbose=2")).unwrap();
    let out = CString::new(String::from("--sout")).unwrap();

    let args = [
        vlc.as_c_str().as_ptr(),
        v.as_c_str().as_ptr(),
        out.as_c_str().as_ptr(),
        smem_options.as_c_str().as_ptr(),
    ];

    // unsafe {
    //     libvlc_new(
    //         std::mem::size_of_val(&args) as c_int / std::mem::size_of_val(&args[0]) as c_int,
    //         args.as_ptr(),
    //     )
    // }

    unsafe { libvlc_new(4, args.as_ptr()) }
}

pub extern "C" fn prepare_render(
    p_audio_data: *const c_void,
    pp_pcm_buffer: *const *const u8,
    size: size_t,
) {
    println!(
        "prepare_render({:?}, {:?}, {:?})",
        p_audio_data, pp_pcm_buffer, size
    );
}

pub extern "C" fn handle_stream(
    p_audio_data: *const c_void,
    p_pcm_buffer: *const u8,
    channels: c_uint,
    rate: c_uint,
    nb_samples: c_uint,
    bits_per_sample: c_uint,
    size: size_t,
    pts: i64,
) {
    println!("handle_stream");
}
