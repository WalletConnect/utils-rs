use {
    crate::{Backend, CloseFrame, Message},
    bytes::Bytes,
    tokio::io::{AsyncRead, AsyncWrite},
    tokio_tungstenite::WebSocketStream,
    tungstenite::{
        Error as NativeError,
        Message as NativeMessage,
        Utf8Bytes,
        protocol::CloseFrame as NativeCloseFrame,
    },
};

impl<T> Backend for WebSocketStream<T>
where
    T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Error = NativeError;
    type Message = NativeMessage;
    type Transport = Self;

    fn into_transport(self) -> Self::Transport {
        self
    }

    fn encode_message(msg: Message) -> Self::Message {
        match msg {
            Message::Binary(data) => NativeMessage::binary(data),
            Message::Text(data) => NativeMessage::text(data),
            Message::Ping(data) => NativeMessage::Ping(data),
            Message::Pong(data) => NativeMessage::Pong(data),
            Message::Close(msg) => {
                let frame = msg.map(|frame| NativeCloseFrame {
                    code: frame.code.into(),
                    reason: frame.reason.into(),
                });

                NativeMessage::Close(frame)
            }
        }
    }

    fn decode_message(msg: Self::Message) -> Message {
        match msg {
            NativeMessage::Binary(data) => Message::Binary(data),
            NativeMessage::Text(data) => Message::Text(bytes_to_string(data)),
            NativeMessage::Ping(data) => Message::Ping(data),
            NativeMessage::Pong(data) => Message::Pong(data),
            NativeMessage::Close(frame) => {
                let frame = frame.map(|frame| CloseFrame {
                    code: frame.code.into(),
                    reason: bytes_to_string(frame.reason),
                });

                Message::Close(frame)
            }
            NativeMessage::Frame(_) => Message::Close(None),
        }
    }
}

fn bytes_to_string(data: Utf8Bytes) -> String {
    String::from_utf8(Bytes::from(data).into()).unwrap_or_default()
}
