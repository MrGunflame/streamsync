use std::marker::PhantomData;
use std::sync::Arc;

use futures::StreamExt;
use gstreamer::{
    element_warning, prelude::*, ClockTime, Element, ElementFactory, MessageView, Pipeline, State,
};
use tracing::{event, span, Level};

use crate::buffer::VideoBuffer;
use crate::size::Size;

// const URI: &str = "https://video-weaver.frahttps://video-edge-a2c4ec.pdx01.abs.hls.ttvnw.net/v1/segment/Cl7rg1JWnRNFDwCSxdo1DRdXxTrn7PV-QU-ZAYDzS3fz_JGvf1TvWyHCScbFLLocXCtCgQhHyTXXVIMLffX6-S6JbmEUC4wlJUlPZOChjPo2RV8nsVW6iLyMMI6HYudC4FKEXX1-EM7JHISto3QOlATYcG9ijggZYsFAPEbGltjPGpxNJgK01vB24rQK4apYg0keQ7TqOrxUFMOumUpmyVWysGn7XrZ_VV9hnIkA1Uq6OT_6raYWTN6F0tsIEE1HnHmzoW6RIn7LsQsaTSCPug9ou-fAqkn7PXsLqbDnojREU1SRi-YGFN7fqKJQhSEZx6wIPk5peUPK0keuwIVNrg2Jax6lpe4qxljxujONnPnU6WntX2djuTQxE24i8_N7dloX1BrbXpfnjH3UoW0Nu6Z3oNDoyyiVUATZQsUahMrkygutWhH2bQVWJobwlN3tExoOasnp_P196-POeIQAlBWYWSOmfUomkdr5-ATWHkOLouYCbc0Pr_ozM2IOADqzzyO8toxZYgbhX8Q0XuMOA-40TFPh-jQAWeATJD01UglNDbC1qSh0aMV1MAdGpa23U6Ca4IzWTS7SykbPyjXaaCBt7Fe9jhbso9bOc0d07Rk1EGmM6yknLM1dhfC2kU_zmgRI1Li3KREli0rMnaGaz8VwcGmM8SmjfNzq9xCdTqPr07wXnreWz1CPCyKZzgu32o8umFGTn0OJ3SuwIK8L41egWT2t8DfJ6UwMWnx7wYLN6aiZyYjT_sIdV6kdCGLHnSkDLS.ts05.hls.ttvnw.net/v1/playlist/CqkF01yHUUB4u3NiYBayoR0lqA4pk-WRkBi_sIxZ7Zu4BuQvfDMtQ_kNOILboXs3y7lSZ4D_HBSi2Vbo2Znfo273bjMiEXpTeQIht5ED_k-YZXWv5PrLdFzIRcVh_NZoo82wiF31Yk2ES5bplFbDjbiz-Cnph731pwdFx362KjwVhi4iEI_GyFpLKFPrReEq169AroEADUACC5b68k5h6eWbAC11z2U-2lAYse_uQrw3v-yphfQPm4ctMyLHX_d5yyPpq0rlRR36ZZnsULmedAsUWXPTg7v8igpwGU9E32twd46dGPvXDc7An4z3bJqE-65B0XMKI_HU4ybVKJVDT5eIYS4aAwZnRsKI5d-1D8dJ-TsLhyGYOBw2PzspyiA21BrSKJgLgtMHjGa-zAF8zEcJ0XwnpxByGgOlPEG1M8RbDW9W80nVQoLKL3qpol9xrgpY44ApeoUIBNgP_Y5jlFOQIj-RzrXafUrXeznb2Qk6O5Z5TCVpKXjBG5jZagNRnd2laaitofEJxyMuVb4qNaav3Tt5HdtWcJ0-RK2PFgNzB3NIYVRM6Wg4ThC6uk3inP3GvyWDUVlDDuY00dDuUHHyl4dXg5wBhdWO7PXw-F9JGELbhoTiJqvq16klTrtL-nBYHxOBUYtmdz6dG3dATE7EnfkAQroX6xJEbPBAAq3Gopt2Rp7CHT0NPjfZNa3bcHFXv_liO52Aj2WAFsJtmou1X5y8Bw-DkN7DmKfag-NPeTLxbTKftFCbNg7eqYYP14m9I3-fQgCZhHpmQb7Ck8OwrR0Jw7mKvpJZTZMpL7WgvsMeyaSbOCcnoPJUFqTy6Sd7BU07oxITyzQY1Ph3_2GiZJz1ke627gTtEJAfq_8k_vhQ6cdP7_G3Ub7arE3iWZmSSI3cqo5HQgttGgzZSV46X_Dhy9aK-oMgASoJZXUtd2VzdC0yMOsE.m3u8";
// const URI:&str = "https://video-weaver.fra05.hls.ttvnw.net/v1/playlist/CrIFApzs6fCT1lJLhpOmmw2gR0FYLsMQzvaLQr_k41N9LGmI5E-3Sya2q7PX35eUJzR1Y3njJ-jVWnC4Y7VQj1D5DsOZj9WFKeKgwEjhat2gyPvUpbUNlCbG-YsY1ZQ_Ex9UiTJvpcfU1oP9ncnSataXtVeY4drXjeIlZiaYjd5ER-mDVHX6s_LHu4owf4_9iunDXt0Dga8id8DLupz_iYUJiOketiV818yFA2XDHyVMgxwatS4svqXv_WsyICStI1WTyWiH7jokZyp7ia4zQtNHnQr6x2vJn0UkcxnB0Wx-xuMOmOLPPa6o8BO_CeMbz_AWVhezpNwIecXs57dUi0yajByES6v3twbxV7tjfo08QFN-ewMqDkxgi8_Yn2IEgTXha4gVT-JEnCEPKRvN3fJZ9x2Ip_RWPxxF9_zKh5zuqvAA75LKXZRiaHcDyB1SjKAU-vq16uVa2KdAZbsD1ERpN-sZQUbqqDNX-lMzSqS4iX9SWyR9ISFm4SaVX7h4EUs5bIlm2TqR5_ammPCMBcvN0N0VqHrkzBxVwxAbs47LnJmq0DK9OodyGTMImC9aaSyI4MnNmsHPhbl8c2Nb05N7dI-EKWFwFd3PycQtgBb3HcchbPQBME8khxsBhIrXfjPHHATyvuypcxcrJXT0Gpgl0DQeS-4N7DHAdLc-gC3YWtJYhBzOhQGt4kA-D696WO2BhJqe9xhbmoXt9pgHjll0y7AvX7EMUQqtbYKDB2U_wgX-CC-Z00hSrJ1b1chtPjz3SNs1Xg_cGFQ1cLJO-3z_uo1C_HJJG2k2vHqVbdv0Opch5xJRhlA95H_jAk7I5AHkMb4Z7fELxlD7oAeX0HvGHdXMoY0q3OJRRxemKAzE49kq5-P-XHSWiDC-KSsMkvPJ0McPP_kuLx4XMjRAMJi_P2d9GgxhfPpW3A5_aQ4a_XcgASoJdXMtd2VzdC0yMOwE.m3u8";
const URI: &str = "http://devimages.apple.com/iphone/samples/bipbop/gear4/prog_index.m3u8";
// const URI: &str = "https://video-weaver.fra02.hls.ttvnw.net/v1/playlist/CpgFGa7wm2n7HeMy0VIdPTzeu04autpVsD0QeJj6RACGNwxqnyEcrxzMUquRa35VnLPVvqgHxy15sXNhSEA2za8fMs4b7zY-YfWv69bGEntaR3I6p-rKPs8eAZT4VuDmWcqHi-Fe9yJ_cCSSZM1bHsOlyXKK1_J8lNvhCIwvzwSoOc0TKisT_mhPYK6PS7ySIaTZE3Ap3hPD3KLGvhcnDCm7mz1nLPApraMPyPEqxiRNT5pD4FVRNvdizL_LPJlep-nHjiuRKqppNyY6bVK4Q_jE3XtkRj9r7lbon7OrmPRIgu8EeleBSZ946iyzKW9g5Xu8Ktsb41ySt0AzD8DNx4Ucd6ePqYKfRDNlPSVrl6t9QgNkXa9vNDHNnZKYVkYxAhuwrT6ce92qZHHxn2bE0mWn2A5crJ7AQEUQwhdfRrfdNqJImj24r-CzXe2TR-hMZl71faXOJ4nCD7R7q_EA2GSVW84itizI6KiK8rlD-HBZVMod4sglifOfW9A7cmohLiHQ9qOteSw_2p95PxMOoRZw6UMYzvK7BK9vbVQrZGi2MRF-nUojX8UpAPKt49K31zkx8oaDwRyYH55-Dz6u3AXR83Ja2oefxGG8SAOoQ6JpPHN05Qwr01oMduoZ9a7T8y_BlnH09O_L-5fIpaDfuBXifaHfk2RxxwMkPtMrMS3Dv6YCXiCHvbx75YMomwXccaqqMxf-EB3Yrvn3feyGt17axH19lj6zBezj1vRy0_HRja1RllECSMJA-drhksVKYz86KpzQFlQrmWncNqc1CUbHPtSY0AbA1LEVKTpla9m_Zo-dHjDNp4mZbAMJjqzHrxrEJIakXDoKT6WaO9n3bRVwYz6tvFKPN5aciSBnG83Vs6ILcHABfAezuBoMCa_nbYCj6tNcoYCEIAEqCWV1LXdlc3QtMjD0BA.m3u8";
// const URI: &str = "https://video-weaver.fra02.hls.ttvnw.net/v1/playlist/CqoFxrwx4iP7w-jEztUwjKYFfvOqmhGuMXuxSF5_7dVgEmIRDT9SzZyvSkR2cfsazzCMtBGPbhz8C37JsDA8fnZZ0G-u4XntR0qB39koarahdVSXbnk0IHTFUS6oDlTo26sVnH-KDTNejGheBlAoQ5mWD_jn_waMXy_Sb2uq-n12VYiPz4jArePyot-2cicXsScKOyR1gkyovojaPhCzxoVVUbbYYKXSVBj4SQoC7EnvM9eVL6waqfJlquzAO9Ozmw4zS56YoSIlZph4A9dSG4YXQGsgHlhW-nU_6fATN9iC9FS7fdmUkCZu6OSjoQMOXSvxhmmzbTgyl9YEEden_t4OO-Yvlx9KxyS4BEa5ZYeb78VznhW3Vo_kid_M-CQTkFjX6Rq-G_ZnBwulbcdNijOeX9BF23AiICYOG3FeX1IBlDWQt4M94PQ4SmsNkkTmb6Y4ylsQs_4bqxW4_T5VlPHmg1rtBkWm3KcfQIv_kZ2aIHX0lkfb_1pb3dKzN8MPafYyTL7mr7vwtupCMeUOrq_2AU0El5zzx7M23Xf-JMNrFrm69ZRGM9AkwuGNAZKTjD2QLzx_mT5t75hJCZ8kMz6sd5TX34y-Xork1K0kjF6umqsTqskYlJB2KGz_48Tt-g6VDqVf9VfBnkHo-Gcm3T5ii5PTArVFz9IDo6cf5VKaGbo2qOLKadCuWwD7WxsyJHOl_S-SNEakZEml3r1qhALdn8OiNu33s1ndWmeS0CIAB3lYVz4bnTYxRTNoP7BSVdNhqy_QpbxnhdpdEIkTFjtxHYv25Oxgp7Sz6odsz2yo-asxI-U-ewfVT465pLTKkkGhJn3Lvl2OY3QzFsbII_LstKvTUuBL48XmfYG8mxEIOcShwXy3Lx2kFmOIcop1SQpDFyVaQEt04qY0dxoMUqbMaXt7sLERHdtcIAEqCWV1LXdlc3QtMjD3BA.m3u8";

