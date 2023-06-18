mod headers;
mod parsed_first_line;
mod routes;
mod status_text;

use std::collections::HashMap;
use std::error::Error as StdError;
use std::io::{prelude::*, BufReader, Error as IoError};
use std::net::{TcpListener, TcpStream};
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
    InvalidLength,
    PayloadToLarge,
    BrokenChunk,
    IO,
}

#[derive(Debug)]
pub enum HttpError {
    RouteExists,
    IO(IoError),
    Responseable(ResponseableError),
    Std,
}

impl From<IoError> for ResponseableError {
    fn from(_: IoError) -> ResponseableError {
        ResponseableError::IO
    }
}

impl From<Box<dyn StdError>> for HttpError {
    fn from(_: Box<dyn StdError>) -> HttpError {
        HttpError::Std
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
    pub headers: Option<HashMap<String, String>>,
}

impl Response {
    pub fn fixed_string(status: u32, headers: Option<HashMap<String, String>>, body: &str) -> Self {
        Response {
            status,
            body: BodyType::Fixed(body.as_bytes().to_vec()),
            headers,
        }
    }
}

pub struct Request {
    pub params: HashMap<String, String>,
    pub query: Option<String>,
    pub headers: HashMap<String, String>,
}

pub enum HandlerResult {
    Abort(Response),
    Continue,
}

pub trait RequestHandler {
    fn chunk(&mut self, chunk: Vec<u8>) -> HandlerResult;
    fn end(&mut self, trailers: Option<HashMap<String, String>>) -> Response;
}

pub struct CancelHandler {
    status: u32,
    body: String,
    headers: Option<HashMap<String, String>>,
}

impl CancelHandler {
    pub fn new(status: u32, headers: Option<HashMap<String, String>>, body: &str) -> Box<Self> {
        Box::new(Self {
            status,
            body: body.to_string(),
            headers,
        })
    }
}

impl RequestHandler for CancelHandler {
    fn chunk(&mut self, _chunk: Vec<u8>) -> HandlerResult {
        HandlerResult::Abort(Response::fixed_string(
            self.status,
            self.headers.to_owned(),
            self.body.as_str(),
        ))
    }

    fn end(&mut self, _: Option<HashMap<String, String>>) -> Response {
        Response::fixed_string(self.status, self.headers.to_owned(), self.body.as_str())
    }
}

pub type CollectedRoute<T> = fn(req: &Request, context: &T, data: &[u8]) -> Response;

pub struct CollectingHandler<T> {
    route: CollectedRoute<T>,
    req: Request,
    data: Vec<u8>,
    context: T,
}

impl<T> CollectingHandler<T> {
    pub fn new(req: Request, context: T, route: CollectedRoute<T>) -> Box<Self> {
        Box::new(Self {
            route,
            req,
            data: vec![],
            context,
        })
    }
}

#[macro_export]
macro_rules! collect_body {
    ($route:expr) => {
        |req, context| $crate::CollectingHandler::new(req, context, $route);
    };
}

impl<T> RequestHandler for CollectingHandler<T> {
    fn chunk(&mut self, mut chunk: Vec<u8>) -> HandlerResult {
        self.data.append(&mut chunk);
        HandlerResult::Continue
    }

    fn end(&mut self, _: Option<HashMap<String, String>>) -> Response {
        (self.route)(&self.req, &self.context, &self.data)
    }
}

pub type RouteFn<T> = fn(req: Request, context: Arc<T>) -> Box<dyn RequestHandler>;
pub type RouteFnWithoutData<T> = fn(req: Request, context: Arc<T>) -> Response;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Route<T> {
    GET(RouteFnWithoutData<T>),
    POST(RouteFn<T>),
    PUT(RouteFn<T>),
    DELETE(RouteFnWithoutData<T>),
    PATCH(RouteFn<T>),
}

enum RouteWithoutVerb<T> {
    NoDate(RouteFnWithoutData<T>),
    WithData(RouteFn<T>),
}

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

struct HttpRoutes<T> {
    get: Routes<RouteFnWithoutData<T>>,
    post: Routes<RouteFn<T>>,
    put: Routes<RouteFn<T>>,
    patch: Routes<RouteFn<T>>,
    delete: Routes<RouteFnWithoutData<T>>,
}

impl<T> HttpRoutes<T> {
    fn new() -> Self {
        Self {
            get: Routes::<RouteFnWithoutData<T>>::new(),
            post: Routes::<RouteFn<T>>::new(),
            put: Routes::<RouteFn<T>>::new(),
            patch: Routes::<RouteFn<T>>::new(),
            delete: Routes::<RouteFnWithoutData<T>>::new(),
        }
    }

