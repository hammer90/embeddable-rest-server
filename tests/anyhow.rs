use std::sync::Arc;

use anyhow::Result;
use common::get;
use embeddable_rest_server::{Request, Response, RestServer, Route, SpawnedRestServer};
use isahc::ReadResponseExt;

mod common;

fn build_info() -> Result<Response, Response> {
    Ok(Response::fixed_string(200, None, "info\r\n"))
}

fn handle_info<T>(_: Request, _: Arc<T>) -> Response {
    match build_info() {
        Ok(resp) => resp,
        Err(resp) => resp,
    }
}

fn setup_anyhow_server<T: Send + Sync + 'static>(
    routes: Vec<(String, Route<T>)>,
    context: T,
) -> Result<(u16, SpawnedRestServer)> {
    let server = RestServer::new("0.0.0.0".to_string(), 0, 1024, context, None)?;

    let mut server = server.get("info", handle_info)?;
    for (route, func) in routes {
        server = server.register(route.as_str(), func)?;
    }

    let port = server.port()?;
    Ok((port, SpawnedRestServer::spawn(server, 8192)?))
}

#[test]
fn types_work() -> Result<()> {
    let (port, _server) = setup_anyhow_server(vec![], 42)?;

    let mut res = get(port, "/info");

    assert_eq!(res.status(), 200);
    assert_eq!(res.text().unwrap(), "info\r\n");
    Ok(())
}

#[test]
#[should_panic(expected = "called `Result::unwrap()` on an `Err` value: RouteExists")]
fn error_can_be_printed() {
    setup_anyhow_server(vec![("info".to_owned(), Route::GET(handle_info))], 42).unwrap();
}
