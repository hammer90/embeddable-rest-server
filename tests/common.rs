use embeddable_rest_server::{HttpError, RestServer, RouteFn, SpawnedRestServer};
use ureq::Response;

pub fn start_server(routes: Vec<(String, RouteFn)>) -> SpawnedRestServer {
    let server = setup_server(routes).unwrap();
    SpawnedRestServer::spawn(server)
}

pub fn setup_server(routes: Vec<(String, RouteFn)>) -> Result<RestServer, HttpError> {
    let mut server = RestServer::new("0.0.0.0:8080")?;

    for (route, func) in routes {
        server = server.get(route.as_str(), func)?;
    }

    Ok(server)
}

pub fn get_err(addr: &str) -> Response {
    ureq::get(addr).call().unwrap_err().into_response().unwrap()
}