// -----------------------
// -- FIXME: TWITCH BUG --
//
// Interestingly the twitch HLS stream starts at PTS > 0 (e.g. around 20s), while the segment
// is scheduled Segment::start at PTS of the first frame (e.g. 20s). This causes the stream to only
// start playing at PTS + Segment::start (e.g. 40s) causing a PTS delay (e.g. 40s - 20s).

fn create_pipeline(buf: Arc<VideoBuffer>, src: crate::Source) -> Pipeline {
    let pipeline = Pipeline::new(None);

    let src = SoupHttpSrc::new(&src.location).unwrap();
    let hlsdemux = HlsDemux::new().unwrap();
    let decodebin = DecodeBin::new(&pipeline, buf).unwrap();

    pipeline
        .add_many(&[&src.element, &hlsdemux.element, &decodebin.element])
        .unwrap();

    src.link(&hlsdemux);

    hlsdemux.build(decodebin);

    pipeline
}

#[derive(Debug)]
pub struct FileSrc {
    element: Element,
}

impl FileSrc {
    pub fn new(location: &str) -> Result<Self, Error> {
        let element = create_element("filesrc")?;
        element.set_property("location", location);

        Ok(Self { element })
    }
}

impl<T> Source<T> for FileSrc where T: MediaFormat {}

