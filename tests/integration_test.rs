mod common;
use common::{get, start_server};
use embeddable_rest_server::{BodyType, Response, Streamable};
use isahc::{ReadResponseExt, ResponseExt};

#[test]
fn not_found() {
    let (port, _server) = start_server(vec![]);

    let mut res = get(port, "/no_route");

    assert_eq!(res.status(), 404);
    assert_eq!(res.text().unwrap(), "Route /no_route does not exists\r\n");
}

#[test]
fn fixed() {
    let (port, _server) = start_server(vec![("/ok".to_string(), |_, _| -> Response {
        Response {
            status: 200,
            body: BodyType::Fixed("fixed\r\n".as_bytes().to_vec()),
        }
    })]);

    let mut res = get(port, "/ok");

    assert_eq!(res.status(), 200);
    assert_eq!(res.text().unwrap(), "fixed\r\n");
}

#[test]
fn from_string() {
    let (port, _server) = start_server(vec![("/simple".to_string(), |_, _| -> Response {
        Response::fixed_string(201, "simple\r\n")
    })]);

    let mut res = get(port, "/simple");

    assert_eq!(res.status(), 201);
    assert_eq!(res.text().unwrap(), "simple\r\n");
}

#[test]
fn chunked() {
    let (port, _server) = start_server(vec![("/chunked".to_string(), |_, _| -> Response {
        Response {
            status: 200,
            body: BodyType::Stream(Box::new(
                [
                    "Hello\r\n".as_bytes().to_vec(),
                    "World\r\n".as_bytes().to_vec(),
                ]
                .into_iter(),
            )),
        }
    })]);

    let mut res = get(port, "/chunked");

    assert_eq!(res.status(), 200);
    assert_eq!(res.headers()["transfer-encoding"], "chunked");
    assert_eq!(res.text().unwrap(), "Hello\r\nWorld\r\n");
}

#[test]
fn query() {
    let (port, _server) = start_server(vec![("/query".to_string(), |query, _| -> Response {
        assert_eq!(query.unwrap(), "count&foo=bar");
        Response::fixed_string(200, "queried\r\n")
    })]);

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
    let (port, _server) = start_server(vec![("/trailered".to_string(), |_, _| -> Response {
        Response {
            status: 200,
            body: BodyType::StreamWithTrailers(Box::new(WithTrailers::new())),
        }
    })]);

    let mut res = get(port, "/trailered");

    assert_eq!(res.status(), 200);
    assert_eq!(res.headers()["transfer-encoding"], "chunked");
    assert_eq!(res.headers()["trailers"], "foo");
    assert_eq!(res.text().unwrap(), "Hello\r\nTrailers\r\n");
    assert_eq!(res.trailer().try_get().unwrap()["foo"], "bar");
}
