#[cfg(test)]
mod test;

pub const PROTOCOL_VERSION: u8 = 1;

pub mod messages {
    use std::borrow::BorrowMut;
    use std::io::{BufRead, Read, Write};

    include!(concat!(env!("OUT_DIR"), "/battleshipplus.rs"));
    include!(concat!(env!("OUT_DIR"), "/battleshipplus_op_codes.rs"));

    pub enum MessageEncodingError {
        IO(String),
        PROTOCOL(String),
    }

    const MESSAGE_HEADER_SIZE: usize = 4;

    pub struct Message {
        version: u8,
        op_code: OpCode,
        payload: Vec<u8>,
    }

    impl Message {
        pub fn version(&self) -> u8 { self.version }
        pub fn op_code(&self) -> OpCode { self.op_code }
        pub fn payload_length(&self) -> usize { self.payload.len() as usize }
        pub fn payload(&self) -> &Vec<u8> { &self.payload }

        pub fn decode(rd: &mut dyn BufRead) -> Result<Message, MessageEncodingError> {
            let mut buf = [0u8; 4];
            match rd.read_exact(buf.borrow_mut()) {
                Ok(_) => {}
                Err(e) =>
                    return Err(MessageEncodingError::IO(format!("unable to read header from buffer: {}", &e)))
            }

            let version = buf[0];

            let op_code = match OpCode::try_from(buf[1]) {
                Ok(op_code) => op_code,
                Err(e) => return Err(MessageEncodingError::PROTOCOL(String::from(e)))
            };

            let payload_length = (buf[2] as u16) << 8 | (buf[3] as u16);
            let mut payload = vec![0u8; payload_length as usize];

            match rd.read_exact(payload.borrow_mut()) {
                Ok(_) => {}
                Err(e) =>
                    return Err(MessageEncodingError::IO(format!("unable to read header from buffer: {}", e)))
            }

            Ok(Message {
                version,
                op_code,
                payload,
            })
        }

        pub fn new(version: u8, op_code: OpCode, payload: &[u8]) -> Result<Message, MessageEncodingError> {
            if payload.len() > u16::MAX as usize {
                return Err(MessageEncodingError::PROTOCOL(String::from("message payload is too long")));
            }

            let mut payload = Vec::from(payload);

            Ok(Message {
                version,
                op_code,
                payload,
            })
        }

        pub fn encode(&self) -> Vec<u8> {
            let mut buf = vec![0u8; self.payload_length() + MESSAGE_HEADER_SIZE];

            buf[0] = self.version;
            buf[1] = self.op_code.into();

            let len = self.payload_length();
            buf[2] = (len >> 8) as u8;
            buf[3] = len as u8;

            for i in 0..len {
                buf[MESSAGE_HEADER_SIZE + i] = self.payload[i];
            }

            buf
        }

        pub fn write_to(&self, writer: &mut dyn Write) -> Result<(), MessageEncodingError> {
            let buf = self.encode();
            match writer.write_all(&buf) {
                Ok(_) => Ok(()),
                Err(e) => Err(MessageEncodingError::IO(format!("unable to write message: {}", e)))
            }
        }
    }
}