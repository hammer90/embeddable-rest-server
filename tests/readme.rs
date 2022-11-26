use std::{collections::HashMap, time::Duration};

use embeddable_rest_server::{
    CollectingHandler, HttpError, Response, RestServer, SpawnedRestServer,
};
use isahc::ReadResponseExt;

struct Context {
    greeting: String,
}

#[test]
fn readme() -> Result<(), HttpError> {
    let port = portpicker::pick_unused_port().unwrap();
    let context = Context {
        greeting: "Hello".to_string(),
    };

    // create the server
    let mut server = RestServer::new(
        "0.0.0.0".to_string(),
        port,
        2048,
        context,
        Some(Duration::from_secs(2)),
    )?;

    // register routes without body
    server = server.get("/info", |_, _| {
        Response::fixed_string(200, None, "Hello World")
    })?;
    // register routes with only small bodies
    server = server.post("/greeting/:name", |req, context| {
        CollectingHandler::new(req, context, |req, context, body| {
            Response::fixed_string(
                200,
                Some(HashMap::from([("Foo".to_string(), "bar".to_string())])),
                &format!(
                    "{} {}, thanks for {} bytes and {} headers",
                    context.greeting,
                    req.params["name"],
                    body.len(),
                    req.headers.len()
                ),
            )
        })
    })?;

    // start the server blocking
    // server.start()?;
    // or start server in a new thread
    let spawned_server = SpawnedRestServer::spawn(server, 8192)?;

    // adding new routes is not possible after the server is started

    let mut res = isahc::get(format!("http://localhost:{}/info", port).as_str()).unwrap();
    assert_eq!(res.text().unwrap(), "Hello World");

    let mut res = isahc::post(
        format!("http://localhost:{}/greeting/Bob", port).as_str(),
        "123456789",
    )
    .unwrap();

    assert_eq!(res.headers()["foo"], "bar");
    assert_eq!(
        res.text().unwrap(),
        "Hello Bob, thanks for 9 bytes and 6 headers"
    );

    spawned_server.stop();
    Ok(())
}
