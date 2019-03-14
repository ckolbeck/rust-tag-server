extern crate regex;

use std::net::TcpStream;
use std::collections::HashMap;
use std::io::{Write, Error, BufReader, BufWriter, BufRead};
use http::StatusCode;


pub const SPACE: &[u8] = &[b' '];
pub const COLON: &[u8] = &[b':'];
pub const HTTP_VERSION: &str = "HTTP/1.1";
pub const CONTENT_LENGTH: &str = "Content-Length:";
pub const NEWLINE: &[u8] = &[b'\n'];
pub const RETURN_NEWLINE: &[u8] = &[b'\r', b'\n'];


pub struct Request {
    pub request_headers: HashMap<String, Vec<String>>,
    pub query_params: HashMap<String, Vec<String>>,
    pub path: String,
    pub verb: String,
    pub reader: BufReader<TcpStream>,
    writer: BufWriter<TcpStream>,
    response_headers: HashMap<String, Vec<String>>,
    response_headers_sent: bool,
}

impl<'a> Write for Request {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        if !self.response_headers_sent {
            panic!("Attempted to write body before begin_response called")
        }

        self.writer.write(buf)
    }

    fn flush(&mut self) -> Result<(), Error> {
        self.writer.flush()
    }
}

impl<'a> Request {
    fn new(
        request_headers: HashMap<String, Vec<String>>,
        query_params: HashMap<String, Vec<String>>,
        path: String,
        verb: String,
        reader: BufReader<TcpStream>,
        writer: BufWriter<TcpStream>,
        response_headers: HashMap<String, Vec<String>>,
        response_body_writeable: bool) -> Request {
        Request {
            request_headers,
            query_params,
            path,
            verb,
            reader,
            writer,
            response_headers,
            response_headers_sent: response_body_writeable,
        }
    }

    pub fn response_headers_sent(&self) -> bool {
        self.response_headers_sent
    }

    pub fn parse_request(reader: BufReader<TcpStream>, writer: BufWriter<TcpStream>) -> Result<Request, &'static str> {
        let mut reader = reader;
        let writer = writer;

        let mut request_line = String::new();
        if let Err(_) = reader.read_line(&mut request_line) {
            return Err("Couldn't read request line");
        }

        let mut request_parts: Vec<&str> = request_line.split_whitespace().collect();

        if request_parts.len() != 3 {
            return Err("Couldn't parse request line");
        }

        request_parts.pop(); //Discard http version

        let path_and_params = request_parts.pop().unwrap();
        let verb = request_parts.pop().unwrap();

        if !path_and_params.starts_with('/') {
            return Err("Request path must start with a '/'");
        }

        let mut path_and_params: Vec<&str> = path_and_params.splitn(2, '?').collect();
        let (path, query_params) = match path_and_params.len() {
            1 => (path_and_params.pop().unwrap(), HashMap::new()),
            2 => {
                let params = Request::parse_query_params(path_and_params.pop().unwrap());
                (path_and_params.pop().unwrap(), params)
            }
            len => panic!("Unexpected path and param split length {} from request line {}", len, request_line)
        };

        let request_headers: HashMap<String, Vec<String>> = match Request::parse_headers(&mut reader) {
            Ok(map) => map,
            Err(err) => return Err(err),
        };

        Ok(
            Request::new(
                request_headers,
                query_params,
                String::from(path),
                String::from(verb),
                reader,
                writer,
                HashMap::new(),
                false,
            )
        )
    }

    fn parse_query_params(params: &str) -> HashMap<String, Vec<String>> {
        let mut query_params = HashMap::new();
        for param_and_val in params.split('&') {
            let mut param_and_val: Vec<&str> = param_and_val.splitn(2, '=').collect();

            let value = match param_and_val.pop() {
                None => String::new(),
                Some(value) => String::from(value),
            };

            let param = match param_and_val.pop() {
                None => continue,
                Some(param) => String::from(param),
            };

            let values = query_params.entry(param).or_insert(Vec::new());
            values.push(value)
        };

        query_params
    }

    fn parse_headers(reader: &mut BufReader<TcpStream>) -> Result<HashMap<String, Vec<String>>, &'static str> {
        let mut headers = HashMap::new();
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(cnt) => if cnt == 0 {
                    break;
                },
                Err(_) => return Err("Error reading headers"),
            }

            if line.eq("\r\n") {
                break;
            }

            let mut header_and_value: Vec<&str> = line.splitn(2, ':').collect();

            let value = match header_and_value.pop() {
                Some(value) => String::from(value.trim()),
                None => return Err("Malformed header line"),
            };

            let header = match header_and_value.pop() {
                Some(header) => String::from(header.trim()),
                None => return Err("Malformed header line"),
            };

            if header.is_empty() {
                return Err("Empty header name");
            }

            let values = headers.entry(header).or_insert(Vec::new());
            values.push(value)
        }

        Ok(headers)
    }

    pub fn get_request_header(&self, header: &str) -> Option<&String> {
        self.request_headers.get(&String::from(header))
            .and_then(|v| { v.get(0) })
    }

    pub fn add_response_header(&mut self, header: &'a str, value: &'a str) {
        if self.response_headers_sent {
            panic!("Attempted to add header after begin_response called")
        }

        let headers = self.response_headers.entry(String::from(header)).or_insert(vec![]);
        headers.push(String::from(value));
    }

    pub fn send_preamble(&mut self, code: StatusCode, body_size: usize) -> Result<(), Error> {
        if self.response_headers_sent {
            panic!("begin_response called twice!")
        }

        self.response_headers_sent = true;

        if self.response_headers.contains_key("Content-Length") {
            panic!("Attempted to add explicit Content-Length header!")
        }

        let writes: Result<usize, Error> = {
            self.writer.write(HTTP_VERSION.as_bytes())?;
            self.writer.write(SPACE)?;
            self.writer.write(code.as_str().as_bytes())?;
            self.writer.write(SPACE)?;
            self.writer.write(code.canonical_reason().unwrap_or("UNKNOWN").as_bytes())?;
            self.writer.write(RETURN_NEWLINE)?;

            for (header, values) in self.response_headers.iter() {
                for value in values.iter() {
                    self.writer.write(header.as_bytes())?;
                    self.writer.write(COLON)?;
                    self.writer.write(SPACE)?;
                    self.writer.write(value.as_bytes())?;
                    self.writer.write(NEWLINE)?;
                }
            }

            self.writer.write(CONTENT_LENGTH.as_bytes())?;
            self.writer.write(SPACE)?;
            self.writer.write(body_size.to_string().as_bytes())?;
            self.writer.write(RETURN_NEWLINE)?;
            self.writer.write(RETURN_NEWLINE)?;
            self.writer.flush()?;

            Ok(0)
        };

        match writes {
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        }
    }
}