impl AsRef<Element> for FileSrc {
    fn as_ref(&self) -> &Element {
        &self.element
    }
}

/// A raw HTTP fetch source element.
///
/// `Source<Http>`
///
/// https://gstreamer.freedesktop.org/documentation/soup/souphttpsrc.html
#[derive(Clone, Debug)]
pub struct SoupHttpSrc {
    element: Element,
}

impl SoupHttpSrc {
    pub fn new(location: &str) -> Result<Self, Error> {
        let element = create_element("souphttpsrc")?;
        element.set_property("location", location);

        Ok(Self { element })
    }
}

impl Source<Http> for SoupHttpSrc {}

impl AsRef<Element> for SoupHttpSrc {
    fn as_ref(&self) -> &Element {
        &self.element
    }
}

/// A `DecodeBin` element demuxes an encoded stream into raw video/audio data.
///
/// `Sink<Hls> => Source<PcmVideo>, Source<PcmAudio>`
///
/// https://gstreamer.freedesktop.org/documentation/playback/decodebin.html
#[derive(Clone, Debug)]
pub struct DecodeBin {
    element: gstreamer::Element,
}

impl DecodeBin {
    pub fn new(pipeline: &Pipeline, buf: Arc<VideoBuffer>) -> Result<Self, Error> {
        let element = create_element("decodebin")?;

        let span = span!(target: "decodebin", Level::TRACE, "");

        let pipeline = pipeline.downgrade();
        let buf_clone = buf.clone();
        element.connect_pad_added(move |dbin, src_pad| {
            event!(parent: &span, Level::TRACE, "connect_pad_added");

            let buf = buf_clone.clone();
            println!("{:?}", src_pad.caps());

            let pipeline = match pipeline.upgrade() {
                Some(pipeline) => pipeline,
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

            if is_audio {
                return;
            }

            let sink = AppSink::new(buf);
            let sink_el: &gstreamer::Element = sink.element.as_ref();
            let elements = [sink_el];
            pipeline.add_many(&elements).unwrap();
            //gstreamer::Element::link_many(&[&queue, &convert, &scale]).unwrap();

            for e in elements {
                e.sync_state_with_parent().unwrap();
            }

            let sink_pad = sink_el.static_pad("sink").expect("queue has no sinkpad");
            src_pad.link(&sink_pad).unwrap();

            // if is_audio {
            //     // let queue = ElementFactory::make("queue", None).unwrap();
            //     // let convert = ElementFactory::make("audioconvert", None).unwrap();
            //     // let resample = ElementFactory::make("audioresample", None).unwrap();
            //     // let sink = ElementFactory::make("autoaudiosink", None).unwrap();
            //     // let sink = AppSink::new(buf);
            //     // let sink_el: &gstreamer::Element = sink.element.as_ref();

            //     // // //let elements = [&queue, &convert, &resample, &sink];
            //     // let elements = [sink_el];
            //     // pipeline.add_many(&elements).unwrap();
            //     // gstreamer::Element::link_many(&elements).unwrap();

            //     // for e in elements {
            //     //     e.sync_state_with_parent().unwrap();
            //     // }

            //     // let sink_pad = sink_el.static_pad("sink").expect("queue has no sinkpad");
            //     // src_pad.link(&sink_pad).unwrap();
            // } else if is_video {
            //     buf.set_caps(src_pad.caps().unwrap());

            //     // let queue = ElementFactory::make("queue", None).unwrap();
            //     // let convert = ElementFactory::make("videoconvert", None).unwrap();
            //     // let scale = ElementFactory::make("videoscale", None).unwrap();
            //     // let sink = ElementFactory::make("autovideosink", None).unwrap();
            //     let sink = AppSink::new(buf);
            //     let sink_el: &gstreamer::Element = sink.element.as_ref();

            //     // let elements = [&queue, &convert, &scale, &sink];
            //     let elements = [sink_el];
            //     pipeline.add_many(&elements).unwrap();
            //     //gstreamer::Element::link_many(&[&queue, &convert, &scale]).unwrap();

            //     for e in elements {
            //         e.sync_state_with_parent().unwrap();
            //     }

            //     let sink_pad = sink_el.static_pad("sink").expect("queue has no sinkpad");
            //     src_pad.link(&sink_pad).unwrap();
            // }
        });

        Ok(Self { element })
    }
}

impl AsRef<Element> for DecodeBin {
    fn as_ref(&self) -> &Element {
        &self.element
    }
}

impl Sink<Hls> for DecodeBin {}

impl Source<PcmVideo> for DecodeBin {}
impl Source<PcmAudio> for DecodeBin {}

#[derive(Clone, Debug)]
pub struct HlsDemux {
    element: gstreamer::Element,
}

impl HlsDemux {
    pub fn new() -> Result<Self, Error> {
        let element = create_element("hlsdemux")?;

        Ok(Self { element })
    }

