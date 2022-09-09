# embeddable-rest-server

`embeddable-rest-server` is a lightweight HTTP server which is intended to work on embedded hardware with low computing power.
The primary focus is a stateless RESTful server, thus HTTP/2 features like Stateful-Headers are explicitly excluded.

## Features

* size limit for internal buffers (do not confuse this with the buffers of the TCP/IP stack)
* parameterized routes: `/files/:name/size`
* chunked transfers
    * incoming request are additionally split to met the configured size limit
    * sending of HTTP trailers (note that many HTTP clients ignore them)
* as of commit `7e221b93e` only dev-dependencies are needed (even if this was never a goal)

## Missing but planned Features

* access to HTTP trailers for incoming request
* HTTPS support
* handling of parallel request, most properly through:
* async support

## Installation


`embeddable-rest-server` is neither HTTP/1.1 feature complete nor API stable, you must accept 501/505 responses for certain requests and breaking change.

Install via Cargo by adding to your `Cargo.toml` file:

```toml
[dependencies]
embeddable_rest_server = { git = "https://github.com/hammer90/embeddable-rest-server" }
```

## Usage

```rust
    let context = Context {
        greeting: "Hello".to_string(),
    };

    // create the server
    let mut server = RestServer::new(format!("0.0.0.0:{}", port), 2048, context)?;

    // register routes (for requests without or with only small bodies)
    server = server.post("/greeting/:name", |req, context| {
        SimpleHandler::new(req, context, |req, context, body| {
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
    
    let mut res = isahc::post(format!("http://localhost:{}/greeting/Bob", port).as_str(),"123456789").unwrap();

    assert_eq!(res.headers()["foo"], "bar");
    assert_eq!(res.text().unwrap(), "Hello Bob, thanks for 9 bytes and 6 headers");

    spawned_server.stop();

```
