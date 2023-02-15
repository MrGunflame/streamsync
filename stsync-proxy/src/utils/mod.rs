pub mod serial;

use std::cmp::Ordering;
use std::fmt::{self, Debug, Formatter};
use std::hash::{Hash, Hasher};
use std::ptr::NonNull;

/// A shared, raw pointer of `T`. `Shared` is a covariant over `T`.
#[repr(transparent)]
pub struct Shared<T>(NonNull<T>);

impl<T> Shared<T> {
    /// Returns a shared reference to the value.
    #[inline]
    pub unsafe fn as_ref<'a>(&self) -> &'a T {
        // SAFETY: The caller must guarantee all safety conditions are met.
        unsafe { self.0.as_ref() }
    }
}

impl<T> From<&T> for Shared<T> {
    #[inline]
    fn from(src: &T) -> Self {
        Self(src.into())
    }
}

impl<T> Clone for Shared<T> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Shared<T> {}

impl<T> Debug for Shared<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<T> PartialEq for Shared<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<T> Eq for Shared<T> {}

impl<T> PartialOrd for Shared<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

impl<T> Ord for Shared<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}

impl<T> Hash for Shared<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

unsafe impl<T> Send for Shared<T> where T: Send {}
unsafe impl<T> Sync for Shared<T> where T: Sync {}
