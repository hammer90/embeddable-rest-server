use embeddable_rest_server::{HttpError, RestServer, RouteFn, SpawnedRestServer};
use isahc::Response;

pub fn start_server(routes: Vec<(String, RouteFn)>) -> (u16, SpawnedRestServer) {
    let port = portpicker::pick_unused_port().unwrap();
    let server = setup_server(port, routes).unwrap();
    (port, SpawnedRestServer::spawn(server))
}

fn setup_server(port: u16, routes: Vec<(String, RouteFn)>) -> Result<RestServer, HttpError> {
    let mut server = RestServer::new(format!("0.0.0.0:{}", port))?;

    for (route, func) in routes {
        server = server.get(route.as_str(), func)?;
    }

    Ok(server)
}

pub fn get(port: u16, route: &str) -> Response<isahc::Body> {
    isahc::get(format!("http://localhost:{}{}", port, route).as_str()).unwrap()
}
