pub use tokio_util::sync::CancellationToken;
use {
    pin_project::pin_project,
    std::{
        future::{ready, Future, Ready},
        pin::Pin,
        task::{Context, Poll},
        time::Duration,
    },
    tokio::{task::JoinHandle, time::Timeout},
    tokio_util::sync::WaitForCancellationFutureOwned,
};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("Timeout has expired")]
    Timeout,

    #[error("Canceled")]
    Canceled,
}

#[must_use = "futures do nothing unless you `.await` or poll them"]
#[pin_project]
pub struct TimeoutFuture<T, U> {
    #[pin]
    fut: Timeout<T>,
    #[pin]
    on_timeout: U,
}

impl<T, U> TimeoutFuture<T, U>
where
    T: Future,
    U: Future,
{
    pub fn on_timeout<V>(self, on_timeout: V) -> TimeoutFuture<T, V>
    where
        V: Future,
    {
        TimeoutFuture {
            fut: self.fut,
            on_timeout,
        }
    }
}

impl<T, U> Future for TimeoutFuture<T, U>
where
    T: Future,
    U: Future,
{
    type Output = Result<T::Output, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        match this.fut.poll(cx) {
            Poll::Ready(Err(_)) => match this.on_timeout.poll(cx) {
                Poll::Ready(_) => Poll::Ready(Err(Error::Timeout)),
                Poll::Pending => Poll::Pending,
            },

            Poll::Ready(Ok(val)) => Poll::Ready(Ok(val)),

            Poll::Pending => Poll::Pending,
        }
    }
}

#[must_use = "futures do nothing unless you `.await` or poll them"]
#[pin_project]
pub struct CancellationFuture<T, U = Ready<()>> {
    #[pin]
    cancellation: WaitForCancellationFutureOwned,
    #[pin]
    fut: T,
    #[pin]
    on_cancel: U,
}

impl<T, U> CancellationFuture<T, U>
where
    T: Future,
    U: Future,
{
    pub fn on_cancel<V>(self, on_cancel: V) -> CancellationFuture<T, V>
    where
        V: Future,
    {
        CancellationFuture {
            cancellation: self.cancellation,
            fut: self.fut,
            on_cancel,
        }
    }
}

impl<T, U> Future for CancellationFuture<T, U>
where
    T: Future,
    U: Future,
{
    type Output = Result<T::Output, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        match this.cancellation.poll(cx) {
            Poll::Ready(_) => match this.on_cancel.poll(cx) {
                Poll::Ready(_) => Poll::Ready(Err(Error::Canceled)),
                Poll::Pending => Poll::Pending,
            },

            Poll::Pending => match this.fut.poll(cx) {
                Poll::Ready(val) => Poll::Ready(Ok(val)),
                Poll::Pending => Poll::Pending,
            },
        }
    }
}

/// Quality of life methods for cleaner futures spawning, timeout and
/// cancellation using [`CancellationToken`].
pub trait FutureExt {
    type Future: Future;

    /// Effectively wraps the future in [`tokio::time::timeout()`], returning a
    /// future that also allows you to run different future, in case the timeout
    /// expires.
    ///
    /// # Example
    ///
    /// ```rust
    /// use {
    ///     future::{Error, FutureExt},
    ///     std::time::Duration,
    /// };
    ///
    /// # async fn example() {
    /// let answer = async {
    ///     tokio::time::sleep(Duration::from_millis(500)).await;
    ///     42
    /// }
    /// .with_timeout(Duration::from_millis(100))
    /// .on_timeout(async {
    ///     // Run some cleanup routine...
    /// });
    ///
    /// // Did not receive the answer within 100ms.
    /// assert!(matches!(answer.await, Err(Error::Timeout)));
    /// # }
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// #     example().await;
    /// # }
    /// ```
    fn with_timeout(self, duration: Duration) -> TimeoutFuture<Self::Future, Ready<()>>;

    /// Consumes the future, returning a new future that cancels the original
    /// future if the provided [`CancellationToken`] is canceled. Optionally
    /// allows to run another future in case of cancellation.
    ///
    /// # Example
    ///
    /// ```rust
    /// use {
    ///     future::{Error, FutureExt, StaticFutureExt},
    ///     std::time::Duration,
    ///     tokio_util::sync::CancellationToken,
    /// };
    ///
    /// # async fn example() {
    /// let token = CancellationToken::new();
    ///
    /// let answer = tokio::task::spawn(
    ///     async {
    ///         tokio::time::sleep(Duration::from_millis(500)).await;
    ///         42
    ///     }
    ///     .with_cancellation(token.clone())
    ///     .on_cancel(async {
    ///         // Run some cleanup routine...
    ///     }),
    /// );
    ///
    /// tokio::time::sleep(Duration::from_millis(100)).await;
    /// token.cancel();
    ///
    /// // Did not receive the answer, since the future was canceled before it
    /// // finished.
    /// assert!(matches!(dbg!(answer.await), Ok(Err(Error::Canceled))));
    /// # }
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// #     example().await;
    /// # }
    /// ```
    fn with_cancellation(
        self,
        token: CancellationToken,
    ) -> CancellationFuture<Self::Future, Ready<()>>;
}

pub trait StaticFutureExt {
    type Future: Future + Send;

