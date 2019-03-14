extern crate core;

use request::Request;
use std::collections::HashMap;
use std::sync::Arc;
use std::io::Error;
use http::StatusCode;

pub trait Handler: Send + Sync {
    fn handle(&self, request: &mut Request) -> Result<(), Error>;
}

pub struct Router {
    routes: HashMap<String, HashMap<String, Arc<Handler>>>
}

impl Router {
    pub fn new() -> Router {
        Router {
            routes: HashMap::new()
        }
    }

    pub fn  add_route<H: Handler + 'static>(&mut self, path: &str, verb: &str, handler: H) {
        let path = String::from(path);
        let verb = String::from(verb);

        let path_map = self.routes.entry(path).or_insert(HashMap::new());
        path_map.insert(String::from(verb), Arc::new(handler));
    }

    pub fn get_handler(&self, path: &String, verb: &String) -> Result<Arc<Handler>, StatusCode> {
        assert!(path.starts_with('/'), "Routes must be canonical, but got: {}", path);
        assert!(!verb.is_empty());

        let mut path= &path[..];
        while !path.is_empty() {
            if let Some(verb_map) = self.routes.get(path) {
                if let Some(handler) = verb_map.get(verb) {
                    return Ok(handler.clone());
                } else {
                    return Err(StatusCode::METHOD_NOT_ALLOWED)
                }
            }

            let splits: Vec<&str> = path.rsplitn(2, '/').collect();

            match splits.last() {
                None => break,
                Some(new_path) => path = new_path,
            }
        }

        Err(StatusCode::NOT_FOUND)
    }
}