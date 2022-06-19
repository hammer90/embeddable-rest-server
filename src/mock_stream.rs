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
