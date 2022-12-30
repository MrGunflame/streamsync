use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::task::{Context, Poll};

use pin_project::{pin_project, pinned_drop};
use tokio::sync::futures::Notified;
use tokio::sync::Notify;

pub static SHUTDOWN: Shutdown = Shutdown::new();

#[pin_project(PinnedDrop)]
pub struct ShutdownListener {
    #[pin]
    notify: Notified<'static>,
}

impl ShutdownListener {
    #[inline]
    pub fn new() -> Self {
        SHUTDOWN.listen()
    }

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

pub struct Shutdown {
    in_progress: AtomicBool,
    counter: AtomicUsize,
    notify_shutdown: Notify,
    notify_done: Notify,
}

impl Shutdown {
    const fn new() -> Self {
        Self {
            in_progress: AtomicBool::new(false),
            counter: AtomicUsize::new(0),
            notify_shutdown: Notify::const_new(),
            notify_done: Notify::const_new(),
        }
    }

    #[inline]
    fn inc(&self) {
        self.counter.fetch_add(1, Ordering::AcqRel);
    }

    #[inline]
    fn dec(&self) {
        self.counter.fetch_sub(1, Ordering::AcqRel);

        if self.in_progress.load(Ordering::Acquire) && self.counter.load(Ordering::Acquire) == 0 {
            self.notify_done.notify_waiters();
        }
    }

    #[inline]
    pub fn listen(&'static self) -> ShutdownListener {
        self.inc();

        ShutdownListener {
            notify: self.notify_shutdown.notified(),
        }
    }

    pub async fn wait(&self) {
        // Shutdown is in progress and no listeners are being waited for.
        if self.in_progress.load(Ordering::Acquire) && self.counter.load(Ordering::Acquire) == 0 {
            return;
        }

        self.notify_done.notified().await;
    }
}

#[inline]
pub fn init() {
    #[cfg(unix)]
    unix::init();
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
