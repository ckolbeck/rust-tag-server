extern crate core;

use std::thread;
use std::sync::{mpsc, Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::TryRecvError;

pub struct ThreadPool {
    workers: Vec<Worker>,
    queue: mpsc::SyncSender<Job>,
    should_run: Arc<AtomicBool>,
}

impl ThreadPool {
    pub fn new(threads: usize, queue_len: usize) -> ThreadPool {
        assert!(threads > 0);
        assert!(queue_len > 0);

        let (sender, receiver) = mpsc::sync_channel(queue_len);
        let receiver = Arc::new(Mutex::new(receiver));
        let should_run = Arc::new(AtomicBool::new(true));

        let mut workers = Vec::with_capacity(threads);

        for _ in 0..threads {
            workers.push(Worker::new(should_run.clone(), Arc::clone(&receiver)))
        }

        ThreadPool {
            workers,
            queue: sender,
            should_run,
        }
    }

    pub fn execute<F>(&self, func: F) -> bool
        where F: FnOnce() + Send + 'static
    {
        let job = Box::new(func);
        match self.queue.send(job) {
            Ok(_) => true,
            Err(_) => false,
        }
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        self.should_run.store(false, Ordering::Relaxed);

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

struct Worker {
    thread: thread::JoinHandle<()>
}

impl Worker {
    fn new(should_run: Arc<AtomicBool>, queue: Arc<Mutex<mpsc::Receiver<Job>>>) -> Worker {
        let thread = thread::spawn(move || {
            loop {
                let job = {
                    queue.lock().unwrap().try_recv()
                };

                match job {
                    Ok(job) => job.call_box(),
                    Err(TryRecvError::Disconnected) => return,
                    Err(TryRecvError::Empty) => {
                        if !should_run.load(Ordering::Relaxed) {
                            return;
                        }
                    }
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
