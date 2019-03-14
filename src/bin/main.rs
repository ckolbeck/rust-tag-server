extern crate rust_tag_server;
extern crate http;

use rust_tag_server::httpd::{WebServer, Handler, Router, Request};
use http::StatusCode;
use std::io::{Write, Error, Read};
use tag_store::TagStore;
use std::sync::{Arc, RwLock};
use std::collections::HashMap;
use std::sync::atomic::Ordering;

struct Echo(TagStore);

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

        } else {
            request.send_preamble(StatusCode::BAD_REQUEST, 0)?;
        }

        Ok(())
    }
}



fn main() {
    let mut router = Router::new();
    router.add_route("/", "POST", Echo(TagStore::new()));

    let server = WebServer::new("127.0.0.1:8080", router, 100, 10000, |err| {eprintln!("{}", err)})
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

    pub fn add_tag(&mut self, user: &String, tag: &String, ts: u64) {
        assert!(ts as isize > 0);

        let ts = ts as isize;
        let tag_ts = match self.get_tag(user, tag, ts) {
            None => return,
            Some(tag_ts) => tag_ts,
        };

        loop {
            let old_ts = tag_ts.load(Ordering::Acquire).abs();
            if old_ts >= ts {
                break;
                ;
            }

            if ts == tag_ts.compare_and_swap(old_ts, ts, Ordering::AcqRel) {
                break;
            }
        }
    }

    pub fn remove_tag(&mut self, user: &String, tag: &String, ts: u64) {
        assert!(ts as isize > 0);

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

    fn get_tag(&mut self, user: &String, tag: &String, ts: isize) -> Option<Arc<AtomicIsize>> {
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