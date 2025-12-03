#[cfg(feature = "json")]
pub use json::Json;
#[cfg(feature = "tungstenite")]
pub use tokio_tungstenite;
use {
    crate::wrapper::Config,
    enum_as_inner::EnumAsInner,
    futures_util::{Sink, Stream},
    std::{error::Error as StdError, time::Duration},
};
pub use {bytes::Bytes, wrapper::WebSocket};

mod adapters;
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
    pub fn serialization<T: StdError + 'static>(err: T) -> Self {
        Self::Encoding(Box::new(err))
    }

    pub fn deserialization<T: StdError + 'static>(err: T) -> Self {
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
    type MessageCodec: sealed::MessageCodec;

    /// Payload type that can be sent and received. Assumes a symmetrical
    /// payload format for both directions.
    type Payload: Send + 'static;

    /// Encode the given payload into bytes for transmission.
    fn encode(&self, data: Self::Payload) -> Result<Bytes, Error>;

    /// Decode the given bytes into the payload.
    fn decode(&self, data: Bytes) -> Result<Self::Payload, Error>;
}

/// Adapter for integrating different WebSocket transport implementations.
pub trait Adapter: Send + 'static {
    type Error: StdError + Send;
    type Message: Send;
    type Transport: Sink<Self::Message, Error = Self::Error>
        + Stream<Item = Result<Self::Message, Self::Error>>
        + Send;

    /// Convert the adapter into the underlying transport.
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
        type MessageCodec = TextMessage;
        type Payload = T;

        fn encode(&self, data: Self::Payload) -> Result<Bytes, Error> {
            serde_json::to_vec(&data)
                .map(Into::into)
                .map_err(Error::serialization)
        }

        fn decode(&self, data: Bytes) -> Result<Self::Payload, Error> {
            serde_json::from_slice(&data).map_err(Error::deserialization)
        }
    }
}

/// Generic binary data codec that transmits raw bytes as-is using WebSocket
/// binary messages.
#[derive(Debug, Default)]
pub struct Binary;

impl DataCodec for Binary {
    type MessageCodec = BinaryMessage;
    type Payload = Bytes;

    fn encode(&self, data: Self::Payload) -> Result<Bytes, Error> {
        Ok(data)
    }

    fn decode(&self, data: Bytes) -> Result<Self::Payload, Error> {
        Ok(data)
    }
}

/// Generic plaintext data codec that transmits UTF-8 strings as-is using
/// WebSocket text messages.
#[derive(Debug, Default)]
pub struct Plaintext;

impl DataCodec for Plaintext {
    type MessageCodec = TextMessage;
    type Payload = String;

    fn encode(&self, data: Self::Payload) -> Result<Bytes, Error> {
        Ok(data.into())
    }

    fn decode(&self, data: Bytes) -> Result<Self::Payload, Error> {
        String::from_utf8(data.into()).map_err(Error::deserialization)
    }
}

mod sealed {
    use super::*;

    /// Codec for encoding and decoding WebSocket messages.
    ///
    /// The only two useful implementations for consumers are [`BinaryMessage`]
    /// and [`TextMessage`].
    pub trait MessageCodec {
        fn encode(data: Bytes) -> Result<Message, Error>;
        fn decode(data: Message) -> Result<Bytes, Error>;
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Message is not binary")]
struct InvalidBinaryError;

/// WebSocket message codec for binary data. Encodes the payload into
/// [`Message::Binary`].
pub struct BinaryMessage;

impl sealed::MessageCodec for BinaryMessage {
    fn encode(data: Bytes) -> Result<Message, Error> {
        Ok(Message::Binary(data))
    }

    fn decode(data: Message) -> Result<Bytes, Error> {
        if let Message::Binary(data) = data {
            Ok(data)
        } else {
            Err(Error::invalid_payload(InvalidBinaryError))
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Message is not UTF-8")]
struct InvalidUtf8Error;

/// WebSocket message codec for text data. Encodes the payload into
/// [`Message::Text`].
pub struct TextMessage;

impl sealed::MessageCodec for TextMessage {
    fn encode(data: Bytes) -> Result<Message, Error> {
        String::from_utf8(data.into())
            .map(Message::Text)
            .map_err(|_| Error::invalid_payload(InvalidUtf8Error))
    }

    fn decode(data: Message) -> Result<Bytes, Error> {
        if let Message::Text(data) = data {
            Ok(data.into_bytes().into())
        } else {
            Err(Error::invalid_payload(InvalidBinaryError))
        }
    }
}

/// Builder for configuring and constructing a [`WebSocket`] instance.
pub struct Builder<A, C, O> {
    adapter: A,
    codec: C,
    observer: O,
    config: Config,
}

impl Builder<(), (), ()> {
    /// Create a new [`WebSocket`] builder instance.
    pub fn new() -> Self {
        Self {
            adapter: (),
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

impl<A, C, O> Builder<A, C, O> {
    /// Set the [`Adapter`] for the WebSocket.
    pub fn adapter<T>(self, adapter: T) -> Builder<T, C, O>
    where
        T: Adapter,
    {
        Builder {
            adapter,
            codec: self.codec,
            observer: self.observer,
            config: self.config,
        }
    }

    /// Set the [`DataCodec`] for the WebSocket.
    pub fn codec<T>(self, codec: T) -> Builder<A, T, O>
    where
        T: DataCodec,
    {
        Builder {
            adapter: self.adapter,
            codec,
            observer: self.observer,
            config: self.config,
        }
    }

    /// Set the [`Observer`] for the WebSocket.
    pub fn observer<T>(self, observer: T) -> Builder<A, C, T>
    where
        T: Observer,
    {
        Builder {
            adapter: self.adapter,
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
        A: Adapter,
        C: DataCodec,
        O: Observer,
    {
        WebSocket::new_internal(self.adapter, self.codec, self.observer, self.config)
    }
}
