//! One admitted preparation worker across world/resource replacements.
use render_model::PreparedBlockEntityArtwork;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
pub(super) type Reply = ([u8; 32], Result<PreparedBlockEntityArtwork, String>);
static BUSY: AtomicBool = AtomicBool::new(false);
pub(super) struct Lease;
impl Lease {
    pub fn acquire() -> Option<Self> {
        BUSY.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| Self)
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        BUSY.store(false, Ordering::Release);
    }
}
pub(super) struct Pending {
    receiver: crossbeam_channel::Receiver<Reply>,
    cancel: Arc<AtomicBool>,
    pub identity: [u8; 32],
}
impl Pending {
    pub fn new(
        receiver: crossbeam_channel::Receiver<Reply>,
        cancel: Arc<AtomicBool>,
        identity: [u8; 32],
    ) -> Self {
        Self {
            receiver,
            cancel,
            identity,
        }
    }
    pub fn try_recv(&self) -> Result<Reply, crossbeam_channel::TryRecvError> {
        self.receiver.try_recv()
    }
}
impl Drop for Pending {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resource_retirement_cancels_and_rejects_obsolete_publication() {
        let (tx, rx) = crossbeam_channel::bounded(1);
        let cancel = Arc::new(AtomicBool::new(false));
        let pending = Pending::new(rx, Arc::clone(&cancel), [7; 32]);
        drop(pending);
        assert!(cancel.load(Ordering::Acquire));
        assert!(tx.send(([7; 32], Err("obsolete".into()))).is_err());
    }
    #[test]
    fn replacement_cannot_spawn_another_worker_until_retired_work_finishes() {
        let first = Lease::acquire().unwrap();
        assert!(Lease::acquire().is_none());
        drop(first);
        assert!(Lease::acquire().is_some());
    }
}
