extern crate core;

use std::thread;
use std::sync::{mpsc, Arc, Mutex};

pub struct ThreadPool {
    workers: Vec<Worker>,
    queue: mpsc::SyncSender<QueueItem>
}

impl ThreadPool {
    pub fn new(threads: usize, queue_len: usize) -> ThreadPool {
        assert!(threads > 0);
        assert!(queue_len > 0);

        let (sender, receiver) = mpsc::sync_channel(queue_len);
        let receiver = Arc::new(Mutex::new(receiver));

        let mut workers = Vec::with_capacity(threads);

        for _ in 0..threads {
            workers.push(Worker::new(Arc::clone(&receiver)))
        }

        ThreadPool {
            workers,
            queue: sender,
        }
    }

    pub fn execute<F>(&self, func: F) -> bool
        where F: FnOnce() + Send + 'static
    {
        let job = Box::new(func);
        match self.queue.send(QueueItem::Work(job)) {
            Ok(_) => true,
            Err(_) => false,
        }
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        for _ in self.workers.iter() {
            match self.queue.send(QueueItem::ShutdownSignal) {
                Ok(_) => {},
                Err(_) => eprintln!("Failed to dispatch shutdown signal"),
            }
        }

        while self.workers.len() > 0 {
            let worker = self.workers.pop().unwrap();
            worker.join();
        }
    }
}

trait FnBox {
    fn call_box(self: Box<Self>);
}

impl<F: FnOnce()> FnBox for F {
    fn call_box(self: Box<F>) {
        (*self)()
    }
}

type Job = Box<FnBox + Send + 'static>;

enum QueueItem {
    ShutdownSignal,
    Work(Job)
}

struct Worker {
    thread: thread::JoinHandle<()>
}

impl Worker {
    fn new(queue: Arc<Mutex<mpsc::Receiver<QueueItem>>>) -> Worker {
        let thread = thread::spawn(move || {
            loop {
                let job = {
                    queue.lock().unwrap().recv()
                };

                match job {
                    Ok(QueueItem::Work(job)) => job.call_box(),
                    Ok(QueueItem::ShutdownSignal) => return,
                    Err(_) => return,
                }
            }
        });

        Worker {
            thread
        }
    }

    fn join(self) {
        match self.thread.join() {
            Ok(_) => {}
            Err(_) => eprintln!("Failed to join worker thread"),
        }
    }
}
