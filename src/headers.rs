use std::collections::HashMap;
use std::io::{prelude::*, BufReader};

use crate::ResponseableError;

pub fn parse_headers<R: Read>(
    reader: &mut BufReader<R>,
) -> Result<HashMap<String, String>, ResponseableError> {
    let mut headers = HashMap::new();
    loop {
        let mut header = String::new();
        let len = reader.read_line(&mut header)?;
        if len == 0 || header == "\r\n" {
            break;
        }
        if let Some((name, value)) = header.split_once(": ") {
            headers.insert(
                name.to_string().to_lowercase(),
                value[0..(value.len() - 2)].to_string(),
            );
        } else {
            return Err(ResponseableError::BadHeader(header));
        }
    }
    Ok(headers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock_stream::MockReadableStream;

    #[test]
    fn lowercases_headers() {
        let stream = MockReadableStream::new(vec!["Host: localhost", "Content-Length: 42", ""]);
        let mut reader = BufReader::new(stream);

        assert_eq!(
            parse_headers(&mut reader),
            Ok(HashMap::from([
                ("host".to_string(), "localhost".to_string()),
                ("content-length".to_string(), "42".to_string())
            ]))
        );
        assert!(reader.buffer().is_empty());
    }

    #[test]
    fn missing_space() {
        let stream = MockReadableStream::new(vec!["Host:localhost", ""]);
        let mut reader = BufReader::new(stream);

        assert_eq!(
            parse_headers(&mut reader),
            Err(ResponseableError::BadHeader(
                "Host:localhost\r\n".to_string()
            ))
        )
    }
}
