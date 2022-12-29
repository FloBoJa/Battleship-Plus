pub const PROTOCOL_VERSION: u8 = 1;

pub mod types {
    include!(concat!(env!("OUT_DIR"), "/battleshipplus.types.rs"));
}

#[allow(clippy::large_enum_variant)]
pub mod messages {
    pub use crate::messages::packet_payload::ProtocolMessage;

    include!(concat!(env!("OUT_DIR"), "/battleshipplus.messages.rs"));
}

pub mod codec {
    use std::fmt::{Display, Formatter};

    use bytes::{Buf, BufMut, BytesMut};
    pub use prost::Message as ProstMessage;
    use tokio_util::codec::{Decoder, Encoder};

    use crate::messages;

    #[derive(Clone, Debug)]
    pub enum CodecError {
        IO(String),
        PROTOCOL(String),
    }

    impl Display for CodecError {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            match self {
                CodecError::IO(s) => f.write_str(format!("IO: {s}").as_str()),
                CodecError::PROTOCOL(s) => f.write_str(format!("PROTOCOL: {s}").as_str()),
            }
        }
    }

    impl From<std::io::Error> for CodecError {
        fn from(io_error: std::io::Error) -> CodecError {
            CodecError::IO(io_error.to_string())
        }
    }

    const HEADER_SIZE: usize = 3;

    pub struct BattleshipPlusCodec {
        version: u8,
        length: Option<usize>,
    }

    impl Default for BattleshipPlusCodec {
        fn default() -> BattleshipPlusCodec {
            BattleshipPlusCodec {
                version: crate::PROTOCOL_VERSION,
                length: None,
            }
        }
    }

    impl Encoder<messages::ProtocolMessage> for BattleshipPlusCodec {
        type Error = CodecError;

        fn encode(
            &mut self,
            message: messages::ProtocolMessage,
            buffer: &mut BytesMut,
        ) -> Result<(), Self::Error> {
            let payload = messages::PacketPayload {
                protocol_message: Some(message),
            };

            let length = payload.encoded_len();
            if length > u16::MAX as usize {
                return Err(CodecError::PROTOCOL(String::from("message is too long")));
            }
            let length = length as u16;

            buffer.put_u8(self.version);
            buffer.put_u16(length);
            payload
                .encode(buffer)
                .map_err(|error| CodecError::IO(error.to_string()))
        }
    }

    impl Decoder for BattleshipPlusCodec {
        // Item is Option<ProtocolMessage> instead of ProtocolMessage here since erroring in a Decoder due to
        // `payload.protocol_message == None` would immediately close the connection.
        // This behaviour would not be appropriate since the error is recoverable.
        type Item = Option<messages::ProtocolMessage>;
        type Error = CodecError;

        fn decode(&mut self, buffer: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
            // Try to read the header if it has not been read yet.
            if self.length.is_none() {
                if buffer.len() < HEADER_SIZE {
                    buffer.reserve(HEADER_SIZE - buffer.len());
                    return Ok(None);
                }

                let version = buffer.get_u8();
                let length = buffer.get_u16() as usize;
                self.length = Some(length);

                if version != self.version {
                    return Err(CodecError::PROTOCOL(format!(
                        "unsupported protocol version {}, supported version is {}",
                        version, self.version
                    )));
                }

                // Reserve enough memory for this message and the next header.
                if length + HEADER_SIZE > buffer.capacity() {
                    buffer.reserve(length + HEADER_SIZE - buffer.len());
                }
            }

            // Try to read the message if the header has successfully been read.
            if let Some(length) = self.length {
                if buffer.len() < length {
                    return Ok(None);
                }

                // Reset length for next message.
                self.length = None;

                // Decode the message.
                match messages::PacketPayload::decode(buffer.split_to(length)) {
                    Ok(payload) => Ok(Some(payload.protocol_message)),
                    Err(error) => Err(CodecError::PROTOCOL(format!(
                        "malformed message, expecting PacketPayload: {error}"
                    ))),
                }
            } else {
                Ok(None)
            }
        }
    }

    #[test]
    fn encode() {
        let expected_message = messages::ProtocolMessage::JoinRequest(messages::JoinRequest {
            username: "Example P. Name Sr.".to_string(),
        });

        let mut codec = BattleshipPlusCodec::default();
        let mut buffer = BytesMut::new();
        codec
            .encode(expected_message.clone(), &mut buffer)
            .expect("Encoding does not fail");

        let expected_payload = messages::PacketPayload {
            protocol_message: Some(expected_message.clone()),
        };

        assert_eq!(buffer.get_u8(), crate::PROTOCOL_VERSION);
        assert_eq!(buffer.get_u16() as usize, expected_payload.encoded_len());
        assert_eq!(buffer.len(), expected_payload.encoded_len());

        let decoded_payload = match messages::PacketPayload::decode(buffer) {
            Err(error) => panic!("Prost decoding failed: {error}"),
            Ok(value) => value,
        };
        let decoded_message = decoded_payload
            .protocol_message
            .expect("The message is not empty");

        assert_eq!(expected_message, decoded_message);
    }

    #[test]
    fn decode() {
        let expected_message = messages::ProtocolMessage::JoinRequest(messages::JoinRequest {
            username: "Example P. Name Sr.".to_string(),
        });
        let expected_payload = messages::PacketPayload {
            protocol_message: Some(expected_message.clone()),
        };

        let mut buffer = BytesMut::new();
        buffer.put_u8(crate::PROTOCOL_VERSION);
        buffer.put_u16(expected_payload.encoded_len() as u16);
        expected_payload
            .encode(&mut buffer)
            .expect("Prost encoding does not fail");

        let mut codec = BattleshipPlusCodec::default();
        let decoded_message = codec
            .decode(&mut buffer)
            .expect("No error occurs during decoding")
            .expect("An entire message is in the buffer")
            .expect("The message could not be empty");

        assert_eq!(expected_message, decoded_message);
    }

    #[test]
    fn encode_then_decode() {
        let expected_message = messages::ProtocolMessage::JoinRequest(messages::JoinRequest {
            username: "Example P. Name Sr.".to_string(),
        });

        let mut codec = BattleshipPlusCodec::default();
        let mut buffer = BytesMut::new();
        codec
            .encode(expected_message.clone(), &mut buffer)
            .expect("Encoding does not fail");

        let decoded_message = codec
            .decode(&mut buffer)
            .expect("No error occurs during decoding")
            .expect("An entire message is in the buffer")
            .expect("The message could not be empty");

        assert_eq!(expected_message, decoded_message);
    }
}
