extern crate rust_tag_server;
extern crate http;

use rust_tag_server::httpd::{WebServer, Handler, Router, Request};
use http::StatusCode;
use std::io::{Write, Error, Read};

struct Echo;

impl Handler for Echo {
    fn handle(&self, request: &mut Request) -> Result<(), Error> {

        let length = {
            if let Some(length) = request.get_request_header("Content-Length") {
                if let Ok(length) = length.parse::<i32>() {
                    length
                } else {
                    -1
                }
            } else {
                -1
            }
        };

        if length >= 0 {
            let mut vec = vec![b'\n'; length as usize];
            let buf = vec.as_mut_slice();
            request.reader.read_exact(buf)?;

            match request.send_preamble(StatusCode::OK, buf.len()) {
                Ok(()) => {
                    request.write(buf)?;
                }
                Err(a) => return Err(a)
            }

//        } else if length == 0 {
//            request.send_preamble(StatusCode::OK, 0)
        } else {
            request.send_preamble(StatusCode::BAD_REQUEST, 0)?;
        }

        Ok(())
    }
}

fn main() {
    let mut router = Router::new();
    router.add_route("/", "POST", Echo);

    let server = WebServer::new("127.0.0.1:8080", router, 100, 10000, |err| {eprintln!("{}", err)})
        .expect("Welp");

    server.run();
}
