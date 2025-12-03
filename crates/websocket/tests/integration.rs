use {
    bytes::Bytes,
    futures_concurrency::future::Race,
    futures_util::{FutureExt as _, SinkExt as _, Stream, StreamExt as _},
    serde::{Deserialize, Serialize},
    std::{
        marker::PhantomData,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    },
    tokio::{
        net::{TcpListener, TcpStream},
        sync::Notify,
    },
    tokio_tungstenite::{MaybeTlsStream, WebSocketStream},
    wc_websocket::{Binary, DataCodec, Json, Message, Observer, Plaintext, WebSocket},
};

struct EchoServer<C> {
    address: String,
    notify: Arc<Notify>,
    num_connections: Arc<AtomicUsize>,
    _marker: PhantomData<C>,
}

impl<C> EchoServer<C>
where
    C: DataCodec + Default + Send + Sync + 'static,
{
    async fn new() -> Self {
        Self::with_builder(|socket| WebSocket::new(socket, C::default())).await
    }

    async fn with_builder<B>(builder: B) -> Self
    where
        B: Fn(WebSocketStream<TcpStream>) -> WebSocket<C> + Send + Sync + 'static,
    {
        let builder = Arc::new(builder);
        let socket = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = socket.local_addr().unwrap();
        let address = format!("ws://{address}");
        let notify = Arc::new(Notify::new());
        let num_connections = Arc::new(AtomicUsize::new(0));

        tokio::spawn({
            let notify = notify.clone();
            let num_connections = num_connections.clone();

            async move {
                while let Ok((stream, _)) = socket.accept().await {
                    let builder = builder.clone();
                    let notify = notify.clone();
                    let num_connections = num_connections.clone();

                    tokio::spawn(async move {
                        num_connections.fetch_add(1, Ordering::SeqCst);

                        let socket = tokio_tungstenite::accept_async(stream).await.unwrap();
                        let socket = builder(socket);
                        let (tx, rx) = socket.split();
                        let _ = rx.take_until(notify.notified()).map(Ok).forward(tx).await;

                        num_connections.fetch_sub(1, Ordering::SeqCst);
                    });
                }
            }
        });

        Self {
            address,
            notify,
            num_connections,
            _marker: PhantomData,
        }
    }

    fn disconnect_clients(&self) {
        self.notify.notify_waiters();
    }

    fn num_connections(&self) -> usize {
        self.num_connections.load(Ordering::SeqCst)
    }

    async fn connect(&self) -> WebSocketStream<MaybeTlsStream<TcpStream>> {
        let (socket, _) = tokio_tungstenite::connect_async(&self.address)
            .await
            .unwrap();

        socket
    }
}

#[tokio::test]
async fn basic() {
    let server = EchoServer::<Plaintext>::new().await;
    let mut socket = WebSocket::new(server.connect().await, Plaintext);

    assert_eq!(server.num_connections(), 1);

    let sent_payload = "hello world".to_owned();
    socket.send(sent_payload.clone()).await.unwrap();

    assert_eq!(next(&mut socket).await, Some(sent_payload));
    server.disconnect_clients();
    assert_eq!(next(&mut socket).await, None);
    assert_eq!(server.num_connections(), 0);
}

#[derive(Default, Clone)]
struct HeartbeatObserver {
    inbound_pings: Arc<AtomicUsize>,
    inbound_pongs: Arc<AtomicUsize>,
    outbound_pings: Arc<AtomicUsize>,
    latency_reports: Arc<AtomicUsize>,
}

impl HeartbeatObserver {
    fn inbound_pings(&self) -> usize {
        fetch(&self.inbound_pings)
    }

    fn inbound_pongs(&self) -> usize {
        fetch(&self.inbound_pongs)
    }

    fn outbound_pings(&self) -> usize {
        fetch(&self.outbound_pings)
    }

    fn latency_reports(&self) -> usize {
        fetch(&self.latency_reports)
    }
}

