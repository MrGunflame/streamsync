use std::collections::BTreeSet;

/// A fixed size ordered queue.
pub struct SegmentQueue<S>
where
    S: Ord,
{
    queue: BTreeSet<S>,
    size: usize,
}

impl<S> SegmentQueue<S>
where
    S: Ord,
{
    pub fn new(size: usize) -> Self {
        Self {
            queue: BTreeSet::new(),
            size,
        }
    }

    pub fn push(&mut self, segment: S) {
        if self.queue.len() == self.size {
            return;
        }

        self.queue.insert(segment);
    }

    pub fn peek(&mut self) -> Option<&'_ S> {
        self.queue.first()
    }

    pub fn pop(&mut self) -> Option<S> {
        self.queue.pop_first()
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn capacity(&self) -> usize {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn clear(&mut self) {
        self.queue.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::SegmentQueue;

    #[test]
    fn test_queue() {
        let mut queue = SegmentQueue::new(8192);
        queue.push(5);

        assert_eq!(*queue.peek().unwrap(), 5);

        queue.push(6);
        queue.push(7);
        queue.push(8);
        assert_eq!(*queue.peek().unwrap(), 5);

        queue.push(4);
        assert_eq!(*queue.peek().unwrap(), 4);

        for val in [4, 5, 6, 7, 8] {
            assert_eq!(queue.pop().unwrap(), val);
        }

        assert_eq!(queue.peek(), None);
        assert_eq!(queue.pop(), None);
        assert_eq!(queue.len(), 0);
        assert!(queue.is_empty());
    }
}
