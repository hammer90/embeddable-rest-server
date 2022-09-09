mod common;
use std::{collections::HashMap, sync::Arc};

use common::{get, get_header, post, start_server};
use embeddable_rest_server::{
    BodyType, FixedHandler, HandlerResult, HttpVerbs, RequestHandler, Response, SimpleHandler,
    Streamable,
};
use isahc::{http::header::CACHE_CONTROL, ReadResponseExt, ResponseExt};

use crate::common::put_chunked;

#[test]
fn not_found() {
    let (port, _server) = start_server(vec![], 1024, 42);

    let mut res = get(port, "/no_route");

    assert_eq!(res.status(), 404);
    assert_eq!(res.text().unwrap(), "Route /no_route does not exists\r\n");
}

#[test]
fn fixed() {
    let (port, _server) = start_server(
        vec![(HttpVerbs::GET, "/ok".to_string(), |req, context| {
            SimpleHandler::new(req, context, |_, _, _| Response {
                status: 200,
                body: BodyType::Fixed("fixed\r\n".as_bytes().to_vec()),
                headers: None,
            })
        })],
        1024,
        42,
    );

    let mut res = get(port, "/ok");

    assert_eq!(res.status(), 200);
    assert_eq!(res.text().unwrap(), "fixed\r\n");
}

#[test]
fn from_string() {
    let (port, _server) = start_server(
        vec![(HttpVerbs::GET, "/string".to_string(), |req, context| {
            SimpleHandler::new(req, context, |_, _, _| {
                Response::fixed_string(202, None, "string\r\n")
            })
        })],
        1024,
        42,
    );

    let mut res = get(port, "/string");

    assert_eq!(res.status(), 202);
    assert_eq!(res.text().unwrap(), "string\r\n");
}

#[test]
fn fixed_handler() {
    let (port, _server) = start_server(
        vec![(HttpVerbs::GET, "/fixed-handler".to_string(), |_, _| {
            FixedHandler::new(200, None, "fixed-handler\r\n")
        })],
        1024,
        42,
    );

    let mut res = get(port, "/fixed-handler");

    assert_eq!(res.status(), 200);
    assert_eq!(res.text().unwrap(), "fixed-handler\r\n");
}

#[test]
fn fixed_handler_with_headers() {
    let (port, _server) = start_server(
        vec![(
            HttpVerbs::GET,
            "/fixed-handler-with-headers".to_string(),
            |_, _| {
                FixedHandler::new(
                    200,
                    Some(HashMap::from([
                        ("Foo".to_string(), "bar".to_string()),
                        ("Cache-Control".to_string(), "no-cache".to_string()),
                    ])),
                    "fixed-handler-with-headers\r\n",
                )
            },
        )],
        1024,
        42,
    );

    let mut res = get(port, "/fixed-handler-with-headers");

    assert_eq!(res.status(), 200);
    assert_eq!(res.headers()["foo"], "bar");
    assert_eq!(res.headers()[CACHE_CONTROL], "no-cache");
    assert_eq!(res.text().unwrap(), "fixed-handler-with-headers\r\n");
}

#[test]
fn chunked() {
    let (port, _server) = start_server(
        vec![(HttpVerbs::GET, "/chunked".to_string(), |req, context| {
            SimpleHandler::new(req, context, |_, _, _| Response {
                status: 200,
                body: BodyType::Stream(Box::new(
                    [
                        "Hello\r\n".as_bytes().to_vec(),
                        "World\r\n".as_bytes().to_vec(),
                    ]
                    .into_iter(),
                )),
                headers: None,
            })
        })],
        1024,
        42,
    );

    let mut res = get(port, "/chunked");

    assert_eq!(res.status(), 200);
    assert_eq!(res.headers()["transfer-encoding"], "chunked");
    assert_eq!(res.text().unwrap(), "Hello\r\nWorld\r\n");
}

#[test]
fn chunked_with_headers() {
    let (port, _server) = start_server(
        vec![(
            HttpVerbs::GET,
            "/chunked-with-headers".to_string(),
            |req, context| {
                SimpleHandler::new(req, context, |_, _, _| Response {
                    status: 200,
                    body: BodyType::Stream(Box::new(
                        [
                            "Hello\r\n".as_bytes().to_vec(),
                            "Headers\r\n".as_bytes().to_vec(),
                        ]
                        .into_iter(),
                    )),
                    headers: Some(HashMap::from([
                        ("Foo".to_string(), "bar".to_string()),
                        ("Cache-Control".to_string(), "no-cache".to_string()),
                    ])),
                })
            },
        )],
        1024,
        42,
    );

    let mut res = get(port, "/chunked-with-headers");

    assert_eq!(res.status(), 200);
    assert_eq!(res.headers()["transfer-encoding"], "chunked");
    assert_eq!(res.headers()["foo"], "bar");
    assert_eq!(res.headers()[CACHE_CONTROL], "no-cache");
    assert_eq!(res.text().unwrap(), "Hello\r\nHeaders\r\n");
}

