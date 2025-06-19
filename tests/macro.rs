mod common;

use common::start_server;
use embeddable_rest_server::{collect_body, collect_body_limit, discard_body, Response, Route};
use isahc::ReadResponseExt;

use crate::common::put_chunked;

#[test]
fn collected_macro() {
    let (port, _server) = start_server(
        vec![(
            "/collect-macro".to_string(),
            Route::PUT(collect_body!(|_, _, data| {
                assert_eq!(std::str::from_utf8(data.as_ref()).unwrap(), "Hello Data");
                Response::fixed_string(200, None, "collected by macro\r\n")
            })),
        )],
        1024,
        42,
    );

    let mut res = put_chunked(port, "/collect-macro", "Hello Data");

    assert_eq!(res.status(), 200);
    assert_eq!(res.text().unwrap(), "collected by macro\r\n");
}

#[test]
fn collected_macro_limit() {
    let (port, _server) = start_server(
        vec![(
            "/collect-limit-macro".to_string(),
            Route::PUT(collect_body_limit!(10, |_, _, data| {
                assert_eq!(std::str::from_utf8(data.as_ref()).unwrap(), "Hello Data");
                Response::fixed_string(200, None, "collected by macro\r\n")
            })),
        )],
        1024,
        42,
    );

    let mut res = put_chunked(port, "/collect-limit-macro", "Hello Data");

    assert_eq!(res.status(), 200);
    assert_eq!(res.text().unwrap(), "collected by macro\r\n");

    let mut res = put_chunked(
        port,
        "/collect-limit-macro",
        "Hello Data, this will be to long",
    );

    assert_eq!(res.status(), 413);
    assert_eq!(res.text().unwrap(), "Max payload size 10 exceeded\r\n");
}

#[test]
fn discard_macro() {
    let (port, _server) = start_server(
        vec![(
            "/discard-macro".to_string(),
            Route::PUT(discard_body!(|_, _| {
                Response::fixed_string(200, None, "body has been discarded\r\n")
            })),
        )],
        1024,
        42,
    );

    let mut res = put_chunked(port, "/discard-macro", "To be discarded");

    assert_eq!(res.status(), 200);
    assert_eq!(res.text().unwrap(), "body has been discarded\r\n");
}