    pub fn build(&self, sink: DecodeBin) {
        let span = span!(target: "hlsdemux", Level::TRACE, "");

        self.element.connect_pad_added(move |demuxer, src_pad| {
            event!(parent: &span, Level::TRACE, "connect_pad_added");

            let sink_pad = sink.element.static_pad("sink").unwrap();
            src_pad.link(&sink_pad).unwrap();
        });

        self.element.sync_state_with_parent().unwrap();
    }
}

impl AsRef<Element> for HlsDemux {
    fn as_ref(&self) -> &Element {
        &self.element
    }
}

impl Sink<Http> for HlsDemux {}

impl Source<Hls> for HlsDemux {}

#[derive(Clone, Debug)]
pub struct AppSink {
    element: gstreamer_app::AppSink,
}

impl AppSink {
    pub fn new(buf: Arc<VideoBuffer>) -> Self {
        let sink = ElementFactory::make("appsink", None).unwrap();

        let span = span!(target: "appsink", Level::DEBUG, "");

        let appsink = sink.dynamic_cast::<gstreamer_app::AppSink>().unwrap();
        appsink.set_callbacks(
            gstreamer_app::AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    let sample = sink.pull_sample().unwrap();
                    event!(parent: &span, Level::DEBUG, "{:?}", sample);

                    buf.write_buf(sample);

                    Ok(gstreamer::FlowSuccess::Ok)
                })
                .build(),
        );

