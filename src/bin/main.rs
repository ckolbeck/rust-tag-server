extern crate rust_tag_server;
extern crate http;

use rust_tag_server::httpd::{WebServer, Handler, Router, Request};
use http::StatusCode;
use std::io::{Read, Write};

struct Echo;

impl Handler for Echo {
    fn handle(&self, request: &mut Request) {
        let mut request_body = Vec::new();

        match request.reader.read_to_end(&mut request_body) {
            Ok(_) => {},
            Err(_) => {
                request.begin_response(StatusCode::BAD_REQUEST, 0);
                return;
            },
        }

        if let Ok(()) = request.begin_response(StatusCode::OK, request_body.len()) {
            request.write(request_body.as_slice());
        }
    }
}

fn main() {
    let mut router = Router::new();
    router.add_route("GET", "/", Echo);

    let server = WebServer::new("127.0.0.1:8080", router, 100, 10000)
        .expect("Welp");

    server.run();
}