impl Observer for HeartbeatObserver {
    fn inbound_message(&self, msg: &Message) {
        match msg {
            Message::Ping(_) => inc(&self.inbound_pings),
            Message::Pong(_) => inc(&self.inbound_pongs),
            _ => {}
        }
    }

    fn outbound_message(&self, msg: &Message) {
        if let Message::Ping(_) = msg {
            inc(&self.outbound_pings);
        }
    }

    fn latency(&self, _: Duration) {
        inc(&self.latency_reports);
    }
}

#[tokio::test]
async fn heartbeat() {
    let server = EchoServer::with_builder(|socket| {
        WebSocket::builder()
            .adapter(socket)
            .codec(Plaintext)
            .heartbeat_interval(Duration::from_millis(300))
            .build()
    })
    .await;

    let observer = HeartbeatObserver::default();

    let mut socket = WebSocket::builder()
        .adapter(server.connect().await)
        .observer(observer.clone())
        .codec(Plaintext)
        .heartbeat_interval(Duration::from_millis(500))
        .build();

    (socket.next().map(drop), sleep(1100)).race().await;

    assert_eq!(observer.inbound_pings(), 3);
    assert_eq!(observer.outbound_pings(), 2);
    assert_eq!(observer.inbound_pongs(), 2);
    assert_eq!(observer.latency_reports(), 2);
}

#[tokio::test]
async fn timeout_server() {
    let server = EchoServer::with_builder(|socket| {
        WebSocket::builder()
            .adapter(socket)
            .codec(Plaintext)
            .heartbeat_interval(Duration::from_secs(5))
            .idle_timeout(Duration::from_secs(1))
            .build()
    })
    .await;

    let mut socket = WebSocket::new(server.connect().await, Plaintext);

    assert_eq!(server.num_connections(), 1);
    sleep(1100).await;
    assert_eq!(server.num_connections(), 0);
    assert_eq!(next(&mut socket).await, None);
}

#[tokio::test]
async fn timeout_client() {
    let server = EchoServer::with_builder(|socket| {
        WebSocket::builder()
            .adapter(socket)
            .codec(Plaintext)
            .heartbeat_interval(Duration::from_secs(5))
            .idle_timeout(Duration::from_secs(15))
            .build()
    })
    .await;

    let mut socket = WebSocket::builder()
        .adapter(server.connect().await)
        .codec(Plaintext)
        .heartbeat_interval(Duration::from_secs(5))
        .idle_timeout(Duration::from_secs(1))
        .build();

    sleep(1100).await;
    assert!(socket.send("hello world".to_owned()).await.is_err());
    assert_eq!(next(&mut socket).await, None);
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct Payload(u32);

#[tokio::test]
async fn json() {
    let server = EchoServer::<Json<Payload>>::new().await;
    let mut socket = WebSocket::new(server.connect().await, Json::<Payload>::default());

    let sent_payload = Payload(42);
    socket.send(sent_payload.clone()).await.unwrap();
    assert_eq!(next(&mut socket).await, Some(sent_payload));
}

#[tokio::test]
async fn binary() {
    let server = EchoServer::<Binary>::new().await;
    let mut socket = WebSocket::new(server.connect().await, Binary);

    let sent_payload = Bytes::from("hello world");
    socket.send(sent_payload.clone()).await.unwrap();
    assert_eq!(next(&mut socket).await, Some(sent_payload));
}

async fn sleep(millis: u64) {
    tokio::time::sleep(Duration::from_millis(millis)).await
}

async fn next<T, S>(stream: &mut S) -> Option<T>
where
    S: Stream<Item = T> + Unpin,
{
    tokio::time::timeout(Duration::from_secs(1), stream.next())
        .await
        .unwrap()
}

fn inc(counter: &Arc<AtomicUsize>) {
    counter.fetch_add(1, Ordering::SeqCst);
}

fn fetch(counter: &Arc<AtomicUsize>) -> usize {
    counter.load(Ordering::SeqCst)
}
