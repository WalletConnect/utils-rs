use {
    crate::{Backend, CloseFrame, Message},
    axum::{
        Error as NativeError,
        body::Bytes,
        extract::ws::{
            CloseFrame as NativeCloseFrame,
            Message as NativeMessage,
            Utf8Bytes,
            WebSocket as NativeWebSocket,
        },
    },
};

impl Backend for NativeWebSocket {
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
                    code: frame.code,
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
                    code: frame.code,
                    reason: bytes_to_string(frame.reason),
                });

                Message::Close(frame)
            }
        }
    }
}

fn bytes_to_string(data: Utf8Bytes) -> String {
    String::from_utf8(Bytes::from(data).into()).unwrap_or_default()
}
