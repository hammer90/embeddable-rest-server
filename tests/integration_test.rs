mod common;
use common::start_server;

use curl::easy::Easy;

#[test]
fn not_found() {
    let _server = start_server(vec![]);

    let mut dst = Vec::new();
    let mut easy = Easy::new();
    easy.url("localhost:8080/panic").unwrap();

    let mut transfer = easy.transfer();
    transfer
        .write_function(|data| {
            dst.extend_from_slice(data);
            println!("{:?}", data);
            Ok(data.len())
        })
        .unwrap();
    transfer.perform().unwrap();
}
