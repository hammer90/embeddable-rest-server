use anyhow::Result;
use std::io::prelude::*;
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

fn main() -> Result<()> {
    let listener = TcpListener::bind("0.0.0.0:8080")?;

    for stream in listener.incoming() {
        if let Ok(stream) = stream {
            let _ = handle_connection(stream);
        }
    }
    Ok(())
}

fn handle_connection(mut stream: TcpStream) -> Result<()> {
    let mut buffer = [0; 1024];

    stream.read(&mut buffer)?;

    println!("Request: \n{}", String::from_utf8_lossy(&buffer[..]));

    let msgs = [
        "Hello",
        "World",
        "I'm",
        "chunked",
        "and",
        "I'm",
        "looooooooong!",
    ];
    let start = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n";

    stream.write(start.as_bytes())?;
    stream.flush()?;

    for msg in msgs {
        let chunk = format!("{:x}\r\n{}\r\n\r\n", msg.len() + 2, msg);
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
