#[cfg(feature = "json")]
pub use json::Json;
#[cfg(feature = "tungstenite")]
pub use tokio_tungstenite;
use {
    crate::wrapper::Config,
    derive_more::{From, Into},
    enum_as_inner::EnumAsInner,
    futures_util::{Sink, Stream},
    std::{error::Error as StdError, time::Duration},
};
pub use {bytes::Bytes, wrapper::WebSocket};

mod backend;
mod transport;
mod wrapper;

type BoxError = Box<dyn StdError>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Encoding failed: {0}")]
    Encoding(BoxError),

    #[error("Decoding failed: {0}")]
    Decoding(BoxError),

    #[error("Invalid payload: {0}")]
    InvalidPayload(BoxError),

    #[error("Transport is closed")]
    Closed,

    #[error("Transport error: {0}")]
    Transport(BoxError),

    #[error("Internal error: {0}")]
    Internal(BoxError),
}

impl Error {
    pub fn encoding<T: StdError + 'static>(err: T) -> Self {
        Self::Encoding(Box::new(err))
    }

    pub fn decoding<T: StdError + 'static>(err: T) -> Self {
        Self::Decoding(Box::new(err))
    }

    pub fn transport<T: StdError + 'static>(err: T) -> Self {
        Self::Transport(Box::new(err))
    }

    pub fn internal<T: StdError + 'static>(err: T) -> Self {
        Self::Internal(Box::new(err))
    }

    pub fn invalid_payload<T: StdError + 'static>(err: T) -> Self {
        Self::InvalidPayload(Box::new(err))
    }
}

#[derive(Debug, EnumAsInner)]
pub enum Message {
    Text(String),
    Binary(Bytes),
    Ping(Bytes),
    Pong(Bytes),
    Close(Option<CloseFrame>),
}

impl Message {
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Message::Binary(data) => data,
            Message::Text(data) => data.as_bytes(),
            Message::Ping(data) => data,
            Message::Pong(data) => data,
            Message::Close(_) => &[],
        }
    }
}

#[derive(Debug)]
pub struct CloseFrame {
    pub code: u16,
    pub reason: String,
}

/// Embeddable observer for monitoring WebSocket messages.
pub trait Observer: Send + Sync + 'static {
    /// Called when an inbound message is received.
    fn inbound_message(&self, _msg: &Message) {}

    /// Called when an outbound message is sent.
    fn outbound_message(&self, _msg: &Message) {}

    /// Called when round-trip latency is measured.
    ///
    /// Latency measurement is based on ping-pong messages, which are triggered
    /// on the heartbeat interval, so it should roughly correspond to that
    /// interval.
    fn latency(&self, _rtt: Duration) {}
}

impl Observer for () {}

/// Codec for encoding and decoding the payload sent and received over the
/// WebSocket.
pub trait DataCodec {
    /// Associated message codec used to wrap and unwrap the encoded payload
    /// into the [`Message`] transmitted.
    type Message: Into<Message> + TryFrom<Message, Error = Error>;

    /// Payload type that can be sent and received. Assumes a symmetrical
    /// payload format for both directions.
    type Payload: Send + 'static;

    /// Encode the given payload into [`Message`] for transmission.
    fn encode(&self, data: Self::Payload) -> Result<Self::Message, Error>;

    /// Decode the given [`Message`] into the payload.
    fn decode(&self, data: Self::Message) -> Result<Self::Payload, Error>;
}

/// Backend for integrating different WebSocket transport implementations.
pub trait Backend: Send + 'static {
    type Error: StdError + Send;
    type Message: Send;
    type Transport: Sink<Self::Message, Error = Self::Error>
        + Stream<Item = Result<Self::Message, Self::Error>>
        + Send;

    /// Convert the backend into the underlying transport.
    fn into_transport(self) -> Self::Transport;

    /// Encode the given [`Message`] into the transport-specific message type.
    fn encode_message(msg: Message) -> Self::Message;

    /// Decode the given transport-specific message type into [`Message`].
    fn decode_message(msg: Self::Message) -> Message;
}

#[cfg(feature = "json")]
mod json {
    use {
        super::*,
        serde::{Serialize, de::DeserializeOwned},
        std::marker::PhantomData,
    };

    /// Generic JSON data codec using [`serde_json`] for all payloads that
    /// implement [`serde`]'s [`Serialize`] and [`DeserializeOwned`].
    #[derive(Debug)]
    pub struct Json<T>(PhantomData<T>);

    impl<T> Default for Json<T> {
        fn default() -> Self {
            Self(PhantomData)
        }
    }

    impl<T> DataCodec for Json<T>
    where
        T: Serialize + DeserializeOwned + Send + 'static,
    {
        type Message = TextMessage;
        type Payload = T;

        fn encode(&self, data: Self::Payload) -> Result<Self::Message, Error> {
            serde_json::to_string(&data)
                .map(TextMessage)
                .map_err(Error::encoding)
        }

        fn decode(&self, data: Self::Message) -> Result<Self::Payload, Error> {
            serde_json::from_slice(data.as_bytes()).map_err(Error::decoding)
        }
    }
}

