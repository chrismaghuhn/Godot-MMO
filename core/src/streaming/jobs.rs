//! Worker-Threads für die Hintergrund-Jobs (einziger Ort mit Threads im Streaming).
//!
//! Jobs laufen in der Reihenfolge, in der sie eingereiht werden. Abgebrochene Jobs, die noch nicht
//! laufen, werden übersprungen; Ergebnisse veralteter Jobs erkennt der Aufrufer an der Job-Nummer.
//! Ein Panic im Job kommt als `Err` zurück; der Aufrufer räumt auf und setzt ihn dann fort. Der
//! Empfänger liegt hinter einem `Mutex`, damit der Pool `Sync` ist (z. B. als Bevy-Ressource).

use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Art eines Hintergrund-Jobs (für Messungen, #9).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobKind {
    Column,
    Cave,
    Far,
}

type Work<R> = Box<dyn FnOnce() -> R + Send>;
/// Inhalt eines Panics im Job.
pub(super) type Panic = Box<dyn std::any::Any + Send>;
type Outcome<R> = Result<R, Panic>;
/// Was ein Worker zurückgibt: Art, Dauer und Ergebnis (oder Panic).
type Finished<R> = (JobKind, Duration, Outcome<R>);

struct Queued<R> {
    id: u64,
    cancelled: Arc<AtomicBool>,
    work: Work<R>,
}

struct Shared<R> {
    queue: Mutex<(VecDeque<Queued<R>>, bool)>,
    wake: Condvar,
}

/// Handle eines eingereihten Jobs: Nummer zum Zuordnen des Ergebnisses, Abbrechen beim Verwerfen.
pub(super) struct Ticket {
    pub(super) id: u64,
    cancelled: Arc<AtomicBool>,
}

impl Drop for Ticket {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

/// Ein fertiger Job (oder sein Panic).
pub(super) struct Done<R> {
    pub(super) id: u64,
    pub(super) kind: JobKind,
    pub(super) duration: Duration,
    pub(super) result: Outcome<R>,
}

pub(super) struct JobPool<R> {
    shared: Arc<Shared<Finished<R>>>,
    results: Mutex<Receiver<(u64, Finished<R>)>>,
    workers: Vec<JoinHandle<()>>,
    next_id: u64,
}

impl<R: Send + 'static> JobPool<R> {
    pub(super) fn new(threads: usize) -> Self {
        let shared = Arc::new(Shared { queue: Mutex::new((VecDeque::new(), false)), wake: Condvar::new() });
        let (sender, results) = channel();
        let workers = (0..threads.max(1))
            .map(|i| {
                let shared = Arc::clone(&shared);
                let sender: Sender<_> = sender.clone();
                std::thread::Builder::new()
                    .name(format!("terraforge-stream-{i}"))
                    .spawn(move || worker(&shared, &sender))
                    .expect("Streaming-Thread nicht gestartet")
            })
            .collect();
        Self { shared, results: Mutex::new(results), workers, next_id: 0 }
    }

    /// Reiht einen Job ein. Wird das Ticket verworfen, bevor der Job startet, läuft er nicht.
    pub(super) fn spawn(&mut self, kind: JobKind, work: impl FnOnce() -> R + Send + 'static) -> Ticket {
        self.next_id += 1;
        let cancelled = Arc::new(AtomicBool::new(false));
        let work: Work<Finished<R>> = Box::new(move || {
            let start = Instant::now();
            let outcome = catch_unwind(AssertUnwindSafe(work));
            (kind, start.elapsed(), outcome)
        });
        let queued = Queued { id: self.next_id, cancelled: Arc::clone(&cancelled), work };
        self.shared.queue.lock().expect("Job-Queue vergiftet").0.push_back(queued);
        self.shared.wake.notify_one();
        Ticket { id: self.next_id, cancelled }
    }

    /// Alle seit dem letzten Aufruf fertig gewordenen Jobs.
    pub(super) fn poll(&mut self) -> Vec<Done<R>> {
        self.results
            .get_mut()
            .expect("Job-Ergebnisse vergiftet")
            .try_iter()
            .map(|(id, (kind, duration, result))| Done { id, kind, duration, result })
            .collect()
    }
}

fn worker<R>(shared: &Shared<R>, results: &Sender<(u64, R)>) {
    loop {
        let job = {
            let mut queue = shared.queue.lock().expect("Job-Queue vergiftet");
            loop {
                if queue.1 {
                    return;
                }
                if let Some(job) = queue.0.pop_front() {
                    break job;
                }
                queue = shared.wake.wait(queue).expect("Job-Queue vergiftet");
            }
        };
        if job.cancelled.load(Ordering::Relaxed) {
            continue;
        }
        if results.send((job.id, (job.work)())).is_err() {
            return;
        }
    }
}

impl<R> Drop for JobPool<R> {
    fn drop(&mut self) {
        // Auch bei vergiftetem Mutex beenden, sonst wartet `join` ewig.
        self.shared.queue.lock().unwrap_or_else(PoisonError::into_inner).1 = true;
        self.shared.wake.notify_all();
        for worker in self.workers.drain(..) {
            worker.join().ok();
        }
    }
}