    fn add(self, route: &str, func: Route<T>) -> Result<Self, RoutesError> {
        match func {
            Route::GET(func) => Ok(Self {
                get: self.get.add(route, func)?,
                ..self
            }),
            Route::POST(func) => Ok(Self {
                post: self.post.add(route, func)?,
                ..self
            }),
            Route::PUT(func) => Ok(Self {
                put: self.put.add(route, func)?,
                ..self
            }),
            Route::PATCH(func) => Ok(Self {
                patch: self.patch.add(route, func)?,
                ..self
            }),
            Route::DELETE(func) => Ok(Self {
                delete: self.delete.add(route, func)?,
                ..self
            }),
        }
    }

    fn find(
        &self,
        verb: &HttpVerbs,
        route: &str,
    ) -> Option<(RouteWithoutVerb<T>, HashMap<String, String>)> {
        match verb {
            HttpVerbs::GET => self
                .get
                .find(route)
                .map(|r| (RouteWithoutVerb::NoDate(r.0), r.1)),
            HttpVerbs::POST => self
                .post
                .find(route)
                .map(|r| (RouteWithoutVerb::WithData(r.0), r.1)),
            HttpVerbs::PUT => self
                .put
                .find(route)
                .map(|r| (RouteWithoutVerb::WithData(r.0), r.1)),
            HttpVerbs::PATCH => self
                .patch
                .find(route)
                .map(|r| (RouteWithoutVerb::WithData(r.0), r.1)),
            HttpVerbs::DELETE => self
                .delete
                .find(route)
                .map(|r| (RouteWithoutVerb::NoDate(r.0), r.1)),
        }
    }
}

enum ContentLength {
    Fixed(usize),
    Chunked,
    None,
}

pub struct RestServer<T> {
    listener: TcpListener,
    routes: HttpRoutes<T>,
    shutdown: Arc<Mutex<bool>>,
    buf_size: usize,
    context: Arc<T>,
    addr: String,
    port: u16,
    read_timeout: Option<Duration>,
}

impl<T> RestServer<T> {
    pub fn new(
        addr: String,
        port: u16,
        buf_size: usize,
        context: T,
        read_timeout: Option<Duration>,
    ) -> Result<Self, HttpError> {
        let listener = TcpListener::bind(format!("{}:{}", addr, port))?;
        let shutdown = Arc::new(Mutex::new(false));
        Ok(Self {
            listener,
            routes: HttpRoutes::new(),
            shutdown,
            buf_size,
            context: Arc::new(context),
            addr,
            port,
            read_timeout,
        })
    }

    pub fn port(&self) -> Result<u16, IoError> {
        self.listener
            .local_addr()
            .map(|local_addr| local_addr.port())
    }

    pub fn start(self) -> Result<(), HttpError> {
        let stop = self.shutdown.clone();
        for stream in self.listener.incoming() {
            if *stop.lock().unwrap() {
                println!("shutting down");
                break;
            }
            if let Ok(stream) = stream {
                let result = self.handle_connection_witherrors(stream);
                if let Err(err) = result {
                    println!("{:?}", err);
                }
            }
        }
        Ok(())
    }

    pub fn register(self, route: &str, func: Route<T>) -> Result<Self, HttpError> {
        Ok(Self {
            routes: self.routes.add(route, func)?,
            ..self
        })
    }

    pub fn get(self, route: &str, func: RouteFnWithoutData<T>) -> Result<Self, HttpError> {
        self.register(route, Route::GET(func))
    }

    pub fn post(self, route: &str, func: RouteFn<T>) -> Result<Self, HttpError> {
        self.register(route, Route::POST(func))
    }

