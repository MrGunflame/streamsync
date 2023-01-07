//! Shutdown signaling
//!
//! This module provides utilities for gracefully shutting down the server. For consumers,
//! [`ShutdownListener`] is a future that completes once the shutdown signal has been received.
//! The main process will only exit once all [`ShutdownListener`]s have been dropped.
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::task::{Context, Poll};

use pin_project::{pin_project, pinned_drop};
use tokio::sync::futures::Notified;
use tokio::sync::Notify;

/// The global [`Shutdown`] signal handler.
pub static SHUTDOWN: Shutdown = Shutdown::new();

/// A listener for a shutdown signal.
///
/// `ShutdownListener` is a future that completes once a shutdown signal has been received. Once
/// completed all future calls to [`Future::poll`] return immediately.
///
/// `ShutdownListener` also serves as a RAII, the main process will only exit once all listeners
/// have been dropped.
#[pin_project(PinnedDrop)]
pub struct ShutdownListener {
    #[pin]
    notify: Notified<'static>,
}

impl ShutdownListener {
    /// Creates a new `ShutdownListener`.
    #[inline]
    pub fn new() -> Self {
        SHUTDOWN.listen()
    }

    /// Returns `true` if a shutdown is already in progress.
    ///
    /// Note that if this function returns `true`, all future calls to `poll` will return
    /// immediately.
    #[inline]
    pub fn in_progress(&self) -> bool {
        SHUTDOWN.in_progress.load(Ordering::Acquire)
    }
}

impl Future for ShutdownListener {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.in_progress() {
            return Poll::Ready(());
        }

        self.project().notify.poll(cx)
    }
}

impl Default for ShutdownListener {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

#[pinned_drop]
impl PinnedDrop for ShutdownListener {
    #[inline]
    fn drop(self: Pin<&mut Self>) {
        SHUTDOWN.dec();
    }
}

/// A shutdown handler.
pub struct Shutdown {
    in_progress: AtomicBool,
    counter: AtomicUsize,
    notify_shutdown: Notify,
    notify_done: Notify,
}

impl Shutdown {
    /// Creates a new `Shutdown` handler.
    const fn new() -> Self {
        Self {
            in_progress: AtomicBool::new(false),
            counter: AtomicUsize::new(0),
            notify_shutdown: Notify::const_new(),
            notify_done: Notify::const_new(),
        }
    }

    /// Increments the internal counter of listeners.
    #[inline]
    fn inc(&self) {
        self.counter.fetch_add(1, Ordering::AcqRel);
    }

    /// Decrements the internal counter of listeners.
    #[inline]
    fn dec(&self) {
        self.counter.fetch_sub(1, Ordering::AcqRel);

        if self.in_progress.load(Ordering::Acquire) && self.counter.load(Ordering::Acquire) == 0 {
            self.notify_done.notify_waiters();
        }
    }

    /// Creates a new [`ShutdownListener`], waiting for signals on this `Shutdown` instance.
    #[inline]
    pub fn listen(&'static self) -> ShutdownListener {
        self.inc();

        ShutdownListener {
            notify: self.notify_shutdown.notified(),
        }
    }

    /// Completes once the shutdown signal was sent, and all [`ShutdownListener`]s were dropped.
    pub async fn wait(&self) {
        // Shutdown is in progress and no listeners are being waited for.
        if self.in_progress.load(Ordering::Acquire) && self.counter.load(Ordering::Acquire) == 0 {
            return;
        }

        self.notify_done.notified().await;
    }
}

/// Initializes the handlers for OS signals.
#[inline]
pub fn init() {
    #[cfg(unix)]
    unix::init();

    #[cfg(not(unix))]
    {
        tracing::warn!("Signal handlers are only supported for unix-like systems");
        tracing::warn!("Graceful shutdown will not work");
    }
}

pub fn terminate() {
    if SHUTDOWN.in_progress.load(Ordering::Acquire) {
        tracing::info!("SIGKILL");
        std::process::exit(0);
    }

    SHUTDOWN.in_progress.store(true, Ordering::Release);
    SHUTDOWN.notify_shutdown.notify_waiters();

    if SHUTDOWN.counter.load(Ordering::Acquire) == 0 {
        SHUTDOWN.notify_done.notify_waiters();
    }

    tracing::info!(
        "Waiting on {} listeners",
        SHUTDOWN.counter.load(Ordering::Relaxed)
    );
}

#[cfg(unix)]
mod unix {
    use std::ffi::c_int;

    use nix::sys::signal::{sigaction, SaFlags, SigAction, SigHandler, Signal};
    use nix::sys::signalfd::SigSet;

    #[inline]
    pub(super) fn init() {
        let action = SigAction::new(
            SigHandler::Handler(terminate),
            SaFlags::empty(),
            SigSet::empty(),
        );

        unsafe {
            let _ = sigaction(Signal::SIGINT, &action);
            let _ = sigaction(Signal::SIGTERM, &action);
        }
    }

    extern "C" fn terminate(_: c_int) {
        super::terminate();
    }
}
