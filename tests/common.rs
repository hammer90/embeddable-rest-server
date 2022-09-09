use std::io::prelude::*;
use std::net::TcpStream;

use embeddable_rest_server::{HttpError, HttpVerbs, RestServer, RouteFn, SpawnedRestServer};
use isahc::{Body, Request, RequestExt, Response};

pub fn start_server<T: 'static + std::marker::Send + std::marker::Sync>(
    routes: Vec<(HttpVerbs, String, RouteFn<T>)>,
    buf_len: usize,
    context: T,
) -> (u16, SpawnedRestServer) {
    let port = portpicker::pick_unused_port().unwrap();
    let server = setup_server(port, routes, buf_len, context).unwrap();
    (port, SpawnedRestServer::spawn(server, 8192).unwrap())
}

fn setup_server<T>(
    port: u16,
    routes: Vec<(HttpVerbs, String, RouteFn<T>)>,
    buf_len: usize,
    context: T,
) -> Result<RestServer<T>, HttpError> {
    let mut server = RestServer::new(format!("0.0.0.0:{}", port), buf_len, context)?;

    for (verb, route, func) in routes {
        server = server.register(verb, route.as_str(), func)?;
    }

    Ok(server)
}

pub fn get(port: u16, route: &str) -> Response<isahc::Body> {
    isahc::get(format!("http://localhost:{}{}", port, route).as_str()).unwrap()
}

pub fn post(port: u16, route: &str, data: &str) -> Response<isahc::Body> {
    isahc::post(format!("http://localhost:{}{}", port, route).as_str(), data).unwrap()
}

pub fn put_chunked(port: u16, route: &str, data: &'static str) -> Response<isahc::Body> {
    let body = Body::from_reader(data.as_bytes());
    isahc::put(format!("http://localhost:{}{}", port, route).as_str(), body).unwrap()
}

pub fn send_raw(port: u16, data: &str) -> String {
    let mut stream = TcpStream::connect(format!("localhost:{}", port).as_str()).unwrap();

    stream.write_all(data.as_bytes()).unwrap();
    let mut buf = vec![];
    stream.read_to_end(&mut buf).unwrap();
    let nul_range_end = buf.iter().position(|&c| c == b'\0').unwrap_or(buf.len());
    std::str::from_utf8(&buf[0..nul_range_end])
        .unwrap()
        .to_string()
}

pub fn get_header(
    port: u16,
    route: &str,
    header_name: &str,
    header_value: &str,
) -> Response<isahc::Body> {
    Request::get(format!("http://localhost:{}{}", port, route).as_str())
        .header(header_name, header_value)
        .body(())
        .unwrap()
        .send()
        .unwrap()
}
