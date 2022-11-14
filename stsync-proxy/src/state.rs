use std::ops::Deref;
use std::sync::Arc;

use crate::database::Database;
use crate::session::buffer::BufferSessionManager;
use crate::srt;

#[derive(Clone, Debug)]
pub struct State(Arc<StateInner>);

impl State {
    pub fn new(srt: srt::state::State<BufferSessionManager>) -> Self {
        Self(Arc::new(StateInner {
            db: Database::new(),
            srt,
        }))
    }
}

impl Deref for State {
    type Target = StateInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug)]
pub struct StateInner {
    pub srt: srt::state::State<BufferSessionManager>,
    pub db: Database,
}
