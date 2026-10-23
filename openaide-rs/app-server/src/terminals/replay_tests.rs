use super::Replay;

#[test]
fn keeps_everything_within_limits() {
    let mut replay = Replay::with_limits(3, 100);
    replay.push(b"one\ntwo\n");
    replay.push(b"thr");
    assert_eq!(replay.contents(), b"one\ntwo\nthr");
}

#[test]
fn drops_whole_oldest_lines_past_the_newline_limit() {
    let mut replay = Replay::with_limits(2, 100);
    replay.push(b"one\ntwo\nthree\nfour");
    assert_eq!(replay.contents(), b"two\nthree\nfour");
}

#[test]
fn cuts_at_a_newline_past_the_byte_limit() {
    let mut replay = Replay::with_limits(100, 10);
    replay.push(b"aaaa\nbbbb\ncccc");
    assert_eq!(replay.contents(), b"bbbb\ncccc");
}

#[test]
fn cuts_bytes_when_output_has_no_newline() {
    let mut replay = Replay::with_limits(100, 4);
    replay.push(b"abcdefgh");
    assert_eq!(replay.contents(), b"efgh");
}
