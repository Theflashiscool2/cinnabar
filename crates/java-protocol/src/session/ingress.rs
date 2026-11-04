//! Control/game-plane separation adapted from the pinned MIT native Java reference.
//! Source: horizon-rust/horizon 59d8cc38a2627f5b7aa0c96fc91b013df70c45fc,
//! crates/horizon-client/src/net.rs; assets/licenses/Native-Java-Reference-MIT.txt.
//! Keep-alives remain live while the player owner admits an ordered correction.
use std::collections::VecDeque;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Frame {
    pub(super) bytes: Vec<u8>,
    pub(super) keepalive_replied: bool,
    pub(super) boundary: bool,
}
impl Frame {
    pub(super) fn new(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            keepalive_replied: false,
            boundary: false,
        }
    }
    fn cost(&self) -> usize {
        self.bytes.capacity() + std::mem::size_of::<Self>()
    }
}

/// Deferred decoded frames retain wire order without applying world state twice.
/// Capacity pauses socket reads; it never drops content or acknowledges player authority.
pub(super) struct Ingress {
    frames: VecDeque<Frame>,
    bytes: usize,
    budget: usize,
    boundary: bool,
}
impl Ingress {
    pub(super) fn new(budget: usize) -> Self {
        Self {
            frames: VecDeque::new(),
            bytes: 0,
            budget,
            boundary: false,
        }
    }
    pub(super) fn can_read(&self) -> bool {
        !self.boundary && self.bytes < self.budget
    }
    pub(super) fn push(&mut self, frame: Frame) {
        // Account payload capacity and record overhead, not the UI channel's frame count.
        // One final bounded frame may cross the watermark; then reads pause. The deque's
        // spare record slots are additionally bounded by its geometric allocation growth.
        self.bytes += frame.cost();
        self.boundary |= frame.boundary;
        self.frames.push_back(frame);
    }
    pub(super) fn pop(&mut self) -> Option<Frame> {
        let frame = self.frames.pop_front()?;
        self.bytes -= frame.cost();
        if frame.boundary {
            self.boundary = false;
        }
        Some(frame)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn backpressure_retains_fifo_frames_and_resumes_after_drain() {
        let cost = std::mem::size_of::<Frame>();
        let mut q = Ingress::new(cost * 2 + 3);
        q.push(Frame::new(vec![1, 2]));
        q.push(Frame::new(vec![3]));
        assert!(!q.can_read());
        assert_eq!(q.pop().unwrap().bytes, vec![1, 2]);
        assert!(q.can_read());
        q.push(Frame::new(vec![4, 5]));
        assert_eq!(q.pop().unwrap().bytes, vec![3]);
        assert_eq!(q.pop().unwrap().bytes, vec![4, 5]);
        assert_eq!(q.pop(), None);
    }
    #[test]
    fn byte_watermark_does_not_discard_a_large_valid_frame() {
        let mut q = Ingress::new(crate::stream::MAX_UNCOMPRESSED + std::mem::size_of::<Frame>());
        q.push(Frame::new(vec![7; crate::stream::MAX_UNCOMPRESSED - 1]));
        assert!(q.can_read());
        q.push(Frame::new(vec![8; 4]));
        assert!(!q.can_read());
        assert_eq!(
            q.pop().unwrap().bytes.len(),
            crate::stream::MAX_UNCOMPRESSED - 1
        );
        assert!(q.can_read());
        assert_eq!(q.pop().unwrap().bytes, vec![8; 4]);
    }
    #[test]
    fn codec_boundary_stops_read_ahead_until_consumed() {
        let mut q = Ingress::new(1024);
        q.push(Frame::new(vec![1]));
        q.push(Frame {
            bytes: vec![2],
            keepalive_replied: false,
            boundary: true,
        });
        assert!(!q.can_read());
        q.pop();
        assert!(!q.can_read());
        q.pop();
        assert!(q.can_read());
    }
}
