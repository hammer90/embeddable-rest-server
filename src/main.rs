mod server;

use server::{HttpError, Response, RestServer};

fn empty(_: Vec<u8>) -> Response {
    Response {
        status: 204,
        data: |_| -> Option<Vec<u8>> { None },
    }
}

fn bad(_: Vec<u8>) -> Response {
    Response {
        status: 400,
        data: |_| -> Option<Vec<u8>> { None },
    }
}

fn greeting(_: Vec<u8>) -> Response {
    Response {
        status: 200,
        data: |i| -> Option<Vec<u8>> {
            match i {
                0 => Some("Hello\r\n".into()),
                1 => Some("World\r\n".into()),
                2 => None,
                _ => None,
            }
        },
    }
}

fn main() -> Result<(), HttpError> {
    let server = RestServer::new("0.0.0.0:8080")?
        .get("/", empty)?
        .get("/bad", bad)?
        .get("/greeting", greeting)?;

    server.start()?;

    Ok(())
}
