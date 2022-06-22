mod headers;
mod parsed_first_line;
mod routes;
mod status_text;

#[cfg(test)]
mod mock_stream;

use std::collections::HashMap;
use std::io::{prelude::*, BufReader, Error as IoError};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use headers::parse_headers;
use parsed_first_line::ParsedFirstLine;
use routes::{Routes, RoutesError};
use status_text::status_text;

#[derive(Debug, PartialEq, Eq)]
pub enum ResponseableError {
    NotHttpConform,
    UnsupportedVersion(String),
    NotFound(String),
    BadHeader(String),
    MethodNotImplemented(String),
    LengthRequired,
    PayloadToLarge,
    IO,
}

#[derive(Debug)]
pub enum HttpError {
    RouteExists,
    IO(IoError),
    Responseable(ResponseableError),
}

impl From<IoError> for ResponseableError {
    fn from(_: IoError) -> ResponseableError {
        ResponseableError::IO
    }
}

impl From<IoError> for HttpError {
    fn from(err: IoError) -> HttpError {
        HttpError::IO(err)
    }
}

impl From<ResponseableError> for HttpError {
    fn from(err: ResponseableError) -> HttpError {
        HttpError::Responseable(err)
    }
}

impl From<RoutesError> for HttpError {
    fn from(_: RoutesError) -> HttpError {
        HttpError::RouteExists
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
    pub params: HashMap<String, String>,
    pub query: Option<String>,
    pub headers: HashMap<String, String>,
    pub data: Option<Vec<u8>>,
}

pub type RouteFn = fn(req: Request) -> Response;

#[derive(Debug, PartialEq, Eq)]
pub enum HttpVerbs {
    GET,
    POST,
    PUT,
    DELETE,
    PATCH,
}

impl HttpVerbs {
    fn map_method(method: &str) -> Result<Self, ResponseableError> {
        match method {
            "GET" => Ok(HttpVerbs::GET),
            "POST" => Ok(HttpVerbs::POST),
            "PUT" => Ok(HttpVerbs::PUT),
            "DELETE" => Ok(HttpVerbs::DELETE),
            "PATCH" => Ok(HttpVerbs::PATCH),
            _ => Err(ResponseableError::MethodNotImplemented(method.to_string())),
        }
    }
}

struct HttpRoutes {
    get: Routes<RouteFn>,
    post: Routes<RouteFn>,
    put: Routes<RouteFn>,
    patch: Routes<RouteFn>,
    delete: Routes<RouteFn>,
}

impl HttpRoutes {
    fn new() -> Self {
        Self {
            get: Routes::<RouteFn>::new(),
            post: Routes::<RouteFn>::new(),
            put: Routes::<RouteFn>::new(),
            patch: Routes::<RouteFn>::new(),
            delete: Routes::<RouteFn>::new(),
        }
    }

    fn add(self, verb: HttpVerbs, route: &str, func: RouteFn) -> Result<Self, RoutesError> {
        match verb {
            HttpVerbs::GET => Ok(Self {
                get: self.get.add(route, func)?,
                ..self
            }),
            HttpVerbs::POST => Ok(Self {
                post: self.post.add(route, func)?,
                ..self
            }),
            HttpVerbs::PUT => Ok(Self {
                put: self.put.add(route, func)?,
                ..self
            }),
            HttpVerbs::PATCH => Ok(Self {
                patch: self.patch.add(route, func)?,
                ..self
            }),
            HttpVerbs::DELETE => Ok(Self {
                delete: self.delete.add(route, func)?,
                ..self
            }),
        }
    }

    fn find_verb(&self, verb: &HttpVerbs) -> &Routes<RouteFn> {
        match verb {
            HttpVerbs::GET => &self.get,
            HttpVerbs::POST => &self.post,
            HttpVerbs::PUT => &self.put,
            HttpVerbs::PATCH => &self.patch,
            HttpVerbs::DELETE => &self.delete,
        }
    }

    fn find(&self, verb: &HttpVerbs, route: &str) -> Option<(RouteFn, HashMap<String, String>)> {
        self.find_verb(verb).find(route)
    }
}

pub struct RestServer {
    listener: TcpListener,
    routes: HttpRoutes,
    shutdown: Arc<Mutex<bool>>,
    buf_size: usize,
}

impl RestServer {
    pub fn new<A>(addr: A, buf_size: usize) -> Result<Self, HttpError>
    where
        A: ToSocketAddrs,
    {
        let listener = TcpListener::bind(addr)?;
        listener.set_nonblocking(true)?;
        let shutdown = Arc::new(Mutex::new(false));
        Ok(Self {
            listener,
            routes: HttpRoutes::new(),
            shutdown,
            buf_size,
        })
    }