#[test]
fn query() {
    let (port, _server) = start_server(
        vec![(HttpVerbs::GET, "/query".to_string(), |req, context| {
            SimpleHandler::new(req, context, |req, _, _| {
                assert_eq!(req.query.as_ref().unwrap(), "count&foo=bar");
                Response::fixed_string(200, None, "queried\r\n")
            })
        })],
        1024,
        42,
    );

    let mut res = get(port, "/query?count&foo=bar");

    assert_eq!(res.status(), 200);
    assert_eq!(res.text().unwrap(), "queried\r\n");
}

struct WithTrailers {
    count: usize,
}

impl WithTrailers {
    fn new() -> Self {
        Self { count: 0 }
    }
}

impl Iterator for WithTrailers {
    type Item = Vec<u8>;

    fn next(&mut self) -> Option<Self::Item> {
        self.count += 1;
        match self.count {
            1 => Some("Hello\r\n".as_bytes().to_vec()),
            2 => Some("Trailers\r\n".as_bytes().to_vec()),
            _ => None,
        }
    }
}

impl Streamable for WithTrailers {
    fn trailer_names(&self) -> Vec<String> {
        vec!["foo".to_string()]
    }

    fn trailers(&self) -> Vec<(String, String)> {
        vec![("foo".to_string(), "bar".to_string())]
    }
}

#[test]
fn trailers() {
    let (port, _server) = start_server(
        vec![(HttpVerbs::GET, "/trailered".to_string(), |req, context| {
            SimpleHandler::new(req, context, |_, _, _| Response {
                status: 200,
                body: BodyType::StreamWithTrailers(Box::new(WithTrailers::new())),
                headers: None,
            })
        })],
        1024,
        42,
    );

    let mut res = get(port, "/trailered");

    assert_eq!(res.status(), 200);
    assert_eq!(res.headers()["transfer-encoding"], "chunked");
    assert_eq!(res.headers()["trailers"], "foo");
    assert_eq!(res.text().unwrap(), "Hello\r\nTrailers\r\n");
    assert_eq!(res.trailer().try_get().unwrap()["foo"], "bar");
}

#[test]
fn trailers_with_headers() {
    let (port, _server) = start_server(
        vec![(
            HttpVerbs::GET,
            "/trailered-with-headers".to_string(),
            |req, context| {
                SimpleHandler::new(req, context, |_, _, _| Response {
                    status: 200,
                    body: BodyType::StreamWithTrailers(Box::new(WithTrailers::new())),
                    headers: Some(HashMap::from([
                        ("Foo-Foo".to_string(), "bar-bar".to_string()),
                        ("Cache-Control".to_string(), "no-cache".to_string()),
                    ])),
                })
            },
        )],
        1024,
        42,
    );

    let mut res = get(port, "/trailered-with-headers");

    assert_eq!(res.status(), 200);
    assert_eq!(res.headers()["transfer-encoding"], "chunked");
    assert_eq!(res.headers()["trailers"], "foo");
    assert_eq!(res.headers()["foo-foo"], "bar-bar");
    assert_eq!(res.headers()[CACHE_CONTROL], "no-cache");
    assert_eq!(res.text().unwrap(), "Hello\r\nTrailers\r\n");
    assert_eq!(res.trailer().try_get().unwrap()["foo"], "bar");
}

#[test]
fn headers() {
    let (port, _server) = start_server(
        vec![(HttpVerbs::GET, "/headers".to_string(), |req, context| {
            SimpleHandler::new(req, context, |req, _, _| {
                assert_eq!(req.headers["foo"], "bar");
                Response::fixed_string(200, None, "heading\r\n")
            })
        })],
        1024,
        42,
    );

    let mut res = get_header(port, "/headers", "foo", "bar");

    assert_eq!(res.status(), 200);
    assert_eq!(res.text().unwrap(), "heading\r\n");
}

