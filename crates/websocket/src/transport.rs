use {
    crate::{Backend, Error, Message, Observer},
    bytes::Bytes,
    futures_concurrency::future::{Join as _, Race},
    futures_timer::Delay,
    futures_util::{FutureExt as _, Sink, SinkExt as _, Stream, StreamExt as _, TryStreamExt},
    pin_project::pin_project,
    std::{
        pin::Pin,
        sync::Arc,
        task::{self, Context, Poll},
        time::{Duration, SystemTime, UNIX_EPOCH},
    },
    tap::Pipe as _,
    tokio::sync::{Notify, mpsc},
    tokio_stream::wrappers::{IntervalStream, ReceiverStream},
    tokio_util::sync::PollSender,
};

pub struct DropGuard(Arc<Notify>);

impl Drop for DropGuard {
    fn drop(&mut self) {
        self.0.notify_waiters();
    }
}

/// Spawn the transport task, which handles forwarding messages between the
/// native transport and [`Core`] via [`tokio`] channels.
pub fn spawn<B, O>(
    transport: B::Transport,
    observer: O,
    capacity: usize,
    heartbeat_interval: Duration,
) -> (mpsc::Sender<Message>, mpsc::Receiver<Message>, DropGuard)
where
    B: Backend,
    O: Observer,
{
    let (trans_tx, trans_rx) = transport.split();
    let (in_tx, in_rx) = mpsc::channel(capacity);
    let (out_tx, out_rx) = mpsc::channel(capacity);

    // External shutdown is triggered when the `WebSocket` is dropped.
    let shutdown = Arc::new(Notify::new());

    tokio::spawn({
        let external_shutdown = shutdown.clone();

        async move {
            // Internal shutdown is triggered when the receiving stream from the underlying
            // transport has ended.
            let internal_shutdown = Notify::new();

            let in_rx = ReceiverStream::new(in_rx);

            // Since we're merging multiple streams below, we need to end the heartbeat
            // stream with both internal and external triggers. Otherwise the heartbeat
            // stream will keep the channels alive indefinitely.
            let heartbeat = heartbeat_stream(heartbeat_interval)
                .take_until((external_shutdown.notified(), internal_shutdown.notified()).race());

            // Forward messages from the `WebSocket` instance into the native transport.
            let fwd_in = tokio_stream::StreamExt::merge(in_rx, heartbeat)
                .inspect(|msg| {
                    observer.outbound_message(msg);
                })
                .map(B::encode_message)
                .map(Ok)
                .forward(trans_tx);

            // Forward messages from the native transport to the `WebSocket` instance.
            let fwd_out = trans_rx
                .take_until(external_shutdown.notified())
                .map_ok(B::decode_message)
                .map_err(Error::transport)
                .inspect_ok(|msg| {
                    observer.inbound_message(msg);

                    // We've received a `Pong` message with the timestamp from the heartbeat stream.
                    // Decode the timestamp and calculate the round-trip time.
                    if let Message::Pong(data) = msg {
                        let rtt = timestamp()
                            .saturating_sub(decode_timestamp(data))
                            .pipe(Duration::from_millis);

                        observer.latency(rtt);
                    }
                })
                .forward(PollSender::new(out_tx).sink_map_err(Error::internal))
                .map(|_| internal_shutdown.notify_one());

            (fwd_in, fwd_out).join().map(drop).await;
        }
    });

    (in_tx, out_rx, DropGuard(shutdown))
}

/// Core transport that handles sending and receiving [`Message`]s with
/// heartbeat and idle timeout support.
#[pin_project]
pub struct Core {
    #[pin]
    tx: PollSender<Message>,
    rx: ReceiverStream<Message>,
    timeout_fut: Option<Delay>,
    timeout: Duration,
}

impl Core {
    pub fn new(tx: mpsc::Sender<Message>, rx: mpsc::Receiver<Message>, timeout: Duration) -> Self {
        Self {
            tx: PollSender::new(tx),
            rx: ReceiverStream::new(rx),
            timeout_fut: Some(Delay::new(timeout)),
            timeout,
        }
    }

    #[inline]
    fn poll_timeout(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        if let Some(delay) = &mut self.timeout_fut {
            if delay.poll_unpin(cx).is_ready() {
                self.timeout_fut = None;

                Poll::Ready(())
            } else {
                Poll::Pending
            }
        } else {
            Poll::Ready(())
        }
    }

    #[inline]
    fn reset_timeout(&mut self) {
        let timeout = self.timeout;

        if let Some(delay) = &mut self.timeout_fut {
            delay.reset(timeout);
        }
    }
}

impl Sink<Message> for Core {
    type Error = Error;

    fn start_send(self: Pin<&mut Self>, msg: Message) -> Result<(), Self::Error> {
        self.project().tx.start_send(msg).map_err(|_| Error::Closed)
    }

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if self.poll_timeout(cx).is_ready() {
            Poll::Ready(Err(Error::Closed))
        } else {
            self.project().tx.poll_ready(cx).map_err(|_| Error::Closed)
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().tx.poll_flush(cx).map_err(|_| Error::Closed)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().tx.poll_close(cx).map_err(|_| Error::Closed)
    }
}

impl Stream for Core {
    type Item = Message;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.poll_timeout(cx).is_ready() {
            return Poll::Ready(None);
        }

        loop {
            let result = task::ready!(self.rx.poll_next_unpin(cx));

            self.reset_timeout();

            let result = match result {
                Some(msg) => match msg {
                    Message::Binary(_) | Message::Text(_) => Some(msg),
                    Message::Close(_) => None,
                    Message::Ping(_) | Message::Pong(_) => continue,
                },

                None => None,
            };

            return Poll::Ready(result);
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.rx.size_hint()
    }
}

/// Creates a stream that yields heartbeat [`Message::Ping`] messages at the
/// specified period.
fn heartbeat_stream(period: Duration) -> impl Stream<Item = Message> {
    let mut interval = tokio::time::interval(period);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    interval.reset();

    IntervalStream::new(interval).map(|_| encode_timestamp(timestamp()).pipe(Message::Ping))
}

fn encode_timestamp(timestamp: u64) -> Bytes {
    timestamp.to_be_bytes().to_vec().into()
}

fn decode_timestamp(data: &[u8]) -> u64 {
    data.try_into().map(u64::from_be_bytes).unwrap_or_default()
}

fn timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .pipe(|timestamp| timestamp as u64)
}
