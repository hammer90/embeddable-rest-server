use std::collections::HashMap;
use std::io::{prelude::*, BufReader, Error as IoError};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};

#[derive(Debug)]
pub enum HttpError {
    RouteExists,
    BadRequest,
    IO(IoError),
}

impl From<IoError> for HttpError {
    fn from(err: IoError) -> HttpError {
        HttpError::IO(err)
    }
}

pub type Stream = fn(u32) -> Option<Vec<u8>>;

#[derive(Debug)]
pub struct Response {
    pub status: u32,
    pub data: Stream,
}

type RouteFn = fn(Vec<u8>) -> Response;

pub struct RestServer {
    listener: TcpListener,
    routes: HashMap<String, RouteFn>,
}

impl RestServer {
    pub fn new<A>(addr: A) -> Result<Self, HttpError>
    where
        A: ToSocketAddrs,
    {
        let listener = TcpListener::bind(addr)?;
        Ok(Self {
            listener,
            routes: HashMap::new(),
        })
    }

    pub fn start(&self) -> Result<(), HttpError> {
        for stream in self.listener.incoming() {
            if let Ok(stream) = stream {
                let _ = self.handle_connection(stream);
            }
        }
        Ok(())
    }

    pub fn get(mut self, route: &str, func: RouteFn) -> Result<Self, HttpError> {
        if let Some(_) = self.routes.get(route) {
            return Err(HttpError::RouteExists);
        }
        self.routes.insert(route.to_owned(), func);
        Ok(self)
    }

    fn handle_connection(&self, mut stream: TcpStream) -> Result<(), HttpError> {
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut start = String::new();
        let len = reader.read_line(&mut start)?;
        if len == 0 {
            return Err(HttpError::BadRequest);
        }
        let splitted: Vec<&str> = start.split(' ').collect();
        if let Some(route) = self.routes.get(splitted[1]) {
            let resp = route(vec![]);

            let start = format!(
                "HTTP/1.1 {} OK\r\nTransfer-Encoding: chunked\r\n\r\n",
                resp.status
            );

            stream.write(start.as_bytes())?;
            stream.flush()?;

            let data = resp.data;
            let mut count: u32 = 0;
            while let Some(data) = data(count) {
                count += 1;
                let chunk_head = format!("{:x}\r\n", data.len());
                stream.write(chunk_head.as_bytes())?;
                stream.write(&data[..])?;
                stream.write("\r\n".as_bytes())?;
                stream.flush()?;
            }
            stream.write("0\r\n\r\n".as_bytes())?;
            stream.flush()?;
        } else {
            stream.write("HTTP/1.1 404 NotFound\r\n\r\n".as_bytes())?;
            stream.flush()?;
        }
        Ok(())
    }
}
