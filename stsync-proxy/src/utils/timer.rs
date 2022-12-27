//! Shared async timer utilities

use std::cell::UnsafeCell;
use std::future::Future;
use std::marker::PhantomPinned;
use std::pin::Pin;
use std::task::{ready, Context, Poll, Waker};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use tokio::time::MissedTickBehavior;

pub struct Interval {
    interval: Mutex<tokio::time::Interval>,
    // TODO: Get rid of the Vec and heap allocation and use a stack-allocated
    // intrusive LL.
    waiters: Mutex<Vec<Waiter>>,
}

impl Interval {
    pub fn new(period: Duration) -> Self {
        let mut interval = tokio::time::interval(period.into());
        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

        Self {
            interval: Mutex::new(interval),
            waiters: Mutex::new(Vec::new()),
        }
    }

    pub fn tick(&self) -> Tick<'_> {
        Tick {
            interval: self,
            waiter: UnsafeCell::new(Waiter::new()),
            state: State::Init,
        }
    }

    fn poll_internal(&self, cx: &mut Context<'_>) -> Poll<()> {
        let mut interval = self.interval.lock();
        let instant = ready!(interval.poll_tick(cx));

        let mut waiters = self.waiters.lock();
        for waiter in &mut *waiters {
            waiter.instant = Some(instant.into_std());

            if let Some(waker) = &waiter.waker {
                waker.wake_by_ref();
            }
        }

        Poll::Ready(())
    }
}

pub struct Tick<'a> {
    interval: &'a Interval,
    // Must lock `self.interval.waiters` before accessing.
    waiter: UnsafeCell<Waiter>,
    state: State,
}

impl<'a> Future for Tick<'a> {
    type Output = Instant;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let State::Done(instant) = self.state {
            return Poll::Ready(instant);
        }

        let mut waiters = self.interval.waiters.lock();

        let mut waiter = unsafe { &mut *self.waiter.get() };

        match self.state {
            State::Init => {
                if let Some(instant) = &waiter.instant {
                    self.state = State::Done(instant);
                    return Poll::Ready(instant);
                }
            }
            State::Pending => {}
            _ => unreachable!(),
        }
    }
}

pub struct Waiter {
    waker: Option<Waker>,
    instant: Option<Instant>,
    _pin: PhantomPinned,
}

impl Waiter {
    #[inline]
    fn new() -> Self {
        Self {
            waker: None,
            instant: None,
            _pin: PhantomPinned,
        }
    }
}

enum State {
    Init,
    Pending,
    Done(Instant),
}
