use std::ptr::NonNull;

/// A shared, raw pointer of `T`. `Shared` is a covariant over `T`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

unsafe impl<T> Send for Shared<T> where T: Send {}
unsafe impl<T> Sync for Shared<T> where T: Sync {}
