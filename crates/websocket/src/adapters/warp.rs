use {
    crate::{Adapter, CloseFrame, Message},
    warp::{
        Error as NativeError,
        ws::{Message as NativeMessage, WebSocket as NativeWebSocket},
    },
};

impl Adapter for NativeWebSocket {
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
            Message::Ping(data) => NativeMessage::ping(data),
            Message::Pong(data) => NativeMessage::pong(data),
            Message::Close(msg) => match msg {
                Some(msg) => NativeMessage::close_with(msg.code, msg.reason),
                None => NativeMessage::close(),
            },
        }
    }

    fn decode_message(msg: Self::Message) -> Message {
        if msg.is_binary() {
            Message::Binary(msg.into_bytes())
        } else if msg.is_text() {
            Message::Text(String::from_utf8(msg.into_bytes().into()).unwrap_or_default())
        } else if msg.is_ping() {
            Message::Ping(msg.into_bytes())
        } else if msg.is_pong() {
            Message::Pong(msg.into_bytes())
        } else {
            let msg = msg.close_frame().map(|(code, msg)| CloseFrame {
                code,
                reason: msg.to_owned(),
            });

            Message::Close(msg)
        }
    }
}
