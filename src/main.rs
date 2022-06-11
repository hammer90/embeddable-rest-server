use std::io::{prelude::*, Error as IoError};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

fn main() -> Result<(), HttpError> {
    let listener = TcpListener::bind("0.0.0.0:8080")?;

    for stream in listener.incoming() {
        if let Ok(stream) = stream {
            let _ = handle_connection(stream);
        }
    }
    Ok(())
}

#[derive(Debug)]
enum HttpError {
    IO(IoError),
}

impl From<IoError> for HttpError {
    fn from(err: IoError) -> HttpError {
        HttpError::IO(err)
    }
}

fn handle_connection(mut stream: TcpStream) -> Result<(), HttpError> {
    let mut buffer = [0; 1024];

    stream.read(&mut buffer)?;

    println!("Request: \n{}", String::from_utf8_lossy(&buffer[..]));

    let msgs = [
        "Hello\r\n",
        "World\r\n",
        "I'm\r\n",
        "chunked\r\n",
        "and\r\n",
        "looooooooong!\r\n",
    ];
    let start = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n";

    stream.write(start.as_bytes())?;
    stream.flush()?;

    for msg in msgs {
        let chunk = format!("{:x}\r\n{}\r\n", msg.len(), msg);
        println!("{}", chunk);
        stream.write(chunk.as_bytes())?;
        stream.flush()?;
        thread::sleep(Duration::from_secs(1));
    }

    let stop = "0\r\n\r\n";
    stream.write(stop.as_bytes())?;
    stream.flush()?;
    Ok(())
}
