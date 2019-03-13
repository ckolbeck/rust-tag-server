extern crate core;
extern crate http;

mod threadpool;
mod request;
mod router;

pub mod httpd {
    use std::net::{TcpListener, ToSocketAddrs, TcpStream};
    use std::io::{Error, Write, BufReader, BufWriter};
    use std::sync::Arc;

    use http::status::StatusCode;

    use threadpool::ThreadPool;

    pub use request::Request;
    pub use router::Router;
    pub use router::Handler;


    const BAD_REQUEST: &str = "HTTP/1.1 400 BAD REQUEST\r\n\r\n";
    const SERVICE_UNAVAILABLE: &str = "HTTP/1.1 503 SERVICE UNAVAILABLE\r\n\r\n";

    pub struct WebServer {
        listener: TcpListener,
        router: Arc<Router>,
        threadpool: ThreadPool,
    }

    impl WebServer {
        pub fn new<A: ToSocketAddrs>(addr: A, router: Router, workers: usize, request_queue: usize)
                                     -> Result<WebServer, Error> {

            let listener = TcpListener::bind(addr)?;

            Ok(
                WebServer {
                    listener,
                    router: Arc::new(router),
                    threadpool: ThreadPool::new(workers, request_queue),
                }
            )
        }

        pub fn run(self) {
            loop {
                let stream = match self.listener.accept() {
                    Ok((stream, _)) => stream,
                    Err(_) => continue
                };

                let stream = Arc::new(stream);
                let reader = stream.clone();
                let writer = stream.clone();
                let router = self.router.clone();

                let dispatched = self.threadpool.execute(move || {
                    let mut reader = BufReader::new(&*reader);
                    let mut writer = BufWriter::new(&*writer);

                    match WebServer::parse_request(reader) {
                        Err(_) => {
                            writer.write(BAD_REQUEST.as_bytes());
                            writer.flush();
                            return;
                        }
                        Ok(request) => {
                            match router.get_handler(request.path, request.verb) {
                                None => {
                                    request.begin_response(StatusCode::NOT_FOUND, 0);
                                    request.flush();
                                    return;
                                }
                                Some(handler) => {
                                    handler.handle(request);
                                    request.flush();
                                }
                            };
                        }
                    };

                });

                if !dispatched {
                    (&*stream).write(SERVICE_UNAVAILABLE.as_bytes());
                    (&*stream).flush();
                }
            }
        }

        fn parse_request<'a>(_reader: BufReader<&TcpStream>) -> Result<&mut Request<'a>, Error> {
            unimplemented!();
        }
    }
}