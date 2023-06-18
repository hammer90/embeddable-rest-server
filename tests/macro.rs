mod common;

use common::start_server;
use embeddable_rest_server::{collect_body, Response, Route};
use isahc::ReadResponseExt;

use crate::common::put_chunked;

#[test]
fn body_chunked_collected_macro() {
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
