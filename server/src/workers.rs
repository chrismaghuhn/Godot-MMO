//! Threads, die Chunk-Anfragen berechnen (gemeinsam für alle Verbindungen).

use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;

type Job = Box<dyn FnOnce() + Send>;

pub struct Workers {
    sender: Option<Sender<Job>>,
    threads: Vec<JoinHandle<()>>,
}

impl Workers {
    pub fn new(count: usize) -> Self {
        let (sender, receiver) = channel::<Job>();
        let receiver = Arc::new(Mutex::new(receiver));
        let threads = (0..count.max(1))
            .map(|i| {
                let receiver = Arc::clone(&receiver);
                std::thread::Builder::new()
                    .name(format!("terraforge-server-{i}"))
                    .spawn(move || {
                        loop {
                            let job = receiver.lock().unwrap_or_else(PoisonError::into_inner).recv();
                            match job {
                                Ok(job) => job(),
                                Err(_) => return,
                            }
                        }
                    })
                    .expect("Server-Thread nicht gestartet")
            })
            .collect();
        Self { sender: Some(sender), threads }
    }

    pub fn run(&self, job: impl FnOnce() + Send + 'static) {
        if let Some(sender) = &self.sender {
            sender.send(Box::new(job)).ok();
        }
    }
}

impl Drop for Workers {
    fn drop(&mut self) {
        self.sender = None;
        for thread in self.threads.drain(..) {
            thread.join().ok();
        }
    }
}
