//! Bounded newline-delimited input.

use std::io::BufRead;

/// Read one newline-terminated line, capping retained bytes at `max_bytes`
/// and discarding (without buffering) whatever overruns the cap - so an
/// over-long line still drains from the pipe instead of blocking the writer.
/// `Ok(false)` at EOF with nothing pending.
pub(crate) fn read_bounded_line<R: BufRead>(
    reader: &mut R,
    buf: &mut Vec<u8>,
    max_bytes: usize,
) -> std::io::Result<bool> {
    buf.clear();
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Ok(false);
        }
        if let Some(newline_pos) = available.iter().position(|&b| b == b'\n') {
            let line_part = &available[..newline_pos];
            let room = max_bytes.saturating_sub(buf.len());
            buf.extend_from_slice(&line_part[..line_part.len().min(room)]);
            let consumed = newline_pos + 1;
            reader.consume(consumed);
            return Ok(true);
        }
        let room = max_bytes.saturating_sub(buf.len());
        let take = available.len().min(room);
        buf.extend_from_slice(&available[..take]);
        let consumed = available.len();
        reader.consume(consumed);
    }
}

#[cfg(test)]
mod tests {
    use super::read_bounded_line;
    use std::io::Cursor;

    #[test]
    fn line_under_cap_is_kept() {
        let mut reader = Cursor::new(b"hello\n");
        let mut buf = Vec::new();
        assert!(read_bounded_line(&mut reader, &mut buf, 10).unwrap());
        assert_eq!(buf, b"hello");
    }

    #[test]
    fn overrun_is_dropped_and_stream_stays_aligned() {
        let mut reader = Cursor::new(b"abcdefgh\nok\n");
        let mut buf = Vec::new();
        assert!(read_bounded_line(&mut reader, &mut buf, 4).unwrap());
        assert_eq!(buf, b"abcd");
        assert!(read_bounded_line(&mut reader, &mut buf, 4).unwrap());
        assert_eq!(buf, b"ok");
    }

    #[test]
    fn eof_with_nothing_pending_is_false() {
        let mut reader = Cursor::new(b"");
        let mut buf = Vec::new();
        assert!(!read_bounded_line(&mut reader, &mut buf, 4).unwrap());
    }
}
