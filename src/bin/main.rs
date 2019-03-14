extern crate rust_tag_server;
extern crate http;
extern crate chrono;

#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate serde_json;

use rust_tag_server::httpd::{WebServer, Handler, Router, Request};
use http::StatusCode;
use std::io::{Write, Error, Read};
use std::sync::{Arc, RwLock};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::atomic::AtomicIsize;
use chrono::{DateTime, FixedOffset};

#[derive(Serialize, Deserialize)]
struct TagRequest {
    pub user: String,
    pub add: Vec<String>,
    pub remove: Vec<String>,
    pub timestamp: String,
}

#[derive(Serialize, Deserialize)]
struct TagResponse {
    pub user: String,
    pub tags: Vec<String>,
}

struct TagHandler {
    tag_store: TagStore,
}

const MISSING_BODY_ERROR: &str = "Request had no body";
const JSON_PARSE_ERROR: &str = "Couldn't parse request JSON";
const TS_PARSE_ERROR: &str = "Couldn't parse timestamp, expected zoned ISO 8601";

impl Handler for TagHandler {
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

            let tag_request: TagRequest = match serde_json::from_slice(buf) {
                Ok(tag_request) => tag_request,
                Err(_) => {
                    let err = JSON_PARSE_ERROR.as_bytes();
                    request.send_preamble(StatusCode::BAD_REQUEST, err.len())?;
                    request.write(err)?;
                    return Ok(())
                }
            };

            let ts = match tag_request.timestamp.parse::<DateTime<FixedOffset>>() {
                Ok(datetime) => datetime.timestamp_millis(),
                Err(_) => {
                    let err = TS_PARSE_ERROR.as_bytes();
                    request.send_preamble(StatusCode::BAD_REQUEST, err.len())?;
                    request.write(err)?;
                    return Ok(());
                },
            };

            for tag in tag_request.add {
                if !tag_request.remove.contains(&tag) {
                    self.tag_store.add_tag(&tag_request.user, &tag, ts);
                }
            }

            for tag in tag_request.remove {
                self.tag_store.remove_tag(&tag_request.user, &tag, ts)
            }

            let response = TagResponse {
                user: tag_request.user.clone(),
                tags: self.tag_store.tags_for_user(&tag_request.user)
            };

            let response = match serde_json::to_vec(&response) {
                Ok(response) => response,
                Err(e) => {
                    request.send_preamble(StatusCode::INTERNAL_SERVER_ERROR, buf.len())?;
                    return Err(e.into())
                },
            };

            match request.send_preamble(StatusCode::OK, response.len()) {
                Ok(()) => {
                    request.write(&response[..])?;
                }
                Err(a) => return Err(a)
            }
        } else {
            let err = MISSING_BODY_ERROR.as_bytes();
            request.send_preamble(StatusCode::BAD_REQUEST, err.len())?;
            request.write(err)?;
        }

        Ok(())
    }
}

fn main() {
    let mut router = Router::new();
    router.add_route("/api/tags", "POST", TagHandler{
        tag_store: TagStore::new()
    });

    let server = WebServer::new("127.0.0.1:8080", router, 100, 10000, |err| { eprintln!("{}", err) })
        .expect("Welp");

    server.run();
}

pub struct TagStore {
    store: RwLock<HashMap<String, Arc<RwLock<HashMap<String, Arc<AtomicIsize>>>>>>
}

impl TagStore {
    pub fn new() -> TagStore {
        TagStore {
            store: RwLock::new(HashMap::new()),
        }
    }

    pub fn tags_for_user(&self, user: &String) -> Vec<String> {
        match self.store.read().unwrap().get(user) {
            None => Vec::with_capacity(1),
            Some(tags) => {
                let user_tags = tags.read().unwrap();

                let mut tags = Vec::with_capacity(user_tags.len());
                for (tag, ts) in user_tags.iter() {
                    if ts.load(Ordering::Relaxed) > 0 {
                        tags.push(tag.clone());
                    }
                }
                tags
            }
        }
    }

    pub fn add_tag(&self, user: &String, tag: &String, ts: i64) {
        if ts > isize::max_value() as i64 {
            panic!("This program must be run on a 64bit system")
        }

        let ts = ts as isize;

        let tag_ts = match self.get_tag(user, tag, ts) {
            None => return,
            Some(tag_ts) => tag_ts,
        };

        loop {
            let old_ts = tag_ts.load(Ordering::Acquire).abs();
            if old_ts >= ts {
                break;
            }

            if ts == tag_ts.compare_and_swap(old_ts, ts, Ordering::AcqRel) {
                break;
            }
        }
    }

    pub fn remove_tag(&self, user: &String, tag: &String, ts: i64) {
        if ts > isize::max_value() as i64 {
            panic!("This program must be run on a 64bit system")
        }

        let ts = ts as isize;

        let tag_ts = match self.get_tag(user, tag, -ts) {
            None => return,
            Some(tag_ts) => tag_ts,
        };

        loop {
            let old_ts = tag_ts.load(Ordering::Acquire);
            if ts < old_ts.abs() {
                break;
            } else if ts == old_ts.abs() && old_ts < 0 {
                break;
            }

            if -ts == tag_ts.compare_and_swap(old_ts, -ts, Ordering::AcqRel) {
                break;
            }
        }
    }

    fn get_tag(&self, user: &String, tag: &String, ts: isize) -> Option<Arc<AtomicIsize>> {
        let user_map = {
            match self.store.read().unwrap().get(user) {
                None => None,
                Some(user_map) => Some(user_map.clone()),
            }
        };

        let user_map = match user_map {
            None => {
                let user_map = Arc::new(RwLock::new(HashMap::new()));
                self.store.write().unwrap().entry(user.clone())
                    .or_insert(user_map.clone());
                user_map
            }

            Some(user_map) => user_map,
        };

        let tag_ts = {
            match user_map.read().unwrap().get(tag) {
                None => None,
                Some(tag_ts) => Some(tag_ts.clone()),
            }
        };

        match tag_ts {
            None => {
                let tag_ts = Arc::new(AtomicIsize::new(ts));
                user_map.write().unwrap().entry(tag.clone()).or_insert(tag_ts.clone());
                Some(tag_ts)
            }
            Some(tag_ts) => Some(tag_ts),
        }
    }
}