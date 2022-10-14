use std::sync::{Arc, Mutex};

use gstreamer::{element_error, element_warning, prelude::*, ElementFactory, Pipeline};

use crate::buffer::VideoBuffer;

// const URI: &str = "https://video-weaver.frahttps://video-edge-a2c4ec.pdx01.abs.hls.ttvnw.net/v1/segment/Cl7rg1JWnRNFDwCSxdo1DRdXxTrn7PV-QU-ZAYDzS3fz_JGvf1TvWyHCScbFLLocXCtCgQhHyTXXVIMLffX6-S6JbmEUC4wlJUlPZOChjPo2RV8nsVW6iLyMMI6HYudC4FKEXX1-EM7JHISto3QOlATYcG9ijggZYsFAPEbGltjPGpxNJgK01vB24rQK4apYg0keQ7TqOrxUFMOumUpmyVWysGn7XrZ_VV9hnIkA1Uq6OT_6raYWTN6F0tsIEE1HnHmzoW6RIn7LsQsaTSCPug9ou-fAqkn7PXsLqbDnojREU1SRi-YGFN7fqKJQhSEZx6wIPk5peUPK0keuwIVNrg2Jax6lpe4qxljxujONnPnU6WntX2djuTQxE24i8_N7dloX1BrbXpfnjH3UoW0Nu6Z3oNDoyyiVUATZQsUahMrkygutWhH2bQVWJobwlN3tExoOasnp_P196-POeIQAlBWYWSOmfUomkdr5-ATWHkOLouYCbc0Pr_ozM2IOADqzzyO8toxZYgbhX8Q0XuMOA-40TFPh-jQAWeATJD01UglNDbC1qSh0aMV1MAdGpa23U6Ca4IzWTS7SykbPyjXaaCBt7Fe9jhbso9bOc0d07Rk1EGmM6yknLM1dhfC2kU_zmgRI1Li3KREli0rMnaGaz8VwcGmM8SmjfNzq9xCdTqPr07wXnreWz1CPCyKZzgu32o8umFGTn0OJ3SuwIK8L41egWT2t8DfJ6UwMWnx7wYLN6aiZyYjT_sIdV6kdCGLHnSkDLS.ts05.hls.ttvnw.net/v1/playlist/CqkF01yHUUB4u3NiYBayoR0lqA4pk-WRkBi_sIxZ7Zu4BuQvfDMtQ_kNOILboXs3y7lSZ4D_HBSi2Vbo2Znfo273bjMiEXpTeQIht5ED_k-YZXWv5PrLdFzIRcVh_NZoo82wiF31Yk2ES5bplFbDjbiz-Cnph731pwdFx362KjwVhi4iEI_GyFpLKFPrReEq169AroEADUACC5b68k5h6eWbAC11z2U-2lAYse_uQrw3v-yphfQPm4ctMyLHX_d5yyPpq0rlRR36ZZnsULmedAsUWXPTg7v8igpwGU9E32twd46dGPvXDc7An4z3bJqE-65B0XMKI_HU4ybVKJVDT5eIYS4aAwZnRsKI5d-1D8dJ-TsLhyGYOBw2PzspyiA21BrSKJgLgtMHjGa-zAF8zEcJ0XwnpxByGgOlPEG1M8RbDW9W80nVQoLKL3qpol9xrgpY44ApeoUIBNgP_Y5jlFOQIj-RzrXafUrXeznb2Qk6O5Z5TCVpKXjBG5jZagNRnd2laaitofEJxyMuVb4qNaav3Tt5HdtWcJ0-RK2PFgNzB3NIYVRM6Wg4ThC6uk3inP3GvyWDUVlDDuY00dDuUHHyl4dXg5wBhdWO7PXw-F9JGELbhoTiJqvq16klTrtL-nBYHxOBUYtmdz6dG3dATE7EnfkAQroX6xJEbPBAAq3Gopt2Rp7CHT0NPjfZNa3bcHFXv_liO52Aj2WAFsJtmou1X5y8Bw-DkN7DmKfag-NPeTLxbTKftFCbNg7eqYYP14m9I3-fQgCZhHpmQb7Ck8OwrR0Jw7mKvpJZTZMpL7WgvsMeyaSbOCcnoPJUFqTy6Sd7BU07oxITyzQY1Ph3_2GiZJz1ke627gTtEJAfq_8k_vhQ6cdP7_G3Ub7arE3iWZmSSI3cqo5HQgttGgzZSV46X_Dhy9aK-oMgASoJZXUtd2VzdC0yMOsE.m3u8";
// const URI:&str = "https://video-weaver.fra05.hls.ttvnw.net/v1/playlist/CrIFApzs6fCT1lJLhpOmmw2gR0FYLsMQzvaLQr_k41N9LGmI5E-3Sya2q7PX35eUJzR1Y3njJ-jVWnC4Y7VQj1D5DsOZj9WFKeKgwEjhat2gyPvUpbUNlCbG-YsY1ZQ_Ex9UiTJvpcfU1oP9ncnSataXtVeY4drXjeIlZiaYjd5ER-mDVHX6s_LHu4owf4_9iunDXt0Dga8id8DLupz_iYUJiOketiV818yFA2XDHyVMgxwatS4svqXv_WsyICStI1WTyWiH7jokZyp7ia4zQtNHnQr6x2vJn0UkcxnB0Wx-xuMOmOLPPa6o8BO_CeMbz_AWVhezpNwIecXs57dUi0yajByES6v3twbxV7tjfo08QFN-ewMqDkxgi8_Yn2IEgTXha4gVT-JEnCEPKRvN3fJZ9x2Ip_RWPxxF9_zKh5zuqvAA75LKXZRiaHcDyB1SjKAU-vq16uVa2KdAZbsD1ERpN-sZQUbqqDNX-lMzSqS4iX9SWyR9ISFm4SaVX7h4EUs5bIlm2TqR5_ammPCMBcvN0N0VqHrkzBxVwxAbs47LnJmq0DK9OodyGTMImC9aaSyI4MnNmsHPhbl8c2Nb05N7dI-EKWFwFd3PycQtgBb3HcchbPQBME8khxsBhIrXfjPHHATyvuypcxcrJXT0Gpgl0DQeS-4N7DHAdLc-gC3YWtJYhBzOhQGt4kA-D696WO2BhJqe9xhbmoXt9pgHjll0y7AvX7EMUQqtbYKDB2U_wgX-CC-Z00hSrJ1b1chtPjz3SNs1Xg_cGFQ1cLJO-3z_uo1C_HJJG2k2vHqVbdv0Opch5xJRhlA95H_jAk7I5AHkMb4Z7fELxlD7oAeX0HvGHdXMoY0q3OJRRxemKAzE49kq5-P-XHSWiDC-KSsMkvPJ0McPP_kuLx4XMjRAMJi_P2d9GgxhfPpW3A5_aQ4a_XcgASoJdXMtd2VzdC0yMOwE.m3u8";
const URI: &str = "http://devimages.apple.com/iphone/samples/bipbop/gear4/prog_index.m3u8";