/// Generic binary data codec that transmits raw bytes as-is using WebSocket
/// binary messages.
#[derive(Debug, Default)]
pub struct Binary;

impl DataCodec for Binary {
    type Message = BinaryMessage;
    type Payload = Bytes;

    fn encode(&self, data: Self::Payload) -> Result<Self::Message, Error> {
        Ok(data.into())
    }

    fn decode(&self, data: Self::Message) -> Result<Self::Payload, Error> {
        Ok(data.into())
    }
}

/// Generic plaintext data codec that transmits UTF-8 strings as-is using
/// WebSocket text messages.
#[derive(Debug, Default)]
pub struct Plaintext;

impl DataCodec for Plaintext {
    type Message = TextMessage;
    type Payload = String;

    fn encode(&self, data: Self::Payload) -> Result<Self::Message, Error> {
        Ok(data.into())
    }

    fn decode(&self, data: Self::Message) -> Result<Self::Payload, Error> {
        Ok(data.into())
    }
}

/// WebSocket message codec for binary data. Encodes the payload into
/// [`Message::Binary`].
#[derive(Into, From)]
pub struct BinaryMessage(Bytes);

impl BinaryMessage {
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl From<BinaryMessage> for Message {
    fn from(msg: BinaryMessage) -> Self {
        Message::Binary(msg.into())
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Message is not binary")]
struct InvalidBinaryError;

impl TryFrom<Message> for BinaryMessage {
    type Error = Error;

    fn try_from(data: Message) -> Result<Self, Self::Error> {
        data.into_binary()
            .map(Self)
            .map_err(|_| Error::decoding(InvalidBinaryError))
    }
}

/// WebSocket message codec for text data. Encodes the payload into
/// [`Message::Text`].
#[derive(Into, From)]
pub struct TextMessage(String);

impl TextMessage {
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl From<TextMessage> for Message {
    fn from(msg: TextMessage) -> Self {
        Message::Text(msg.into())
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Message is not UTF-8")]
struct InvalidUtf8Error;

impl TryFrom<Message> for TextMessage {
    type Error = Error;

    fn try_from(data: Message) -> Result<Self, Self::Error> {
        data.into_text()
            .map(Self)
            .map_err(|_| Error::decoding(InvalidUtf8Error))
    }
}

/// Builder for configuring and constructing a [`WebSocket`] instance.
pub struct Builder<B, C, O> {
    backend: B,
    codec: C,
    observer: O,
    config: Config,
}

impl Builder<(), (), ()> {
    /// Create a new [`WebSocket`] builder instance.
    pub fn new() -> Self {
        Self {
            backend: (),
            codec: (),
            observer: (),
            config: Default::default(),
        }
    }
}

impl Default for Builder<(), (), ()> {
    fn default() -> Self {
        Self::new()
    }
}

impl<B, C, O> Builder<B, C, O> {
    /// Set the [`Backend`] for the WebSocket.
    pub fn backend<T>(self, backend: T) -> Builder<T, C, O>
    where
        T: Backend,
    {
        Builder {
            backend,
            codec: self.codec,
            observer: self.observer,
            config: self.config,
        }
    }

    /// Set the [`DataCodec`] for the WebSocket.
    pub fn codec<T>(self, codec: T) -> Builder<B, T, O>
    where
        T: DataCodec,
    {
        Builder {
            backend: self.backend,
            codec,
            observer: self.observer,
            config: self.config,
        }
    }

    /// Set the [`Observer`] for the WebSocket.
    pub fn observer<T>(self, observer: T) -> Builder<B, C, T>
    where
        T: Observer,
    {
        Builder {
            backend: self.backend,
            codec: self.codec,
            observer,
            config: self.config,
        }
    }

    /// Set the internal channel capacity for the WebSocket. The channel is used
    /// to buffer messages sent and received.
    ///
    /// Default value: `64`.
    pub fn channel_capacity(mut self, capacity: usize) -> Self {
        self.config.channel_capacity = capacity;
        self
    }

    /// Set the heartbeat interval for the WebSocket. Heartbeat messages are
    /// sent as [`Message::Ping`] and act as a keep-alive mechanism as well
    /// as to measure the round-trip time latency (see [`Observer::latency`]).
    ///
    /// Default value: `5s`.
    pub fn heartbeat_interval(mut self, interval: Duration) -> Self {
        self.config.heartbeat_interval = interval;
        self
    }

    /// Set the idle timeout for the WebSocket. If no messages are received
    /// within the timeout duration, the WebSocket connection is closed. This
    /// should always be higher than the heartbeat interval.
    ///
    /// Default value: `15s`.
    pub fn idle_timeout(mut self, timeout: Duration) -> Self {
        self.config.idle_timeout = timeout;
        self
    }

    /// Build the configured [`WebSocket`] instance.
    pub fn build(self) -> WebSocket<C>
    where
        B: Backend,
        C: DataCodec,
        O: Observer,
    {
        WebSocket::new_internal(self.backend, self.codec, self.observer, self.config)
    }
}
