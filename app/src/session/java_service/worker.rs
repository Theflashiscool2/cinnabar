//! Single-owner between-frame handoff. Reclaim wakes the worker and joins its current slice.
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::JoinHandle,
    time::Duration,
};
pub(super) trait Work: Send + 'static {
    fn service(&mut self);
}
pub(super) struct Worker<T: Work> {
    jobs: mpsc::SyncSender<T>,
    results: mpsc::Receiver<T>,
    reclaim: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    active: bool,
}
impl<T: Work> Worker<T> {
    pub(super) fn spawn() -> std::io::Result<Self> {
        let (jobs, input) = mpsc::sync_channel::<T>(1);
        let (output, results) = mpsc::sync_channel(1);
        let reclaim = Arc::new(AtomicBool::new(false));
        let requested = Arc::clone(&reclaim);
        let thread = std::thread::Builder::new()
            .name("java-game-service".into())
            .spawn(move || {
                while let Ok(mut authority) = input.recv() {
                    while !requested.load(Ordering::Acquire) {
                        authority.service();
                        std::thread::park_timeout(Duration::from_millis(5));
                    }
                    if output.send(authority).is_err() {
                        break;
                    }
                }
            })?;
        Ok(Self {
            jobs,
            results,
            reclaim,
            thread: Some(thread),
            active: false,
        })
    }
    pub(super) fn launch(&mut self, authority: T) {
        assert!(!self.active, "Java authority already lent");
        self.reclaim.store(false, Ordering::Release);
        self.jobs.send(authority).expect("Java service stopped");
        self.active = true;
    }
    /// True only between handing the actual authority to this worker and reclaiming it.
    pub(super) fn is_lent(&self) -> bool {
        self.active
    }
    pub(super) fn take(&mut self) -> Option<T> {
        if !self.active {
            return None;
        }
        self.reclaim.store(true, Ordering::Release);
        self.thread.as_ref().unwrap().thread().unpark();
        let result = self.results.recv().expect("Java service failed");
        self.active = false;
        Some(result)
    }
}
impl<T: Work> Drop for Worker<T> {
    fn drop(&mut self) {
        // Return/retire the actual session before stopping its service; no detached world.
        drop(self.take());
        let (closed, _) = mpsc::sync_channel(1);
        drop(std::mem::replace(&mut self.jobs, closed));
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Job {
        id: u64,
        slices: u64,
        started: mpsc::SyncSender<u64>,
        drops: Arc<std::sync::atomic::AtomicUsize>,
    }
    impl Work for Job {
        fn service(&mut self) {
            self.slices += 1;
            let _ = self.started.try_send(self.id);
        }
    }
    impl Drop for Job {
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::SeqCst);
        }
    }
    #[test]
    fn reclaim_returns_the_mutated_owner_and_reuses_only_one_worker() {
        let mut worker = Worker::spawn().unwrap();
        let drops = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (started, ready) = mpsc::sync_channel(1);
        let mut owner = Job {
            id: 71,
            slices: 0,
            started,
            drops: Arc::clone(&drops),
        };
        for _ in 0..32 {
            let before = owner.slices;
            worker.launch(owner);
            assert!(worker.is_lent());
            assert_eq!(ready.recv().unwrap(), 71);
            owner = worker.take().unwrap();
            assert!(!worker.is_lent());
            assert!(owner.slices > before);
            assert!(worker.take().is_none());
            assert_eq!(drops.load(Ordering::SeqCst), 0);
            while ready.try_recv().is_ok() {}
        }
        drop(owner);
        drop(worker);
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }
    #[test]
    fn shutdown_reclaims_and_drops_an_active_owner_once() {
        let mut worker = Worker::spawn().unwrap();
        let drops = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (started, ready) = mpsc::sync_channel(1);
        worker.launch(Job {
            id: 9,
            slices: 0,
            started,
            drops: Arc::clone(&drops),
        });
        assert_eq!(ready.recv().unwrap(), 9);
        drop(worker);
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }
}