    pub fn put(self, route: &str, func: RouteFn<T>) -> Result<Self, HttpError> {
        self.register(route, Route::PUT(func))
    }

    pub fn delete(self, route: &str, func: RouteFnWithoutData<T>) -> Result<Self, HttpError> {
        self.register(route, Route::DELETE(func))
    }

    pub fn patch(self, route: &str, func: RouteFn<T>) -> Result<Self, HttpError> {
        self.register(route, Route::PATCH(func))
    }

    fn handle_connection_witherrors(&self, stream: TcpStream) -> Result<(), HttpError> {
        let result = self.handle_connection(&stream);
        match result {
            Err(HttpError::Responseable(responseable)) => match responseable {
                ResponseableError::NotHttpConform => send_not_http_conform_request(stream),
                ResponseableError::UnsupportedVersion(version) => {
                    send_unsupported_version(stream, version)
                }
                ResponseableError::MethodNotImplemented(method) => {
                    send_method_not_implemented(stream, method)
                }
                ResponseableError::NotFound(path) => send_not_found(stream, path),
                ResponseableError::BadHeader(_) => send_bad_headers(stream),
                ResponseableError::InvalidLength => send_invalid_length(stream),
                ResponseableError::PayloadToLarge => send_payload_to_large(stream),
                ResponseableError::BrokenChunk => send_broken_chunk(stream),
                ResponseableError::IO => send_io_error(stream),
            },
            result => result,
        }
    }

    fn extract_length(
        &self,
        headers: &HashMap<String, String>,
    ) -> Result<ContentLength, ResponseableError> {
        if !headers.contains_key("content-length") {
            if headers.contains_key("transfer-encoding")
                && headers["transfer-encoding"] == "chunked"
            {
                return Ok(ContentLength::Chunked);
            } else {
                return Ok(ContentLength::None);
            }
        }
        headers["content-length"]
            .as_str()
            .parse::<usize>()
            .map(ContentLength::Fixed)
            .map_err(|_| ResponseableError::InvalidLength)
    }

    fn handle_connection(&self, stream: &TcpStream) -> Result<(), HttpError> {
        if let Some(timeout) = self.read_timeout {
            stream.set_read_timeout(Some(timeout))?;
        }
        let mut reader = BufReader::with_capacity(self.buf_size, stream);
        let mut start = String::new();
        let len = reader.read_line(&mut start)?;
        if len == 0 {
            return Err(ResponseableError::NotHttpConform.into());
        }
        let parsed = ParsedFirstLine::parse(start)?;
        if !parsed.version.starts_with("HTTP/1.1") {
            return Err(ResponseableError::UnsupportedVersion(parsed.version).into());
        }

        let route = self
            .routes
            .find(&parsed.method, &parsed.path)
            .ok_or(ResponseableError::NotFound(parsed.path))?;

        let headers = parse_headers(&mut reader)?;
        let len = self.extract_length(&headers)?;
        let trailers = headers.get("trailers").map(|x| x.to_owned());

        let resp = match route.0 {
            RouteWithoutVerb::NoDate(func) => func(
                Request {
                    params: route.1,
                    query: parsed.query,
                    headers,
                },
                self.context.clone(),
            ),
            RouteWithoutVerb::WithData(func) => {
                let handler = func(
                    Request {
                        params: route.1,
                        query: parsed.query,
                        headers,
                    },
                    self.context.clone(),
                );
                match len {
                    ContentLength::Fixed(len) => {
                        self.handle_fixed_request(len, handler, &mut reader)?
                    }
                    ContentLength::Chunked => {
                        self.handle_chunked_request(handler, trailers, &mut reader)?
                    }
                    ContentLength::None => {
                        Response::fixed_string(411, None, "Include length or send chunked")
                    }
                }
            }
        };

        match resp.body {
            BodyType::Fixed(body) => fixed_response(stream, resp.status, resp.headers, &body),
            BodyType::StreamWithTrailers(body) => {
                stream_response(stream, resp.status, resp.headers, body)
            }
            BodyType::Stream(body) => stream_response(
                stream,
                resp.status,
                resp.headers,
                Box::new(NoTrailers::new(body)),
            ),
        }
    }

