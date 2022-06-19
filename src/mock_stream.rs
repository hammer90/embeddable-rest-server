use std::cmp::min;
use std::io::{Read, Result};

pub struct MockReadableStream {
    buf: Vec<u8>,
    offset: usize,
}

impl MockReadableStream {
    pub fn new(lines: Vec<&str>) -> Self {
        Self {
            buf: lines.join("\r\n").into_bytes(),
            offset: 0,
        }
    }
}

impl Read for MockReadableStream {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let remaining = self.buf.len() - self.offset;
        let count = min(remaining, buf.len());
        buf[..count].copy_from_slice(&self.buf[..count]);
        self.offset += count;
        Ok(count)
    }
}

mod tests {
    use std::io::BufReader;

    use super::*;

    #[test]
    fn merges_with_newline() {
        let stream = MockReadableStream::new(vec!["123", "456", ""]);
        let mut reader = BufReader::new(stream);

        let mut buf = String::new();
        let count = reader.read_to_string(&mut buf).unwrap();

        assert_eq!(count, 10);
        assert_eq!(buf, "123\r\n456\r\n");
    }

    #[test]
    fn handles_small_chunks() {
        let stream = MockReadableStream::new(vec!["1234567890", "1234567890", ""]);
        let mut reader = BufReader::with_capacity(5, stream);

        let mut buf = String::new();
        let count = reader.read_to_string(&mut buf).unwrap();

        assert_eq!(count, 24);
        assert_eq!(buf, "1234567890\r\n1234567890\r\n");
    }
}
