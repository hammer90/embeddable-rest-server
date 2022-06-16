mod common;
use common::{get_err, start_server};

#[test]
fn not_found() {
    let _server = start_server(vec![]);

    let response = get_err("http://localhost:8080/no_route");

    assert_eq!(response.status(), 404);
    assert_eq!(response.status_text(), "Not Found");
    assert_eq!(
        response.into_string().unwrap(),
        "Route /no_route does not exists\r\n"
    );
}
