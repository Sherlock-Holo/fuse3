use std::future::Future;

pub async fn spawn<F>(f: F) -> impl Future<Output = F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    #[cfg(feature = "async-std-runtime")]
    let task = async_std::task::spawn(f);

    #[cfg(all(not(feature = "async-std-runtime"), feature = "tokio-runtime"))]
    let task = async move { tokio::spawn(f).await.unwrap() };

    task
}
