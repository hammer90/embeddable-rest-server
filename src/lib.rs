use std::collections::HashMap;
use std::io::{prelude::*, BufReader, Error as IoError};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

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

pub trait Streamable: Iterator<Item = Vec<u8>> {
    fn trailer_names(&self) -> Vec<String>;
    fn trailers(&self) -> Vec<(String, String)>;
}

struct NoTrailers {
    base: Box<dyn Iterator<Item = Vec<u8>>>,
}

impl NoTrailers {
    fn new(base: Box<dyn Iterator<Item = Vec<u8>>>) -> Self {
        Self { base }
    }
}

impl Iterator for NoTrailers {
    type Item = Vec<u8>;

    fn next(&mut self) -> Option<Self::Item> {
        self.base.next()
    }
}

impl Streamable for NoTrailers {
    fn trailer_names(&self) -> Vec<String> {
        vec![]
    }

    fn trailers(&self) -> Vec<(String, String)> {
        vec![]
    }
}

pub enum BodyType {
    Fixed(Vec<u8>),
    Stream(Box<dyn Iterator<Item = Vec<u8>>>),
    StreamWithTrailers(Box<dyn Streamable>),
}

pub struct Response {
    pub status: u32,
    pub body: BodyType,
}

impl Response {
    pub fn fixed_string(status: u32, body: &str) -> Self {
        Response {
            status,
            body: BodyType::Fixed(body.as_bytes().to_vec()),
        }
    }
}

pub struct Request {
    pub params: Option<HashMap<String, String>>,
    pub query: Option<String>,
    pub headers: Option<HashMap<String, String>>,
    pub data: Option<Vec<u8>>,
}

pub type RouteFn = fn(req: Request) -> Response;

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
    shutdown: Arc<Mutex<bool>>,
}

impl RestServer {
    pub fn new<A>(addr: A) -> Result<Self, HttpError>
    where
        A: ToSocketAddrs,
    {
        let listener = TcpListener::bind(addr)?;
        listener.set_nonblocking(true)?;
        let shutdown = Arc::new(Mutex::new(false));
        Ok(Self {
            listener,
            routes: HashMap::new(),
            shutdown,
        })
    }

    pub fn start(&self) -> Result<(), HttpError> {
        let stop = self.shutdown.clone();
        for stream in self.listener.incoming() {
            if let Ok(stream) = stream {
                let result = self.handle_connection(stream);
                if let Err(err) = result {
                    println!("{:?}", err);
                }
            }
            if *stop.lock().unwrap() {
                println!("shutting down");
                break;
            }
            thread::sleep(Duration::from_millis(100));
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
                let resp = route(Request {
                    params: None,
                    query: parsed.query,
                    headers: None,
                    data: None,
                });

                match resp.body {
                    BodyType::Fixed(body) => self.fixed_response(stream, resp.status, &body),
                    BodyType::StreamWithTrailers(body) => {
                        self.stream_response(stream, resp.status, body)
                    }
                    BodyType::Stream(body) => {
                        self.stream_response(stream, resp.status, Box::new(NoTrailers::new(body)))
                    }
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
            505,
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
        mut body: Box<dyn Streamable>,
    ) -> Result<(), HttpError> {
        let start = format!(
            "HTTP/1.1 {} {}\r\nTransfer-Encoding: chunked\r\n",
            status,
            status_text(status),
        );

        stream.write(start.as_bytes())?;
        let trailer_names = body.trailer_names();
        let has_trailers = trailer_names.len() > 0;
        if has_trailers {
            stream.write(format!("Trailers: {}\r\n", trailer_names.join(",")).as_bytes())?;
        }
        stream.write("\r\n".as_bytes())?;
        stream.flush()?;

        while let Some(data) = body.next() {
            let chunk_head = format!("{:x}\r\n", data.len());
            stream.write(chunk_head.as_bytes())?;
            stream.write(&data)?;
            stream.write("\r\n".as_bytes())?;
            stream.flush()?;
        }

        stream.write("0\r\n".as_bytes())?;
        if has_trailers {
            let trailers = body.trailers();
            for trailer in trailers {
                stream.write(format!("{}: {}\r\n", trailer.0, trailer.1).as_bytes())?;
            }
        }
        stream.write("\r\n".as_bytes())?;
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
            "HTTP/1.1 {} {}\r\nContent-Length: {}\r\n\r\n",
            status,
            status_text(status),
            body.len()
        );
        stream.write(start.as_bytes())?;
        stream.write(&body)?;
        stream.flush()?;

        Ok(())
    }
}

pub struct SpawnedRestServer {
    _handle: JoinHandle<Result<(), HttpError>>,
    stop: Arc<Mutex<bool>>,
}

impl SpawnedRestServer {
    pub fn spawn(server: RestServer) -> Self {
        let stop = server.shutdown.clone();
        let handle = thread::spawn(move || server.start());
        SpawnedRestServer {
            _handle: handle,
            stop,
        }
    }

    pub fn stop(&self) {
        let mut shutdown_lock = self.stop.lock().unwrap();
        *shutdown_lock = true;
    }
}

impl Drop for SpawnedRestServer {
    fn drop(&mut self) {
        self.stop();
        thread::sleep(Duration::from_millis(100));
    }
}

pub fn status_text(status: u32) -> &'static str {
    match status {
        100 => "Continue",
        101 => "Switching Protocols",
        102 => "Processing",
        103 => "Early Hints",
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        203 => "Non-Authoritative Information",
        204 => "No Content",
        205 => "Reset Content",
        206 => "Partial Content",
        207 => "Multi-Status",
        208 => "Already Reported",
        226 => "IM Used",
        300 => "Multiple Choices",
        301 => "Moved Permanently",
        302 => "Found",
        303 => "See Other",
        304 => "Not Modified",
        305 => "Use Proxy",
        306 => "Switch Proxy",
        307 => "Temporary Redirect",
        308 => "Permanent Redirect",
        400 => "Bad Request",
        401 => "Unauthorized",
        402 => "Payment Required",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        406 => "Not Acceptable",
        407 => "Proxy Authentication Required",
        408 => "Request Timeout",
        409 => "Conflict",
        410 => "Gone",
        411 => "Length Required",
        412 => "Precondition Failed",
        413 => "Payload Too Large",
        414 => "URI Too Long",
        415 => "Unsupported Media Type",
        416 => "Range Not Satisfiable",
        417 => "Expectation Failed",
        418 => "I'm a teapot",
        421 => "Misdirected Request",
        422 => "Unprocessable Entity",
        423 => "Locked",
        424 => "Failed Dependency",
        426 => "Upgrade Required",
        428 => "Precondition Required",
        429 => "Too Many Requests",
        431 => "Request Header Fields Too Large",
        451 => "Unavailable For Legal Reasons",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        505 => "Http Version Not Supported",
        506 => "Variant Also Negotiates",
        507 => "Insufficient Storage",
        508 => "Loop Detected",
        510 => "Not Extended",
        511 => "Network Authentication Required",
        _ => "Unknown",
    }
}
