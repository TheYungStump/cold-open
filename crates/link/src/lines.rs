//! Reading one line at a time with a size limit, so a peer can never make
//! the other side buffer without end.

use std::io::{self, BufRead, BufReader, Read};

pub struct LineReader<R> {
    inner: BufReader<R>,
}

impl<R: Read> LineReader<R> {
    pub fn new(r: R) -> LineReader<R> {
        LineReader {
            inner: BufReader::new(r),
        }
    }

    /// The next line without its line ending, `None` at the end of the
    /// stream. A line longer than `limit` bytes is an `InvalidData` error,
    /// and the stream should then be closed.
    pub fn read_line(&mut self, limit: usize) -> io::Result<Option<String>> {
        let mut buf = Vec::new();
        let n = (&mut self.inner)
            .take(limit as u64 + 1)
            .read_until(b'\n', &mut buf)?;
        if n == 0 {
            return Ok(None);
        }
        if buf.last() == Some(&b'\n') {
            buf.pop();
            if buf.last() == Some(&b'\r') {
                buf.pop();
            }
        } else if buf.len() > limit {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "line too long"));
        }
        String::from_utf8(buf)
            .map(Some)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "line is not UTF-8"))
    }

    pub fn get_ref(&self) -> &R {
        self.inner.get_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_and_limits() {
        let mut r = LineReader::new(&b"one\r\ntwo\nthree"[..]);
        assert_eq!(r.read_line(10).unwrap().as_deref(), Some("one"));
        assert_eq!(r.read_line(10).unwrap().as_deref(), Some("two"));
        assert_eq!(r.read_line(10).unwrap().as_deref(), Some("three"));
        assert_eq!(r.read_line(10).unwrap(), None);
        let mut r = LineReader::new(&b"0123456789AB\n"[..]);
        assert!(r.read_line(10).is_err());
        let mut r = LineReader::new(&b"0123456789\n"[..]);
        assert_eq!(r.read_line(10).unwrap().as_deref(), Some("0123456789"));
    }
}