#[test]
fn body() {
    let (port, _server) = start_server(
        vec![(HttpVerbs::POST, "/body".to_string(), |req, context| {
            SimpleHandler::new(req, context, |_, _, data| {
                assert_eq!(std::str::from_utf8(data.as_ref()).unwrap(), "Hello Data");
                Response::fixed_string(200, None, "posted\r\n")
            })
        })],
        1024,
        42,
    );

    let mut res = post(port, "/body", "Hello Data");

    assert_eq!(res.status(), 200);
    assert_eq!(res.text().unwrap(), "posted\r\n");
}

#[test]
fn params() {
    let (port, _server) = start_server(
        vec![(
            HttpVerbs::GET,
            "/param/:foo/size".to_string(),
            |req, context| {
                SimpleHandler::new(req, context, |req, _, _| {
                    assert_eq!(req.params["foo"], "bar");
                    Response::fixed_string(
                        200,
                        None,
                        format!("size: {}\r\n", req.params["foo"].len()).as_str(),
                    )
                })
            },
        )],
        1024,
        42,
    );

    let mut res = get(port, "/param/bar/size");

    assert_eq!(res.status(), 200);
    assert_eq!(res.text().unwrap(), "size: 3\r\n");
}

struct ChunkedRequestHandler {}

impl RequestHandler for ChunkedRequestHandler {
    fn chunk(&mut self, chunk: Vec<u8>) -> HandlerResult {
        assert_eq!(std::str::from_utf8(chunk.as_ref()).unwrap(), "Hello Data");
        HandlerResult::Continue
    }

    fn end(&mut self) -> Response {
        Response::fixed_string(200, None, "chunked\r\n")
    }
}

#[test]
fn body_chunked() {
    let (port, _server) = start_server(
        vec![(HttpVerbs::PUT, "/chunks".to_string(), |_, _| {
            Box::new(ChunkedRequestHandler {})
        })],
        1024,
        42,
    );

    let mut res = put_chunked(port, "/chunks", "Hello Data");

    assert_eq!(res.text().unwrap(), "chunked\r\n");
    assert_eq!(res.status(), 200);
}

#[test]
fn body_chunked_collected() {
    let (port, _server) = start_server(
        vec![(HttpVerbs::PUT, "/collect".to_string(), |req, context| {
            SimpleHandler::new(req, context, |_, _, data| {
                assert_eq!(std::str::from_utf8(data.as_ref()).unwrap(), "Hello Data");
                Response::fixed_string(200, None, "collected\r\n")
            })
        })],
        1024,
        42,
    );

    let mut res = put_chunked(port, "/collect", "Hello Data");

    assert_eq!(res.status(), 200);
    assert_eq!(res.text().unwrap(), "collected\r\n");
}

struct SmallChunkRequestHandler {
    count: u32,
}

impl RequestHandler for SmallChunkRequestHandler {
    fn chunk(&mut self, chunk: Vec<u8>) -> HandlerResult {
        assert_eq!(std::str::from_utf8(chunk.as_ref()).unwrap(), "0123456789");
        self.count += 1;
        HandlerResult::Continue
    }

    fn end(&mut self) -> Response {
        Response::fixed_string(200, None, format!("{}\r\n", self.count).as_str())
    }
}

#[test]
fn small() {
    let (port, _server) = start_server(
        vec![(HttpVerbs::POST, "/small".to_string(), |_, _| {
            Box::new(SmallChunkRequestHandler { count: 0 })
        })],
        10,
        42,
    );

    let mut res = post(port, "/small", "012345678901234567890123456789");

    assert_eq!(res.text().unwrap(), "3\r\n");
    assert_eq!(res.status(), 200);
}

#[test]
fn small_chunked() {
    let (port, _server) = start_server(
        vec![(HttpVerbs::PUT, "/small-chunks".to_string(), |_, _| {
            Box::new(SmallChunkRequestHandler { count: 0 })
        })],
        10,
        42,
    );

    let mut res = put_chunked(port, "/small-chunks", "012345678901234567890123456789");

    assert_eq!(res.text().unwrap(), "3\r\n");
    assert_eq!(res.status(), 200);
}

struct Context {
    path: String,
}

#[test]
fn with_context() {
    let context = Context {
        path: "foo".to_string(),
    };

    let (port, _server) = start_server(
        vec![(
            HttpVerbs::GET,
            "/context".to_string(),
            |req, context: Arc<Context>| {
                SimpleHandler::new(req, context, |_, context, _| {
                    Response::fixed_string(200, None, &format!("path: '{}'\r\n", context.path))
                })
            },
        )],
        10,
        context,
    );

    let mut res = get(port, "/context");

    assert_eq!(res.status(), 200);
    assert_eq!(res.text().unwrap(), "path: 'foo'\r\n");
}
