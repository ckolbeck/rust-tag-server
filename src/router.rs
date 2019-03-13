extern crate core;

use request::Request;
use std::collections::HashMap;
use std::sync::Arc;

pub trait Handler: Send + Sync {
    fn handle(&self, request: &mut Request);
}

pub struct Router {
    routes: HashMap<&'static str, HashMap<&'static str, Arc<Handler>>>
}

impl Router {
    pub fn new() -> Router {
        Router {
            routes: HashMap::new()
        }
    }

    pub fn  add_route<H: Handler + 'static>(&mut self, path: &'static str, verb: &'static str, handler: H) {
        let verb_map = self.routes.entry(verb).or_insert(HashMap::new());
        verb_map.insert(path, Arc::new(handler));
    }

    pub fn get_handler(&self, path: &'static str, verb: &'static str) -> Option<Arc<Handler>> {
        assert!(path.starts_with('/'), "Routes must be canonical");
        assert!(!verb.is_empty());

        let path_map = self.routes.get(verb)?;
        let mut path= path;

        while !path.is_empty() {
            if let Some(handler) = path_map.get(path) {
                return Some(handler.clone())
            }

            let splits: Vec<&str> = path.rsplitn(2, '/').collect();

            match splits.last() {
                None => break,
                Some(new_path) => path = new_path,
            }
        }

        None
    }
}