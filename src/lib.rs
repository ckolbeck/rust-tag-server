extern crate core;
extern crate http;

mod threadpool;
mod request;
mod router;

pub mod httpd {
    use std::net::{TcpListener, ToSocketAddrs};
    use std::io::{Write, BufReader, BufWriter, Error};
    use std::sync::Arc;

    use threadpool::ThreadPool;

    pub use request::Request;
    pub use router::Router;
    pub use router::Handler;
    use request::{RETURN_NEWLINE, CONTENT_LENGTH, COLON, SPACE};
    use std::error::Error as StdError;


    const BAD_REQUEST: &str = "HTTP/1.1 400 BAD REQUEST\r\n\r\n";
    const SERVER_ERROR: &str = "HTTP/1.1 500 INTERNAL SERVER ERROR\r\n\r\n";
    const SERVICE_UNAVAILABLE: &str = "HTTP/1.1 503 SERVICE UNAVAILABLE\r\n\r\n";

    pub struct WebServer<L: Fn(&str) + Send + Sync + 'static> {
        listener: TcpListener,
        router: Arc<Router>,
        threadpool: ThreadPool,
        err_logger: Arc<L>,
    }

    impl<L: Fn(&str) + Send + Sync + 'static> WebServer<L> {
        pub fn new<A: ToSocketAddrs>(addr: A, router: Router, workers: usize, request_queue: usize, err_logger: L)
                                     -> Result<WebServer<L>, Error> {
            let listener = TcpListener::bind(addr)?;

            Ok(
                WebServer {
                    listener,
                    router: Arc::new(router),
                    threadpool: ThreadPool::new(workers, request_queue),
                    err_logger: Arc::new(err_logger),
                }
            )
        }

        pub fn run(self) {
            loop {
                let mut stream = match self.listener.accept() {
                    Ok((stream, _)) => stream,
                    Err(_) => continue
                };

                let mut err_stream = match stream.try_clone() {
                    Ok(cloned) => cloned,
                    Err(_) => continue,
                };

                let reader = match stream.try_clone() {
                    Ok(cloned) => cloned,
                    Err(_) => continue,
                };

                let writer = match stream.try_clone() {
                    Ok(cloned) => cloned,
                    Err(_) => continue,
                };

                let router = self.router.clone();

                let err_logger = self.err_logger.clone();

                let dispatched = self.threadpool.execute(move || {
                    let reader = BufReader::new(reader);
                    let writer = BufWriter::new(writer);

                    match Request::parse_request(reader, writer) {
                        Err(err) => {
                            let err = err.as_bytes();

                            let mut err_write_result = || {
                                err_stream.write(BAD_REQUEST.as_bytes())?;
                                err_stream.write(RETURN_NEWLINE)?;
                                err_stream.write(CONTENT_LENGTH.as_bytes())?;
                                err_stream.write(COLON)?;
                                err_stream.write(SPACE)?;
                                err_stream.write(err.len().to_string().as_bytes())?;
                                err_stream.write(RETURN_NEWLINE)?;
                                err_stream.write(err)?;

                                err_stream.flush()
                            };

                            if let Err(e) = err_write_result() {
                                err_logger(&format!("Failed to write 400: {}", e))
                            }

                            return;
                        }

                        Ok(mut request) => {
                            let mut handle_request = || {
                                let handle_result = match router.get_handler(&request.path, &request.verb) {
                                    Err(status_code) => {
                                        request.send_preamble(status_code, 0)
                                    }
                                    Ok(handler) => {
                                        match handler.handle(&mut request) {
                                            Ok(_) => request.flush(),
                                            Err(err) => Err(err)
                                        }
                                    }
                                };

                                if let Err(err) = handle_result {
                                    if !request.response_headers_sent() {
                                        let err = err.description().as_bytes();

                                        let mut err_write_result = || {
                                            err_stream.write(SERVER_ERROR.as_bytes())?;
                                            err_stream.write(RETURN_NEWLINE)?;
                                            err_stream.write(CONTENT_LENGTH.as_bytes())?;
                                            err_stream.write(COLON)?;
                                            err_stream.write(SPACE)?;
                                            err_stream.write(err.len().to_string().as_bytes())?;
                                            err_stream.write(RETURN_NEWLINE)?;
                                            err_stream.write(err)?;

                                            err_stream.flush()
                                        };

                                        return err_write_result();
                                    }
                                }

                                Ok(())
                            };

                            if let Err(err) = handle_request() {
                                err_logger(&format!("Failed to handle request: {}", err))
                            }
                        }
                    }
                });

                if !dispatched {
                    match stream.write(SERVICE_UNAVAILABLE.as_bytes()) {
                        Ok(_) => match stream.flush() {
                            Ok(_) => {},
                            Err(e) => (self.err_logger)(&format!("Failed to flush 503: {}", e)),
                        }
                        Err(e) => (self.err_logger)(&format!("Failed to write 503: {}", e)),
                    }
                }
            }
        }
    }
}