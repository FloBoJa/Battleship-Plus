pub const PROTOCOL_VERSION: u8 = 1;

pub mod types {
    include!(concat!(env!("OUT_DIR"), "/battleshipplus.types.rs"));
}

#[allow(clippy::large_enum_variant)]
pub mod messages {
    use prost::Message as ProstMessage;
    use std::borrow::BorrowMut;
    use std::fmt::{Display, Formatter};
    use std::io::{BufRead, Write};

    include!(concat!(env!("OUT_DIR"), "/battleshipplus.messages.rs"));

    #[derive(Clone, Debug)]
    pub enum MessageEncodingError {
        IO(String),
        PROTOCOL(String),
    }

    impl Display for MessageEncodingError {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            match self {
                MessageEncodingError::IO(s) => f.write_str(format!("IO: {}", s).as_str()),
                MessageEncodingError::PROTOCOL(s) => {
                    f.write_str(format!("PROTOCOL: {}", s).as_str())
                }
            }
        }
    }

    const MESSAGE_HEADER_SIZE: usize = 3;

    pub struct Message {
        version: u8,
        payload: PacketPayload,
    }

    impl Message {
        pub fn version(&self) -> u8 {
            self.version
        }
        pub fn payload_length(&self) -> usize {
            self.payload.encoded_len()
        }
        pub fn inner_message(&self) -> &packet_payload::ProtocolMessage {
            if let Some(inner_message) = &self.payload.protocol_message {
                inner_message
            } else {
                panic!("PacketPayloads without inner messages are not constructible");
            }
        }

        pub fn decode(rd: &mut dyn BufRead) -> Result<Message, MessageEncodingError> {
            let mut buf = [0u8; MESSAGE_HEADER_SIZE];
            match rd.read_exact(buf.borrow_mut()) {
                Ok(_) => {}
                Err(e) => {
                    return Err(MessageEncodingError::IO(format!(
                        "unable to read header from buffer: {}",
                        &e
                    )))
                }
            }

            let version = buf[0];

            let payload_length = (buf[1] as u16) << 8 | (buf[2] as u16);
            let mut payload = vec![0u8; payload_length as usize];

            match rd.read_exact(payload.borrow_mut()) {
                Ok(_) => {}
                Err(e) => {
                    return Err(MessageEncodingError::IO(format!(
                        "unable to read header from buffer: {}",
                        e
                    )))
                }
            }

            let payload = match PacketPayload::decode(payload.as_slice()) {
                Ok(value) => value,
                Err(e) => {
                    return Err(MessageEncodingError::PROTOCOL(format!(
                        "malformed message, expecting PacketPayload: {}",
                        e
                    )))
                }
            };

            if payload.protocol_message.is_none() {
                return Err(MessageEncodingError::PROTOCOL(
                    "no message inside PacketPayload".to_string(),
                ));
            };

            Ok(Message { version, payload })
        }

        pub fn new(
            version: u8,
            protocol_message: packet_payload::ProtocolMessage,
        ) -> Result<Message, MessageEncodingError> {
            let payload = PacketPayload {
                protocol_message: Some(protocol_message),
            };

            if payload.encoded_len() > u16::MAX as usize {
                return Err(MessageEncodingError::PROTOCOL(String::from(
                    "message is too long",
                )));
            }

            Ok(Message { version, payload })
        }

        pub fn encode(&self) -> Vec<u8> {
            let mut buf = vec![0u8; self.payload_length() + MESSAGE_HEADER_SIZE];

            buf[0] = self.version;

            let len = self.payload_length();
            buf[1] = (len >> 8) as u8;
            buf[2] = len as u8;

            buf[MESSAGE_HEADER_SIZE..(len + MESSAGE_HEADER_SIZE)]
                .copy_from_slice(&self.payload.encode_to_vec());

            buf
        }

        pub fn write_to(&self, writer: &mut dyn Write) -> Result<(), MessageEncodingError> {
            let buf = self.encode();
            match writer.write_all(&buf) {
                Ok(_) => Ok(()),
                Err(e) => Err(MessageEncodingError::IO(format!(
                    "unable to write message: {}",
                    e
                ))),
            }
        }
    }
}
