//! Server half of RFC 6455, limited to what the session socket uses: text
//! messages, ping/pong, and close. No extensions are negotiated.

use std::io::{self, ErrorKind, Read, Write};

const HANDSHAKE_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

const OPCODE_CONTINUATION: u8 = 0x0;
const OPCODE_TEXT: u8 = 0x1;
const OPCODE_CLOSE: u8 = 0x8;
const OPCODE_PING: u8 = 0x9;
const OPCODE_PONG: u8 = 0xA;
const MAX_CONTROL_PAYLOAD_BYTES: usize = 125;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Incoming {
    Text(String),
    Ping(Vec<u8>),
    Pong,
    Close,
}

/// Value of `Sec-WebSocket-Accept` for a client's `Sec-WebSocket-Key`.
pub(super) fn accept_key(key: &str) -> String {
    let digest = sha1(format!("{}{HANDSHAKE_GUID}", key.trim()).as_bytes());
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, digest)
}

pub(super) fn write_handshake(writer: &mut impl Write, key: &str) -> io::Result<()> {
    write!(
        writer,
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
        accept_key(key)
    )?;
    writer.flush()
}

pub(super) fn write_text(writer: &mut impl Write, text: &str) -> io::Result<()> {
    write_frame(writer, OPCODE_TEXT, text.as_bytes())
}

pub(super) fn write_pong(writer: &mut impl Write, payload: &[u8]) -> io::Result<()> {
    write_frame(writer, OPCODE_PONG, payload)
}

/// `reason` must be a short ASCII code; it shares the control-frame budget.
pub(super) fn write_close(writer: &mut impl Write, code: u16, reason: &str) -> io::Result<()> {
    let mut payload = code.to_be_bytes().to_vec();
    payload.extend_from_slice(reason.as_bytes());
    payload.truncate(MAX_CONTROL_PAYLOAD_BYTES);
    write_frame(writer, OPCODE_CLOSE, &payload)
}

/// Server frames are never masked and never fragmented.
fn write_frame(writer: &mut impl Write, opcode: u8, payload: &[u8]) -> io::Result<()> {
    let mut header = Vec::with_capacity(10);
    header.push(0x80 | opcode);
    match payload.len() {
        length @ 0..=125 => header.push(length as u8),
        length @ 126..=0xFFFF => {
            header.push(126);
            header.extend_from_slice(&(length as u16).to_be_bytes());
        }
        length => {
            header.push(127);
            header.extend_from_slice(&(length as u64).to_be_bytes());
        }
    }
    writer.write_all(&header)?;
    writer.write_all(payload)?;
    writer.flush()
}

/// Reassembles client messages. Any error leaves the stream mid-frame, so the
/// caller must drop the connection rather than read again.
pub(super) struct MessageReader<R> {
    reader: R,
    max_message_bytes: usize,
    /// Fragments received so far; control frames may arrive between them.
    partial: Option<Vec<u8>>,
}

impl<R: Read> MessageReader<R> {
    pub(super) fn new(reader: R, max_message_bytes: usize) -> Self {
        Self {
            reader,
            max_message_bytes,
            partial: None,
        }
    }

    pub(super) fn read(&mut self) -> io::Result<Incoming> {
        loop {
            let mut header = [0_u8; 2];
            self.reader.read_exact(&mut header)?;
            let finished = header[0] & 0x80 != 0;
            if header[0] & 0x70 != 0 {
                return Err(violation("reserved bits are set"));
            }
            // RFC 6455 5.1: a server must fail the connection on an unmasked frame.
            if header[1] & 0x80 == 0 {
                return Err(violation("client frame is not masked"));
            }
            let opcode = header[0] & 0x0F;
            let is_control = opcode & 0x8 != 0;
            let length = self.payload_length(header[1] & 0x7F)?;
            if is_control && (!finished || length > MAX_CONTROL_PAYLOAD_BYTES) {
                return Err(violation("invalid control frame"));
            }
            let buffered = self.partial.as_ref().map_or(0, Vec::len);
            if length > self.max_message_bytes.saturating_sub(buffered) {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    "message is too large",
                ));
            }
            let mut mask = [0_u8; 4];
            self.reader.read_exact(&mut mask)?;
            let mut payload = vec![0_u8; length];
            self.reader.read_exact(&mut payload)?;
            for (index, byte) in payload.iter_mut().enumerate() {
                *byte ^= mask[index % 4];
            }
            match opcode {
                OPCODE_PING => return Ok(Incoming::Ping(payload)),
                OPCODE_PONG => return Ok(Incoming::Pong),
                OPCODE_CLOSE => return Ok(Incoming::Close),
                OPCODE_TEXT if self.partial.is_none() => self.partial = Some(payload),
                OPCODE_CONTINUATION => match self.partial.as_mut() {
                    Some(message) => message.extend_from_slice(&payload),
                    None => return Err(violation("continuation without a message")),
                },
                _ => return Err(violation("unsupported frame")),
            }
            if finished {
                let bytes = self
                    .partial
                    .take()
                    .expect("a data frame started the message");
                return String::from_utf8(bytes)
                    .map(Incoming::Text)
                    .map_err(|_| violation("text is not UTF-8"));
            }
        }
    }

    fn payload_length(&mut self, short: u8) -> io::Result<usize> {
        let length = match short {
            126 => {
                let mut bytes = [0_u8; 2];
                self.reader.read_exact(&mut bytes)?;
                u64::from(u16::from_be_bytes(bytes))
            }
            127 => {
                let mut bytes = [0_u8; 8];
                self.reader.read_exact(&mut bytes)?;
                u64::from_be_bytes(bytes)
            }
            short => u64::from(short),
        };
        usize::try_from(length).map_err(|_| violation("frame length overflows"))
    }
}

fn violation(reason: &'static str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, reason)
}

/// SHA-1 exists here only because the opening handshake mandates it; it
/// protects nothing. Kept local to avoid a dependency for one digest.
fn sha1(input: &[u8]) -> [u8; 20] {
    let mut state: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    let mut padded = input.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&((input.len() as u64) * 8).to_be_bytes());
    for block in padded.chunks_exact(64) {
        let mut words = [0_u32; 80];
        for (index, word) in block.chunks_exact(4).enumerate() {
            words[index] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for index in 16..80 {
            words[index] =
                (words[index - 3] ^ words[index - 8] ^ words[index - 14] ^ words[index - 16])
                    .rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = state;
        for (index, word) in words.iter().enumerate() {
            let (mix, constant) = match index {
                0..=19 => ((b & c) | (!b & d), 0x5A827999),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6_u32),
            };
            let next = a
                .rotate_left(5)
                .wrapping_add(mix)
                .wrapping_add(e)
                .wrapping_add(constant)
                .wrapping_add(*word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = next;
        }
        for (slot, value) in state.iter_mut().zip([a, b, c, d, e]) {
            *slot = slot.wrapping_add(value);
        }
    }
    let mut digest = [0_u8; 20];
    for (chunk, value) in digest.chunks_exact_mut(4).zip(state) {
        chunk.copy_from_slice(&value.to_be_bytes());
    }
    digest
}

#[cfg(test)]
#[path = "websocket_tests.rs"]
mod tests;