pub fn stream(buf: Arc<Mutex<VideoBuffer>>) {
    std::thread::spawn(move || {
        let pipeline = create_pipeline(buf);
        main_loop(pipeline);
    });
}

fn create_pipeline(buf: Arc<Mutex<VideoBuffer>>) -> Pipeline {
    gstreamer::init().unwrap();

    let pipeline = Pipeline::new(None);

    let src = gstreamer::ElementFactory::make("souphttpsrc", None).unwrap();
    src.set_property("location", URI);

    // let src = gstreamer::ElementFactory::make("filesrc", None).unwrap();
    // src.set_property("location", super::PATH2);

    let queue = ElementFactory::make("multiqueue", None).unwrap();
    queue.set_property("max-size-buffers", 0u32);
    queue.set_property("max-size-time", 0u64);
    queue.set_property("max-size-bytes", 1024u32 * 1024 * 100);

    let hlsdemux = gstreamer::ElementFactory::make("hlsdemux", None).unwrap();

    let decodebin = ElementFactory::make("decodebin", None).unwrap();

    // let src = gstreamer::ElementFactory::make("audiotestsrc", None).unwrap();

    let sink = gstreamer::ElementFactory::make("appsink", None).unwrap();
    // let sink = ElementFactory::make("filesink", None).unwrap();
    // sink.set_property("location", "/tmp/test.ts");

    pipeline
        .add_many(&[&src, &queue, &hlsdemux, &decodebin])
        .unwrap();

    src.link(&hlsdemux).unwrap();

    // src.link(&hlsdemux).unwrap();
    // queue.link(&hlsdemux).unwrap();
    // src.link(&hlsdemux).unwrap();
    // hlsdemux.link(&sink).unwrap();

    let muxer_clone = decodebin.clone();
    let queue_clone = queue.clone();
    hlsdemux.connect_pad_added(move |demux, src_pad| {
        handle_demux_pad_added(demux, src_pad, &queue_clone, &muxer_clone);
    });
    hlsdemux.sync_state_with_parent().unwrap();

    // let appsink = sink.dynamic_cast::<gstreamer_app::AppSink>().unwrap();

    // appsink.set_caps();
    //appsink.set_caps(Some(&gstreamer::Caps::builder("application/x-hls").build()));

    // DECODEBIN
    let pipeline_weak = pipeline.downgrade();
    decodebin.connect_pad_added(move |dbin, src_pad| {
        println!("[DECODEBIN] connect_pad_added");

        let pipeline = match pipeline_weak.upgrade() {
            Some(p) => p,
            None => return,
        };

        let (is_audio, is_video) = {
            let media_type = src_pad.current_caps().and_then(|caps| {
                caps.structure(0).map(|s| {
                    let name = s.name();
                    (name.starts_with("audio/"), name.starts_with("video/"))
                })
            });

            match media_type {
                None => {
                    element_warning!(
                        dbin,
                        gstreamer::CoreError::Negotiation,
                        ("Failed to get media type from pad {}", src_pad.name())
                    );

                    return;
                }
                Some(media_type) => media_type,
            }
        };

        println!("{}, {}", is_audio, is_video);

        if is_audio {
            let queue = ElementFactory::make("queue", None).unwrap();
            let convert = ElementFactory::make("audioconvert", None).unwrap();
            let resample = ElementFactory::make("audioresample", None).unwrap();
            let sink = ElementFactory::make("autoaudiosink", None).unwrap();

            let elements = [&queue, &convert, &resample, &sink];
            pipeline.add_many(&elements).unwrap();
            gstreamer::Element::link_many(&elements).unwrap();

            for e in elements {
                e.sync_state_with_parent().unwrap();
            }

            let sink_pad = queue.static_pad("sink").expect("queue has no sinkpad");
            src_pad.link(&sink_pad).unwrap();
        } else if is_video {
            let queue = ElementFactory::make("queue", None).unwrap();
            let convert = ElementFactory::make("videoconvert", None).unwrap();
            let scale = ElementFactory::make("videoscale", None).unwrap();
            let sink = ElementFactory::make("autovideosink", None).unwrap();

            let elements = [&queue, &convert, &scale, &sink];
            pipeline.add_many(&elements).unwrap();
            gstreamer::Element::link_many(&elements).unwrap();

            for e in elements {
                e.sync_state_with_parent().unwrap();
            }

            let sink_pad = queue.static_pad("sink").expect("queue has no sinkpad");
            src_pad.link(&sink_pad).unwrap();
        }
    });

    // appsink.set_callbacks(
    //     gstreamer_app::AppSinkCallbacks::builder()
    //         .new_sample(move |appsink| {
    //             // println!("Got data");
    //             let sample = appsink.pull_sample().unwrap();

    //             let buffer = sample
    //                 .buffer()
    //                 .ok_or_else(|| {
    //                     element_error!(
    //                         appsink,
    //                         gstreamer::ResourceError::Failed,
    //                         ("Failed to get buffer from appsink")
    //                     );
    //                 })
    //                 .unwrap();

    //             let map = buffer
    //                 .map_readable()
    //                 .map_err(|_| {
    //                     element_error!(
    //                         appsink,
    //                         gstreamer::ResourceError::Failed,
    //                         ("Failed to map buffer readable")
    //                     );
    //                 })
    //                 .unwrap();

    //             // println!("{:?}", map);

    //             buf.lock().unwrap().write(&map);

    //             Ok(gstreamer::FlowSuccess::Ok)
    //         })
    //         .build(),
    // );

    pipeline
}

