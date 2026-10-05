use std::io::Cursor;

use super::*;

fn client_frame(first_byte: u8, payload: &[u8]) -> Vec<u8> {
    let mask = [0x11, 0x22, 0x33, 0x44];
    let mut frame = vec![first_byte];
    match payload.len() {
        length @ 0..=125 => frame.push(0x80 | length as u8),
        length @ 126..=0xFFFF => {
            frame.push(0x80 | 126);
            frame.extend_from_slice(&(length as u16).to_be_bytes());
        }
        length => {
            frame.push(0x80 | 127);
            frame.extend_from_slice(&(length as u64).to_be_bytes());
        }
    }
    frame.extend_from_slice(&mask);
    frame.extend(
        payload
            .iter()
            .enumerate()
            .map(|(index, byte)| byte ^ mask[index % 4]),
    );
    frame
}

fn reader(bytes: Vec<u8>) -> MessageReader<Cursor<Vec<u8>>> {
    MessageReader::new(Cursor::new(bytes), 1024 * 1024)
}

#[test]
fn accept_key_matches_the_rfc_6455_example() {
    assert_eq!(
        accept_key("dGhlIHNhbXBsZSBub25jZQ=="),
        "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
    );
}

#[test]
fn sha1_matches_known_digests_across_block_boundaries() {
    let hex = |input: &[u8]| {
        sha1(input)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    };
    assert_eq!(hex(b""), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
    assert_eq!(hex(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
    assert_eq!(
        hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
        "84983e441c3bd26ebaae4aa1f95129e5e54670f1"
    );
}

#[test]
fn reads_a_masked_text_message() {
    assert_eq!(
        reader(client_frame(0x81, b"hello")).read().unwrap(),
        Incoming::Text("hello".into())
    );
}

#[test]
fn reads_extended_lengths() {
    let medium = "m".repeat(300);
    let large = "l".repeat(70_000);
    let mut bytes = client_frame(0x81, medium.as_bytes());
    bytes.extend(client_frame(0x81, large.as_bytes()));
    let mut reader = reader(bytes);
    assert_eq!(reader.read().unwrap(), Incoming::Text(medium));
    assert_eq!(reader.read().unwrap(), Incoming::Text(large));
}

#[test]
fn reassembles_fragments_around_an_interleaved_ping() {
    let mut bytes = client_frame(0x01, b"hel");
    bytes.extend(client_frame(0x89, b"p"));
    bytes.extend(client_frame(0x80, b"lo"));
    let mut reader = reader(bytes);
    assert_eq!(reader.read().unwrap(), Incoming::Ping(b"p".to_vec()));
    assert_eq!(reader.read().unwrap(), Incoming::Text("hello".into()));
}

#[test]
fn rejects_a_continuation_without_a_message() {
    assert!(reader(client_frame(0x80, b"lo")).read().is_err());
}

#[test]
fn rejects_unmasked_binary_and_oversized_frames() {
    assert!(reader(vec![0x81, 0x02, b'h', b'i']).read().is_err());
    assert!(reader(client_frame(0x82, b"bin")).read().is_err());
    let mut small = MessageReader::new(Cursor::new(client_frame(0x81, b"too long")), 4);
    assert!(small.read().is_err());
}

#[test]
fn reports_close_and_pong() {
    let mut bytes = client_frame(0x8A, b"");
    bytes.extend(client_frame(0x88, &1000_u16.to_be_bytes()));
    let mut reader = reader(bytes);
    assert_eq!(reader.read().unwrap(), Incoming::Pong);
    assert_eq!(reader.read().unwrap(), Incoming::Close);
}

#[test]
fn writes_unmasked_frames_with_the_shortest_length_encoding() {
    let mut wire = Vec::new();
    write_text(&mut wire, "hi").unwrap();
    assert_eq!(wire, [0x81, 0x02, b'h', b'i']);

    let mut wire = Vec::new();
    write_text(&mut wire, &"m".repeat(300)).unwrap();
    assert_eq!(&wire[..4], [0x81, 126, 0x01, 0x2C]);

    let mut wire = Vec::new();
    write_close(&mut wire, 4410, "session_expired").unwrap();
    assert_eq!(&wire[..4], [0x88, 17, 0x11, 0x3A]);
    assert_eq!(&wire[4..], b"session_expired");
}
