use std::future::Future;

#[inline]
pub(crate) fn spawn_without_return<F>(f: F)
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    #[cfg(feature = "async-std-runtime")]
    async_std::task::spawn(f);

    #[cfg(all(not(feature = "async-std-runtime"), feature = "tokio-runtime"))]
    tokio::spawn(f);
}