        Self { element: appsink }
    }
}

/// A queue of any [`MediaFormat`], blocking once full.
///
/// `Sink<T> => Source<T>`
///
/// https://gstreamer.freedesktop.org/documentation/coreelements/queue.html
#[derive(Clone, Debug)]
pub struct Queue<T>
where
    T: MediaFormat,
{
    element: Element,
    _marker: PhantomData<T>,
}

impl<T> Queue<T>
where
    T: MediaFormat,
{
    pub fn new(size: Size) -> Self {
        assert!(size.get() <= u32::MAX as usize);

        let element = ElementFactory::make("queue", None).unwrap();
        element.set_property("max-size-buffers", 0u32);
        element.set_property("max-size-bytes", size.to_u32());

        Self {
            element,
            _marker: PhantomData,
        }
    }
}

impl<T> AsRef<Element> for Queue<T>
where
    T: MediaFormat,
{
    fn as_ref(&self) -> &Element {
        &self.element
    }
}

impl<T> Sink<T> for Queue<T> where T: MediaFormat {}

impl<T> Source<T> for Queue<T> where T: MediaFormat {}

fn playback_pipeline(buf: Arc<VideoBuffer>) -> Pipeline {
    let pipeline = Pipeline::new(None);

    let src = ElementFactory::make("appsrc", None).unwrap();
    let queue = Queue::new(Size::gib(1));
    let sink = AutoVideoSink::new();

    let appsrc = src.downcast_ref::<gstreamer_app::AppSrc>().unwrap();
    appsrc.set_format(gstreamer::Format::Time);
    appsrc.set_is_live(true);
    appsrc.set_do_timestamp(true);
    appsrc.set_latency(Some(ClockTime::SECOND), Some(ClockTime::SECOND * 60));

    pipeline
        .add_many(&[&src, &queue.element, &sink.element])
        .unwrap();

    src.link(&queue.element).unwrap();
    queue.link(&sink);

    appsrc.set_callbacks(
        gstreamer_app::AppSrcCallbacks::builder()
            .need_data(move |appsrc, len| {
                tracing::trace!("[APPSRC] need_data({})", len);

                let sample = buf.read_buf();
                let _ = appsrc.push_sample(&sample);
            })
            .build(),
    );

    pipeline
}

#[derive(Debug)]
pub struct StreamPipeline {
    pl: Pipeline,
}

impl StreamPipeline {
    pub fn new(buf: Arc<VideoBuffer>, src: crate::Source) -> Self {
        Self {
            pl: create_pipeline(buf, src),
        }
    }

