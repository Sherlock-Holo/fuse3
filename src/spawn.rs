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

/*pub async fn spawn_blocking<F, T>(f: F) -> T
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    #[cfg(feature = "async-std-runtime")]
    return async_std::task::spawn_blocking(f).await;

    #[cfg(all(not(feature = "async-std-runtime"), feature = "tokio-runtime"))]
    return tokio::task::spawn_blocking(f).await.unwrap();
}*/
