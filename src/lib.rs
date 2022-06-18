use std::collections::HashMap;
use std::io::{prelude::*, BufReader, Error as IoError};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[derive(Debug)]
pub enum ResponseableError {
    NotHttpConform,
    UnsupportedVersion(String),
    NotFound(String),
    BadHeader(String),
    MethodNotImplemented(String),
    LengthRequired,
    PayloadToLarge,
    IO(IoError),
}

#[derive(Debug)]
pub enum HttpError {
    RouteExists,
    IO(IoError),
    Responseable(ResponseableError),
}

impl From<IoError> for ResponseableError {
    fn from(err: IoError) -> ResponseableError {
        ResponseableError::IO(err)
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

struct ParsedFirstLine {
    method: HttpVerbs,
    path: String,
    query: Option<String>,
    version: String,
}

fn map_method(method: &str) -> Result<HttpVerbs, ResponseableError> {
    match method {
        "GET" => Ok(HttpVerbs::GET),
        "POST" => Ok(HttpVerbs::POST),
        "PUT" => Ok(HttpVerbs::PUT),
        "DELETE" => Ok(HttpVerbs::DELETE),
        "PATCH" => Ok(HttpVerbs::PATCH),
        _ => Err(ResponseableError::MethodNotImplemented(method.to_string())),
    }
}

impl ParsedFirstLine {
    fn parse(line: String) -> Result<Self, ResponseableError> {
        let splitted: Vec<&str> = line.split(' ').collect();
        if splitted.len() != 3 {
            return Err(ResponseableError::NotHttpConform);
        }
        let path_query = splitted[1].split_once('?');
        let method = map_method(splitted[0])?;
        if let Some((path, query)) = path_query {
            Ok(Self {
                method,
                path: path.to_string(),
                query: Some(query.to_string()),
                version: splitted[2].to_string(),
            })
        } else {
            Ok(Self {
                method,
                path: splitted[1].to_string(),
                query: None,
                version: splitted[2].to_string(),
            })
        }
    }
}

fn parse_headers(
    reader: &mut BufReader<TcpStream>,
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

    pub fn register(
        mut self,
        verb: HttpVerbs,
        route: &str,
        func: RouteFn,
    ) -> Result<Self, HttpError> {
        let route = format!("{:?} {}", verb, route);
        if let Some(_) = self.routes.get(route.as_str()) {
            return Err(HttpError::RouteExists);
        }
        self.routes.insert(route, func);
        Ok(self)
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
                ResponseableError::IO(_) => self.send_io_error(stream),
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
        if len > 1024 {
            return Err(ResponseableError::PayloadToLarge)?;
        }
        let mut buf = vec![0 as u8; len];
        reader.read(&mut buf)?;
        Ok(buf)
    }

    fn handle_connection(&self, stream: &TcpStream) -> Result<(), HttpError> {
        let mut reader = BufReader::new(stream.try_clone()?);
        let mut start = String::new();
        let len = reader.read_line(&mut start)?;
        if len == 0 {
            return Err(ResponseableError::NotHttpConform)?;
        }
        let parsed = ParsedFirstLine::parse(start)?;
        if !parsed.version.starts_with("HTTP/1.1") {
            return Err(ResponseableError::UnsupportedVersion(parsed.version))?;
        }

        let route_key = format!("{:?} {}", parsed.method, parsed.path);
        let route = self
            .routes
            .get(&route_key)
            .ok_or(ResponseableError::NotFound(parsed.path))?;

        let headers = parse_headers(&mut reader)?;

        let mut data = None;
        if parsed.method == HttpVerbs::PATCH
            || parsed.method == HttpVerbs::POST
            || parsed.method == HttpVerbs::PUT
        {
            data = Some(self.parse_body(&mut reader, &headers)?)
        }

        let resp = route(Request {
            params: None,
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