    pub fn start(&self) -> Result<(), HttpError> {
        let stop = self.shutdown.clone();
        for stream in self.listener.incoming() {
            if let Ok(stream) = stream {
                let result = self.handle_connection_witherrors(stream);
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

    pub fn register(self, verb: HttpVerbs, route: &str, func: RouteFn) -> Result<Self, HttpError> {
        Ok(Self {
            routes: self.routes.add(verb, route, func)?,
            ..self
        })
    }

    pub fn get(self, route: &str, func: RouteFn) -> Result<Self, HttpError> {
        self.register(HttpVerbs::GET, route, func)
    }

    pub fn post(self, route: &str, func: RouteFn) -> Result<Self, HttpError> {
        self.register(HttpVerbs::POST, route, func)
    }

    pub fn put(self, route: &str, func: RouteFn) -> Result<Self, HttpError> {
        self.register(HttpVerbs::PUT, route, func)
    }

    pub fn delete(self, route: &str, func: RouteFn) -> Result<Self, HttpError> {
        self.register(HttpVerbs::DELETE, route, func)
    }

    pub fn patch(self, route: &str, func: RouteFn) -> Result<Self, HttpError> {
        self.register(HttpVerbs::PATCH, route, func)
    }

    fn handle_connection_witherrors(&self, stream: TcpStream) -> Result<(), HttpError> {
        let result = self.handle_connection(&stream);
        match result {
            Err(HttpError::Responseable(responseable)) => match responseable {
                ResponseableError::NotHttpConform => self.send_not_http_conform_request(stream),
                ResponseableError::UnsupportedVersion(version) => {
                    self.send_unsupported_version(stream, version)
                }
                ResponseableError::MethodNotImplemented(method) => {
                    self.send_method_not_implemented(stream, method)
                }
                ResponseableError::NotFound(path) => self.send_not_found(stream, path),
                ResponseableError::BadHeader(_) => self.send_bad_headers(stream),
                ResponseableError::LengthRequired => self.send_length_required(stream),
                ResponseableError::PayloadToLarge => self.send_payload_to_large(stream),
                ResponseableError::IO => self.send_io_error(stream),
            },
            result => result,
        }
    }

    fn extract_length(
        &self,
        headers: &HashMap<String, String>,
    ) -> Result<usize, ResponseableError> {
        if !headers.contains_key("content-length") {
            return Err(ResponseableError::LengthRequired);
        }
        let len = headers["content-length"].as_str().parse::<usize>();
        match len {
            Err(_) => Err(ResponseableError::LengthRequired),
            Ok(len) => Ok(len),
        }
    }

    fn parse_body(
        &self,
        reader: &mut BufReader<TcpStream>,
        headers: &HashMap<String, String>,
    ) -> Result<Vec<u8>, HttpError> {
        let len = self.extract_length(headers)?;
        if len > self.buf_size {
            return Err(ResponseableError::PayloadToLarge)?;
        }
        let mut buf = vec![0 as u8; len];
        reader.read(&mut buf)?;
        Ok(buf)
    }

    fn handle_connection(&self, stream: &TcpStream) -> Result<(), HttpError> {
        let mut reader = BufReader::with_capacity(self.buf_size, stream.try_clone()?);
        let mut start = String::new();
        let len = reader.read_line(&mut start)?;
        if len == 0 {
            return Err(ResponseableError::NotHttpConform)?;
        }
        let parsed = ParsedFirstLine::parse(start)?;
        if !parsed.version.starts_with("HTTP/1.1") {
            return Err(ResponseableError::UnsupportedVersion(parsed.version))?;
        }

        let route = self
            .routes
            .find(&parsed.method, &parsed.path)
            .ok_or(ResponseableError::NotFound(parsed.path))?;

        let headers = parse_headers(&mut reader)?;

        let mut data = None;
        if parsed.method == HttpVerbs::PATCH
            || parsed.method == HttpVerbs::POST
            || parsed.method == HttpVerbs::PUT
        {
            data = Some(self.parse_body(&mut reader, &headers)?)
        }

        let resp = route.0(Request {
            params: route.1,
            query: parsed.query,
            headers,
            data,
        });

        match resp.body {
            BodyType::Fixed(body) => self.fixed_response(stream, resp.status, &body),
            BodyType::StreamWithTrailers(body) => self.stream_response(stream, resp.status, body),
            BodyType::Stream(body) => {
                self.stream_response(stream, resp.status, Box::new(NoTrailers::new(body)))
            }
        }
    }

    fn send_not_http_conform_request(&self, stream: TcpStream) -> Result<(), HttpError> {
        self.fixed_response(
            &stream,
            400,
            &"Not HTTP conform request\r\n".as_bytes().to_vec(),
        )
    }

    fn send_method_not_implemented(
        &self,
        stream: TcpStream,
        method: String,
    ) -> Result<(), HttpError> {
        self.fixed_response(
            &stream,
            501,
            &format!("Method {} not implemented\r\n", method)
                .as_bytes()
                .to_vec(),
        )
    }

    fn send_unsupported_version(
        &self,
        stream: TcpStream,
        version: String,
    ) -> Result<(), HttpError> {
        self.fixed_response(
            &stream,
            505,
            &format!("Verion {} not supported\r\n", version)
                .as_bytes()
                .to_vec(),
        )
    }

    fn send_io_error(&self, stream: TcpStream) -> Result<(), HttpError> {
        self.fixed_response(
            &stream,
            400,
            &"IO Error while reading\r\n".as_bytes().to_vec(),
        )
    }

    fn send_bad_headers(&self, stream: TcpStream) -> Result<(), HttpError> {
        self.fixed_response(&stream, 400, &"Invalid header data\r\n".as_bytes().to_vec())
    }

    fn send_length_required(&self, stream: TcpStream) -> Result<(), HttpError> {
        self.fixed_response(&stream, 411, &"Include length\r\n".as_bytes().to_vec())
    }

    fn send_payload_to_large(&self, stream: TcpStream) -> Result<(), HttpError> {
        self.fixed_response(&stream, 413, &"Payload to large\r\n".as_bytes().to_vec())
    }

    fn send_not_found(&self, stream: TcpStream, path: String) -> Result<(), HttpError> {
        self.fixed_response(
            &stream,
            404,
            &format!("Route {} does not exists\r\n", path)
                .as_bytes()
                .to_vec(),
        )
    }

    fn stream_response(
        &self,
        mut stream: &TcpStream,
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
        mut stream: &TcpStream,
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
