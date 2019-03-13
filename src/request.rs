use std::net::TcpStream;
use std::collections::HashMap;
use std::io::{Write, Error, BufReader, BufWriter};
use http::StatusCode;


const SPACE: &[u8] = &[b' '];
const COLON: &[u8] = &[b':'];
const NEWLINE: &[u8] = &[b'\n'];
const HTTP_VERSION: &str = "HTTP/1.1";
const CONTENT_LENGTH: &str = "Content-Length:";


pub struct Request<'a> {
    pub request_headers: HashMap<&'a str, Vec<&'a str>>,
    pub query_params: HashMap<&'a str, Vec<&'a str>>,
    pub path: &'a str,
    pub verb: &'a str,
    pub reader: BufReader<TcpStream>,
    writer: BufWriter<TcpStream>,
    stream: TcpStream,
    response_headers: HashMap<&'a str, Vec<&'a str>>,
    response_body_writeable: bool,
}

impl<'a> Write for Request<'a> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        if !self.response_body_writeable {
            panic!("Attempted to write body before begin_response called")
        }

        self.writer.write(buf)
    }

    fn flush(&mut self) -> Result<(), Error> {
        self.writer.flush()
    }
}

impl<'a> Request<'a> {
    pub fn add_response_header(&mut self, header: &'a str, value: &'a str) {
        if self.response_body_writeable {
            panic!("Attempted to add header after begin_response called")
        }

        let headers = self.response_headers.entry(header).or_insert(vec![]);
        headers.push(value);
    }

    pub fn begin_response(&mut self, code: StatusCode, body_size: usize) -> Result<(), Error> {
        if self.response_body_writeable {
            panic!("begin_response called twice!")
        }

        if self.response_headers.contains_key("Content-Length") {
            panic!("Attempted to add explicit Content-Length header!")
        }

        let writes: Result<usize, Error> = {
            self.stream.write(HTTP_VERSION.as_bytes())?;
            self.stream.write(SPACE)?;
            self.stream.write(code.as_str().as_bytes())?;
            self.stream.write(SPACE)?;
            self.stream.write(code.canonical_reason().unwrap_or("UNKNOWN").as_bytes())?;
            self.stream.write(NEWLINE)?;

            for (header, values) in self.response_headers.iter() {
                for value in values.iter() {
                    self.stream.write(header.as_bytes())?;
                    self.stream.write(COLON)?;
                    self.stream.write(SPACE)?;
                    self.stream.write(value.as_bytes())?;
                    self.stream.write(NEWLINE)?;
                }
            }

            if body_size > 0 {
                self.stream.write(CONTENT_LENGTH.as_bytes())?;
                self.stream.write(SPACE)?;
                self.stream.write(body_size.to_string().as_bytes())?;
                self.stream.write(NEWLINE)
            } else {
                Ok(0)
            }
        };

        match writes {
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        }
    }
}
