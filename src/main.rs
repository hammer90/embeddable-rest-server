mod lib;

use std::{thread, time::Duration};

use lib::{BodyType, HttpError, Request, Response, RestServer, Streamable};

fn empty(_: Request) -> Response {
    Response {
        status: 204,
        body: BodyType::Fixed(vec![]),
    }
}

fn bad(_: Request) -> Response {
    Response::fixed_string(400, "This was bad\r\n")
}

fn greeting(_: Request) -> Response {
    Response {
        status: 200,
        body: BodyType::Stream(Box::new(
            [
                "Hello\r\n".as_bytes().to_vec(),
                "World\r\n".as_bytes().to_vec(),
            ]
            .into_iter(),
        )),
    }
}

struct SlowResponse {
    count: usize,
    max: usize,
}

impl SlowResponse {
    fn new(max: usize) -> SlowResponse {
        SlowResponse { count: 0, max }
    }
}

impl Iterator for SlowResponse {
    type Item = Vec<u8>;
    fn next(&mut self) -> Option<<Self as Iterator>::Item> {
        if self.count >= self.max {
            return None;
        }
        self.count += 1;
        let msg = format!("Call number {}\r\n", self.count);
        thread::sleep(Duration::from_secs(1));
        Some(msg.as_bytes().to_vec())
    }
}

fn slow(req: Request) -> Response {
    let query = req.query.unwrap_or("10".to_string());
    let count = query.parse::<usize>();
    if let Err(_) = count {
        return Response::fixed_string(400, "Query should be a number\r\n");
    }
    Response {
        status: 200,
        body: BodyType::Stream(Box::new(SlowResponse::new(count.unwrap_or(10)))),
    }
}

struct WithTrailers {
    count: usize,
    msg: String,
}

impl WithTrailers {
    fn new(msg: &str) -> Self {
        Self {
            count: 0,
            msg: msg.to_string(),
        }
    }
}

impl Iterator for WithTrailers {
    type Item = Vec<u8>;

    fn next(&mut self) -> Option<Self::Item> {
        self.count += 1;
        match self.count {
            1 => Some("Hello\r\n".as_bytes().to_vec()),
            2 => Some("Trailers\r\n".as_bytes().to_vec()),
            _ => None,
        }
    }
}

impl Streamable for WithTrailers {
    fn trailer_names(&self) -> Vec<String> {
        vec!["foo".to_string()]
    }

    fn trailers(&self) -> Vec<(String, String)> {
        vec![("foo".to_string(), self.msg.to_string())]
    }
}

fn trailered(_: Request) -> Response {
    Response {
        status: 200,
        body: BodyType::StreamWithTrailers(Box::new(WithTrailers::new("bar"))),
    }
}

fn main() -> Result<(), HttpError> {
    let server = RestServer::new("0.0.0.0:8080")?
        .get("/", empty)?
        .get("/bad", bad)?
        .get("/greeting", greeting)?
        .get("/slow", slow)?
        .get("/trailered", trailered)?;
    server.start()?;

    Ok(())
}