    fn read_in_chunks(
        &self,
        len: usize,
        handler: &mut Box<dyn RequestHandler>,
        reader: &mut BufReader<&TcpStream>,
    ) -> Result<HandlerResult, HttpError> {
        let mut count = 0;
        while count < len {
            let buf_size = min(len - count, self.buf_size);
            let mut buf = vec![0_u8; buf_size];
            reader.read_exact(&mut buf)?;
            if let HandlerResult::Abort(res) = handler.chunk(buf) {
                return Ok(HandlerResult::Abort(res));
            }
            count += buf_size;
        }
        Ok(HandlerResult::Continue)
    }

    fn handle_fixed_request(
        &self,
        len: usize,
        mut handler: Box<dyn RequestHandler>,
        reader: &mut BufReader<&TcpStream>,
    ) -> Result<Response, HttpError> {
        if let HandlerResult::Abort(res) = self.read_in_chunks(len, &mut handler, reader)? {
            return Ok(res);
        }
        Ok(handler.end(None))
    }

    fn read_chunk_length(
        &self,
        reader: &mut BufReader<&TcpStream>,
    ) -> Result<usize, ResponseableError> {
        let mut len = String::new();
        let count = reader.read_line(&mut len)?;
        if count == 0 || len == "\r\n" {
            return Err(ResponseableError::BrokenChunk);
        }
        usize::from_str_radix(&len[..count - 2], 16).map_err(|e| {
            println!("{}", e);
            ResponseableError::BrokenChunk
        })
    }

