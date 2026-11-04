//! Control/game-plane separation adapted from the pinned MIT native Java reference.
//! Source: horizon-rust/horizon 59d8cc38a2627f5b7aa0c96fc91b013df70c45fc,
//! crates/horizon-client/src/net.rs; assets/licenses/Native-Java-Reference-MIT.txt.
//! Keep-alives remain live while the player owner admits an ordered correction.
use std::collections::VecDeque;

/// Deferred decoded frames retain wire order without applying world state twice.
/// Capacity pauses socket reads; it never drops content or acknowledges player authority.
pub(super) struct Ingress {
    frames: VecDeque<Vec<u8>>,
    bytes: usize,
    capacity: usize,
}
impl Ingress {
    pub(super) fn new(capacity: usize) -> Self {
        Self {
            frames: VecDeque::new(),
            bytes: 0,
            capacity,
        }
    }
    pub(super) fn can_read(&self) -> bool {
        self.frames.len() < self.capacity && self.bytes < crate::stream::MAX_UNCOMPRESSED
    }
    pub(super) fn push(&mut self, frame: Vec<u8>) {
        // The stream bounds each frame. One last frame may cross the byte watermark;
        // subsequent reads pause, retaining at most two maximum-sized frames in bytes.
        self.bytes += frame.len();
        self.frames.push_back(frame);
    }
    pub(super) fn pop(&mut self) -> Option<Vec<u8>> {
        let frame = self.frames.pop_front()?;
        self.bytes -= frame.len();
        Some(frame)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn backpressure_retains_fifo_frames_and_resumes_after_drain() {
        let mut q = Ingress::new(2);
        q.push(vec![1, 2]);
        q.push(vec![3]);
        assert!(!q.can_read());
        assert_eq!(q.pop(), Some(vec![1, 2]));
        assert!(q.can_read());
        q.push(vec![4, 5]);
        assert_eq!(q.pop(), Some(vec![3]));
        assert_eq!(q.pop(), Some(vec![4, 5]));
        assert_eq!(q.pop(), None);
    }
    #[test]
    fn byte_watermark_does_not_discard_a_large_valid_frame() {
        let mut q = Ingress::new(8);
        q.push(vec![7; crate::stream::MAX_UNCOMPRESSED - 1]);
        assert!(q.can_read());
        q.push(vec![8; 4]);
        assert!(!q.can_read());
        assert_eq!(q.pop().unwrap().len(), crate::stream::MAX_UNCOMPRESSED - 1);
        assert!(q.can_read());
        assert_eq!(q.pop(), Some(vec![8; 4]));
    }
}