    pub async fn run(self) {
        pipline_run_async(&self.pl).await;
    }
}

#[derive(Debug)]
pub struct PlaybackPipeline {
    pl: Pipeline,
}

impl PlaybackPipeline {
    pub fn new(buf: Arc<VideoBuffer>) -> Self {
        Self {
            pl: playback_pipeline(buf),
        }
    }

    pub async fn run(self) {
        pipline_run_async(&self.pl).await;
    }
}

/// Runs a [`Pipeline`] until completion asynchronously.
async fn pipline_run_async(pipeline: &Pipeline) {
    pipeline.set_state(gstreamer::State::Playing).unwrap();

    let mut stream = pipeline.bus().unwrap().stream();

    while let Some(msg) = stream.next().await {
        match msg.view() {
            MessageView::Eos(_) => break,
            MessageView::Error(msg) => {
                panic!("{:?}", msg);
            }
            MessageView::Warning(msg) => {
                println!("[WARN] {:?}", msg);
            }
            MessageView::Info(msg) => {
                println!("[INFO] {:?}", msg);
            }
            MessageView::ClockLost(msg) => {
                println!("Clock lost: {:?}", msg);
            }
            MessageView::Buffering(msg) => {
                println!("Buffering {:?}", msg);
            }
            MessageView::Latency(msg) => {
                println!("Latency {:?}", msg);
            }
            _ => (),
        }
    }

    pipeline.set_state(State::Null).unwrap();
}

/// A raw HTTP response body stream.
struct Http;

struct Hls;

/// A gstreamer element that produces `T`.
pub trait Sink<T>: AsRef<Element> {}

/// A gstreamer element that consumes `T`.
pub trait Source<T>: AsRef<Element> {
    /// Links the source element to a fitting sink `S`.
    fn link<S>(&self, sink: &S)
    where
        S: Sink<T>,
    {
        // TODO: error handling
        self.as_ref().link(sink.as_ref()).unwrap();
    }
}

#[derive(Clone, Debug)]
pub struct AutoVideoSink {
    element: Element,
}

impl AutoVideoSink {
    pub fn new() -> Self {
        let element = ElementFactory::make("autovideosink", None).unwrap();

        Self { element }
    }
}

impl AsRef<Element> for AutoVideoSink {
    fn as_ref(&self) -> &Element {
        &self.element
    }
}

impl Sink<PcmVideo> for AutoVideoSink {}

pub trait MediaFormat {
    /// Returns `true` if the format encapsulates an audio stream.
    fn is_audio(&self) -> bool;

    /// Returns `true` if the format encapsulates a video stream.
    fn is_video(&self) -> bool;
}

pub trait AudioMediaFormat: MediaFormat {}

pub trait VideoMediaFormat: MediaFormat {}

/// Any raw PCM video format (e.g. I420 YUV).
pub struct PcmVideo {}

impl MediaFormat for PcmVideo {
    fn is_audio(&self) -> bool {
        false
    }

    fn is_video(&self) -> bool {
        true
    }
}

impl VideoMediaFormat for PcmVideo {}

/// Any raw PCM audio format (e.g. F32LE).
pub struct PcmAudio {}

impl MediaFormat for PcmAudio {
    fn is_audio(&self) -> bool {
        true
    }

    fn is_video(&self) -> bool {
        false
    }
}

impl AudioMediaFormat for PcmAudio {}

pub trait GstElement {
    const PACKAGE: &'static str;
}

#[derive(Debug)]
pub enum Error {
    MissingElement(&'static str),
}

fn create_element(name: &'static str) -> Result<Element, Error> {
    ElementFactory::make(name, None).map_err(|_| Error::MissingElement(name))
}

pub struct AppSrc<T>
where
    T: MediaFormat,
{
    element: gstreamer_app::AppSrc,
    _marker: PhantomData<T>,
}

impl<T> AppSrc<T>
where
    T: MediaFormat,
{
    pub fn new() -> Result<Self, Error> {
        let element = create_element("appsrc")?;
        let element = element.dynamic_cast().unwrap();

        Ok(Self {
            element,
            _marker: PhantomData,
        })
    }
}

impl<T> AsRef<Element> for AppSrc<T>
where
    T: MediaFormat,
{
    fn as_ref(&self) -> &Element {
        self.element.as_ref()
    }
}

impl<T> Source<T> for AppSrc<T> where T: MediaFormat {}
