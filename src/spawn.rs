use std::future::Future;

#[inline]
pub fn spawn_without_return<F>(f: F)
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    #[cfg(feature = "async-std-runtime")]
    async_std::task::spawn(f);

    #[cfg(all(not(feature = "async-std-runtime"), feature = "tokio-runtime"))]
    tokio::spawn(f);
}

pub fn spawn_blocking<F, T>(f: F) -> impl Future<Output = T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    #[cfg(feature = "async-std-runtime")]
    return async_std::task::spawn_blocking(f);

    #[cfg(all(not(feature = "async-std-runtime"), feature = "tokio-runtime"))]
    return async { tokio::task::spawn_blocking(f).await.unwrap() };
}
