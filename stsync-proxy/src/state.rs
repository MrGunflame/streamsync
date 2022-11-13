use crate::session::buffer::BufferSessionManager;
use crate::srt;

pub struct State {}

pub struct StateInner {
    srt: srt::state::State<BufferSessionManager>,
}
