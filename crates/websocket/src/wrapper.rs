use {
    crate::{
        Adapter,
        Builder,
        DataCodec,
        Error,
        Observer,
        sealed::MessageCodec,
        transport::{self, Core, DropGuard},
    },
    futures_util::{Sink, Stream},
    pin_project::pin_project,
    std::{
        pin::Pin,
        task::{self, Context, Poll},
        time::Duration,
    },
};

/// Configuration options for the WebSocket transport.
///
/// This should not be used directly. Instead, use the [`Builder`] to configure
/// and create a [`WebSocket`] instance.
pub struct Config {
    pub channel_capacity: usize,
    pub heartbeat_interval: Duration,
    pub idle_timeout: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            channel_capacity: 64,
            heartbeat_interval: Duration::from_secs(5),
            idle_timeout: Duration::from_secs(15),
        }
    }
}

/// A WebSocket transport that supports sending and receiving messages using
/// a specified data codec.
///
/// This is high-level wrapper around the provided WebSocket [`Adapter`] that
/// adds serialization and implements [`Sink`] and [`Stream`].
///
/// The underlying transport is closed when the [`WebSocket`] is dropped.
#[pin_project]
pub struct WebSocket<C> {
    #[pin]
    inner: Core,
    codec: C,
    _guard: DropGuard,
}

impl WebSocket<()> {
    /// Creates a new [`Builder`] for configuring and constructing a
    /// [`WebSocket`] instance.
    pub fn builder() -> Builder<(), (), ()> {
        Builder::new()
    }
}

impl<C> WebSocket<C>
where
    C: DataCodec,
{
    /// Creates a new [`WebSocket`] instance with the specified adapter and
    /// codec using default configuration.
    pub fn new<A>(adapter: A, codec: C) -> Self
    where
        A: Adapter,
    {
        Self::new_internal(adapter, codec, (), Default::default())
    }

    pub(crate) fn new_internal<A, O>(adapter: A, codec: C, observer: O, config: Config) -> Self
    where
        A: Adapter,
        O: Observer,
    {
        let (tx, rx, _guard) = transport::spawn::<A, O>(
            adapter.into_transport(),
            observer,
            config.channel_capacity,
            config.heartbeat_interval,
        );

        Self {
            inner: transport::Core::new(tx, rx, config.idle_timeout),
            codec,
            _guard,
        }
    }
}

impl<C> Sink<C::Payload> for WebSocket<C>
where
    C: DataCodec,
{
    type Error = Error;

    fn start_send(self: Pin<&mut Self>, item: C::Payload) -> Result<(), Self::Error> {
        let item = self.codec.encode(item).and_then(C::MessageCodec::encode)?;

        self.project().inner.start_send(item)
    }

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_ready(cx)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_close(cx)
    }
}

impl<C> Stream for WebSocket<C>
where
    C: DataCodec,
{
    type Item = C::Payload;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();

        let data = task::ready!(this.inner.poll_next(cx))
            .map(|msg| {
                C::MessageCodec::decode(msg)
                    .and_then(|data| this.codec.decode(data))
                    .ok()
            })
            .flatten();

        Poll::Ready(data)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}
