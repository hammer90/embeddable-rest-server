use std::collections::HashMap;
use std::io::{prelude::*, BufReader, Error as IoError};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};

#[derive(Debug)]
pub enum HttpError {
    RouteExists,
    IO(IoError),
}

impl From<IoError> for HttpError {
    fn from(err: IoError) -> HttpError {
        HttpError::IO(err)
    }
}

pub type Streamable = Box<dyn Iterator<Item = Vec<u8>>>;

pub enum BodyType {
    Fixed(Vec<u8>),
    Stream(Streamable),
}

pub struct Response {
    pub status: u32,
    pub body: BodyType,
}

type RouteFn = fn(query: Option<String>, data: Vec<u8>) -> Response;

struct ParsedFirstLine {
    method: String,
    path: String,
    query: Option<String>,
    version: String,
}

impl ParsedFirstLine {
    fn parse(line: String) -> Result<Self, ()> {
        let splitted: Vec<&str> = line.split(' ').collect();
        if splitted.len() != 3 {
            return Err(());
        }
        let path_query = splitted[1].split_once('?');
        if let Some((path, query)) = path_query {
            Ok(Self {
                method: splitted[0].to_string(),
                path: path.to_string(),
                query: Some(query.to_string()),
                version: splitted[2].to_string(),
            })
        } else {
            Ok(Self {
                method: splitted[0].to_string(),
                path: splitted[1].to_string(),
                query: None,
                version: splitted[2].to_string(),
            })
        }
    }
}

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
                let result = self.handle_connection(stream);
                if let Err(err) = result {
                    println!("{:?}", err);
                }
            }
        }
        Ok(())
    }

    pub fn get(mut self, route: &str, func: RouteFn) -> Result<Self, HttpError> {
        if let Some(_) = self.routes.get(route) {
            return Err(HttpError::RouteExists);
        }
        self.routes.insert(format!("GET {}", route), func);
        Ok(self)
    }

    fn handle_connection(&self, stream: TcpStream) -> Result<(), HttpError> {
        let mut reader = BufReader::new(stream.try_clone()?);
        let mut start = String::new();
        let len = reader.read_line(&mut start)?;
        if len == 0 {
            return self.send_not_http_conform_request(stream);
        }
        let parsed = ParsedFirstLine::parse(start);
        if let Ok(parsed) = parsed {
            if !parsed.version.starts_with("HTTP/1.1") {
                return self.send_unsupported_version(stream, parsed.version);
            }

            let route_key = format!("{} {}", parsed.method, parsed.path);
            if let Some(route) = self.routes.get(&route_key) {
                let resp = route(parsed.query, vec![]);

                match resp.body {
                    BodyType::Fixed(body) => self.fixed_response(stream, resp.status, &body),
                    BodyType::Stream(body) => self.stream_response(stream, resp.status, body),
                }?;
            } else {
                return self.send_not_found(stream, parsed.path);
            }
        } else {
            return self.send_not_http_conform_request(stream);
        }
        Ok(())
    }

    fn send_not_http_conform_request(&self, stream: TcpStream) -> Result<(), HttpError> {
        self.fixed_response(
            stream,
            400,
            &"Not HTTP conform request\r\n".as_bytes().to_vec(),
        )
    }

    fn send_unsupported_version(
        &self,
        stream: TcpStream,
        version: String,
    ) -> Result<(), HttpError> {
        self.fixed_response(
            stream,
            400,
            &format!("Verion {} not supported\r\n", version)
                .as_bytes()
                .to_vec(),
        )
    }

    fn send_not_found(&self, stream: TcpStream, path: String) -> Result<(), HttpError> {
        self.fixed_response(
            stream,
            404,
            &format!("Route {} does not exists\r\n", path)
                .as_bytes()
                .to_vec(),
        )
    }

    fn stream_response(
        &self,
        mut stream: TcpStream,
        status: u32,
        mut body: Streamable,
    ) -> Result<(), HttpError> {
        let start = format!(
            "HTTP/1.1 {} OK\r\nTransfer-Encoding: chunked\r\n\r\n",
            status
        );

        stream.write(start.as_bytes())?;
        stream.flush()?;

        while let Some(data) = body.next() {
            let chunk_head = format!("{:x}\r\n", data.len());
            stream.write(chunk_head.as_bytes())?;
            stream.write(&data)?;
            stream.write("\r\n".as_bytes())?;
            stream.flush()?;
        }

        stream.write("0\r\n\r\n".as_bytes())?;
        stream.flush()?;

        Ok(())
    }

    fn fixed_response(
        &self,
        mut stream: TcpStream,
        status: u32,
        body: &Vec<u8>,
    ) -> Result<(), HttpError> {
        let start = format!(
            "HTTP/1.1 {} OK\r\nContent-Length: {}\r\n\r\n",
            status,
            body.len()
        );
        stream.write(start.as_bytes())?;
        stream.write(&body)?;
        stream.flush()?;

        Ok(())
    }
}