    fn handle_chunked_request(
        &self,
        mut handler: Box<dyn RequestHandler>,
        trailers: Option<String>,
        reader: &mut BufReader<&TcpStream>,
    ) -> Result<Response, HttpError> {
        loop {
            let len = self.read_chunk_length(reader)?;
            if len == 0 {
                let mut extracted_trailers = None;
                if let Some(trailers) = trailers {
                    let mut parse_trailers = parse_headers(reader)?;
                    let mut allowed_trailers = HashMap::with_capacity(parse_trailers.len());
                    for expected_trailer in trailers.split(',') {
                        let lower_case_trailer = expected_trailer.to_lowercase();
                        if let Some(parsed_trailer) = parse_trailers.remove(&lower_case_trailer) {
                            allowed_trailers.insert(lower_case_trailer, parsed_trailer);
                        }
                    }
                    extracted_trailers = Some(allowed_trailers);
                }
                return Ok(handler.end(extracted_trailers));
            }
            if let HandlerResult::Abort(res) = self.read_in_chunks(len, &mut handler, reader)? {
                return Ok(res);
            }
            let mut nl = [0_u8, 2];
            reader.read_exact(&mut nl)?;
            if nl != [13, 10] {
                return Err(ResponseableError::BrokenChunk.into());
            }
        }
    }
}

fn send_not_http_conform_request(stream: TcpStream) -> Result<(), HttpError> {
    fixed_response(
        &stream,
        400,
        None,
        "Not HTTP conform request\r\n".as_bytes(),
    )
}

fn send_method_not_implemented(stream: TcpStream, method: String) -> Result<(), HttpError> {
    fixed_response(
        &stream,
        501,
        None,
        format!("Method {} not implemented\r\n", method).as_bytes(),
    )
}

fn send_unsupported_version(stream: TcpStream, version: String) -> Result<(), HttpError> {
    fixed_response(
        &stream,
        505,
        None,
        format!("Version {} not supported\r\n", version).as_bytes(),
    )
}

fn send_io_error(stream: TcpStream) -> Result<(), HttpError> {
    fixed_response(&stream, 400, None, "IO Error while reading\r\n".as_bytes())
}

fn send_bad_headers(stream: TcpStream) -> Result<(), HttpError> {
    fixed_response(&stream, 400, None, "Invalid header data\r\n".as_bytes())
}

fn send_invalid_length(stream: TcpStream) -> Result<(), HttpError> {
    fixed_response(&stream, 411, None, "Length invalid\r\n".as_bytes())
}

fn send_payload_to_large(stream: TcpStream) -> Result<(), HttpError> {
    fixed_response(&stream, 413, None, "Payload to large\r\n".as_bytes())
}

fn send_not_found(stream: TcpStream, path: String) -> Result<(), HttpError> {
    fixed_response(
        &stream,
        404,
        None,
        format!("Route {} does not exists\r\n", path).as_bytes(),
    )
}

fn send_broken_chunk(stream: TcpStream) -> Result<(), HttpError> {
    fixed_response(&stream, 400, None, "Invalid chunk encoding\r\n".as_bytes())
}

fn stream_response(
    mut stream: &TcpStream,
    status: u32,
    headers: Option<HashMap<String, String>>,
    mut body: Box<dyn Streamable>,
) -> Result<(), HttpError> {
    let start = format!(
        "HTTP/1.1 {} {}\r\nConnection: Close\r\nTransfer-Encoding: chunked\r\n",
        status,
        status_text(status),
    );
    stream.write_all(start.as_bytes())?;

    if let Some(headers) = headers {
        for (key, value) in headers {
            stream.write_all(format!("{}: {}\r\n", key, value).as_bytes())?;
        }
    }

    let trailer_names = body.trailer_names();
    let has_trailers = !trailer_names.is_empty();
    if has_trailers {
        stream.write_all(format!("Trailers: {}\r\n", trailer_names.join(",")).as_bytes())?;
    }
    stream.write_all("\r\n".as_bytes())?;
    stream.flush()?;

    for data in body.by_ref() {
        let chunk_head = format!("{:x}\r\n", data.len());
        stream.write_all(chunk_head.as_bytes())?;
        stream.write_all(&data)?;
        stream.write_all("\r\n".as_bytes())?;
        stream.flush()?;
    }

    stream.write_all("0\r\n".as_bytes())?;
    if has_trailers {
        let trailers = body.trailers();
        for trailer in trailers {
            stream.write_all(format!("{}: {}\r\n", trailer.0, trailer.1).as_bytes())?;
        }
    }
    stream.write_all("\r\n".as_bytes())?;
    stream.flush()?;

    Ok(())
}

fn fixed_response(
    mut stream: &TcpStream,
    status: u32,
    headers: Option<HashMap<String, String>>,
    body: &[u8],
) -> Result<(), HttpError> {
    let start = format!(
        "HTTP/1.1 {} {}\r\nConnection: Close\r\nContent-Length: {}\r\n",
        status,
        status_text(status),
        body.len()
    );
    stream.write_all(start.as_bytes())?;
    if let Some(headers) = headers {
        for (key, value) in headers {
            stream.write_all(format!("{}: {}\r\n", key, value).as_bytes())?;
        }
    }
    stream.write_all("\r\n".as_bytes())?;
    stream.write_all(body)?;
    stream.flush()?;

    Ok(())
}

pub struct SpawnedRestServer {
    _handle: JoinHandle<Result<(), HttpError>>,
    stop: Arc<Mutex<bool>>,
    addr: String,
    port: u16,
}

impl SpawnedRestServer {
    pub fn spawn<T: 'static + std::marker::Send + std::marker::Sync>(
        server: RestServer<T>,
        stack_size: usize,
    ) -> Result<Self, HttpError> {
        let stop = server.shutdown.clone();
        let builder = thread::Builder::new().stack_size(stack_size);
        let addr = server.addr.to_owned();
        let port = server.port;
        let handle = builder.spawn(move || server.start())?;
        Ok(SpawnedRestServer {
            _handle: handle,
            stop,
            addr,
            port,
        })
    }

    pub fn stop(&self) {
        let mut shutdown_lock = self.stop.lock().unwrap();
        *shutdown_lock = true;
    }

    pub fn is_stopped(&self) -> bool {
        let shutdown_lock = self.stop.lock().unwrap();
        *shutdown_lock
    }
}

impl Drop for SpawnedRestServer {
    fn drop(&mut self) {
        self.stop();
        let _ = TcpStream::connect(format!("{}:{}", self.addr, self.port).as_str());
    }
}

fn min(a: usize, b: usize) -> usize {
    if a < b {
        a
    } else {
        b
    }
}