    /// Spawns the future using [`tokio::spawn()`], returning its
    /// [`JoinHandle`].
    ///
    /// # Example
    ///
    /// ```rust
    /// use {future::StaticFutureExt, std::time::Duration};
    ///
    /// # async fn example() {
    /// let join_handle = async {
    ///     tokio::time::sleep(Duration::from_millis(500)).await;
    ///     42
    /// }
    /// .spawn();
    ///
    /// assert!(matches!(join_handle.await, Ok(42)));
    /// # }
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// #     example().await;
    /// # }
    /// ```
    fn spawn(self) -> JoinHandle<<Self::Future as Future>::Output>;
}

impl<T> FutureExt for T
where
    T: Future,
{
    type Future = T;

    fn with_timeout(self, duration: Duration) -> TimeoutFuture<Self::Future, Ready<()>> {
        TimeoutFuture {
            fut: tokio::time::timeout(duration, self),
            on_timeout: ready(()),
        }
    }

    fn with_cancellation(
        self,
        token: CancellationToken,
    ) -> CancellationFuture<Self::Future, Ready<()>> {
        CancellationFuture {
            cancellation: token.cancelled_owned(),
            fut: self,
            on_cancel: ready(()),
        }
    }
}

impl<T> StaticFutureExt for T
where
    T: Future + Send + 'static,
    T::Output: Send,
{
    type Future = T;

    fn spawn(self) -> JoinHandle<<Self::Future as Future>::Output> {
        tokio::spawn(self)
    }
}

#[cfg(test)]
mod test {
    use {
        super::*,
        std::{
            sync::{
                atomic::{AtomicU32, Ordering},
                Arc,
            },
            time::Duration,
        },
        tokio_util::sync::CancellationToken,
    };

    #[tokio::test]
    async fn cancel() {
        let a = Arc::new(AtomicU32::default());
        let b = Arc::new(AtomicU32::default());
        let token = CancellationToken::new();
        let handle = {
            let a = a.clone();
            let b = b.clone();

            async move {
                a.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(500)).await;
                a.fetch_add(1, Ordering::SeqCst);
                42
            }
            .with_cancellation(token.child_token())
            .on_cancel(async move {
                b.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(100)).await;
                b.fetch_add(1, Ordering::SeqCst);
            })
            .spawn()
        };

        tokio::time::sleep(Duration::from_millis(200)).await;
        token.cancel();

        assert_eq!(handle.await.unwrap(), Err(Error::Canceled));
        assert_eq!(a.load(Ordering::SeqCst), 1);
        assert_eq!(b.load(Ordering::SeqCst), 2);

        let a = Arc::new(AtomicU32::default());
        let b = Arc::new(AtomicU32::default());
        let token = CancellationToken::new();
        let handle = {
            let a = a.clone();
            let b = b.clone();

            async move {
                a.fetch_add(1, Ordering::Relaxed);
                tokio::time::sleep(Duration::from_millis(100)).await;
                a.fetch_add(1, Ordering::Relaxed);
                42
            }
            .with_timeout(Duration::from_millis(500))
            .on_timeout(async move {
                b.fetch_add(1, Ordering::Relaxed);
                tokio::time::sleep(Duration::from_millis(100)).await;
                b.fetch_add(1, Ordering::Relaxed);
            })
            .spawn()
        };

        tokio::time::sleep(Duration::from_millis(200)).await;
        token.cancel();

        assert_eq!(handle.await.unwrap(), Ok(42));
        assert_eq!(a.load(Ordering::SeqCst), 2);
        assert_eq!(b.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn timeout() {
        let a = Arc::new(AtomicU32::default());
        let b = Arc::new(AtomicU32::default());
        let handle = {
            let a = a.clone();
            let b = b.clone();

            async move {
                a.fetch_add(1, Ordering::Relaxed);
                tokio::time::sleep(Duration::from_millis(500)).await;
                a.fetch_add(1, Ordering::Relaxed);
                42
            }
            .with_timeout(Duration::from_millis(100))
            .on_timeout(async move {
                b.fetch_add(1, Ordering::Relaxed);
                tokio::time::sleep(Duration::from_millis(100)).await;
                b.fetch_add(1, Ordering::Relaxed);
            })
            .spawn()
        };

        assert_eq!(handle.await.unwrap(), Err(Error::Timeout));
        assert_eq!(a.load(Ordering::SeqCst), 1);
        assert_eq!(b.load(Ordering::SeqCst), 2);

        let a = Arc::new(AtomicU32::default());
        let b = Arc::new(AtomicU32::default());
        let handle = {
            let a = a.clone();
            let b = b.clone();

            async move {
                a.fetch_add(1, Ordering::Relaxed);
                tokio::time::sleep(Duration::from_millis(100)).await;
                a.fetch_add(1, Ordering::Relaxed);
                42
            }
            .with_timeout(Duration::from_millis(500))
            .on_timeout(async move {
                b.fetch_add(1, Ordering::Relaxed);
                tokio::time::sleep(Duration::from_millis(100)).await;
                b.fetch_add(1, Ordering::Relaxed);
            })
            .spawn()
        };

        assert_eq!(handle.await.unwrap(), Ok(42));
        assert_eq!(a.load(Ordering::SeqCst), 2);
        assert_eq!(b.load(Ordering::SeqCst), 0);
    }
}