fn main_loop(pipeline: Pipeline) {
    pipeline.set_state(gstreamer::State::Playing).unwrap();

    let bus = pipeline.bus().unwrap();

    for msg in bus.iter_timed(gstreamer::ClockTime::NONE) {
        use gstreamer::MessageView;

        match msg.view() {
            MessageView::Eos(..) => {
                println!("eof");
            }
            MessageView::Error(err) => {
                pipeline.set_state(gstreamer::State::Null).unwrap();

                panic!("{:?}", err);
            }
            MessageView::Warning(w) => {
                println!("[WARN] {:?}", w);
            }
            MessageView::Info(msg) => {
                println!("[INFO] {:?}", msg);
            }
            // MessageView::StateChanged(s) => {
            //     println!("state_changed");
            // }
            _ => (),
        }
    }

    pipeline.set_state(gstreamer::State::Null).unwrap();
}

fn handle_demux_pad_added(
    demuxer: &gstreamer::Element,
    demux_src_pad: &gstreamer::Pad,
    queue: &gstreamer::Element,
    muxer: &gstreamer::Element,
) {
    println!("handle_demux_pad_added");

    let queue_sink_pad = queue.request_pad_simple("sink_%u").unwrap();
    demux_src_pad.link(&queue_sink_pad).unwrap();

    let queue_src_pad = queue_sink_pad
        .iterate_internal_links()
        .next()
        .unwrap()
        .unwrap();

    let muxer_sink_pad = muxer.compatible_pad(&queue_src_pad, None).unwrap();

    queue_src_pad.link(&muxer_sink_pad).unwrap();
}
