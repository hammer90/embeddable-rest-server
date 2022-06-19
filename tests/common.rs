use embeddable_rest_server::{HttpError, HttpVerbs, RestServer, RouteFn, SpawnedRestServer};
use isahc::{Request, RequestExt, Response};

pub fn start_server(routes: Vec<(HttpVerbs, String, RouteFn)>) -> (u16, SpawnedRestServer) {
    let port = portpicker::pick_unused_port().unwrap();
    let server = setup_server(port, routes).unwrap();
    (port, SpawnedRestServer::spawn(server))
}

fn setup_server(
    port: u16,
    routes: Vec<(HttpVerbs, String, RouteFn)>,
) -> Result<RestServer, HttpError> {
    let mut server = RestServer::new(format!("0.0.0.0:{}", port), 1024)?;

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
